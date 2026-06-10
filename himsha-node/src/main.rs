use anyhow::Result;
use himsha_node::{
    bitcoin_indexer::BitcoinIndexer, block_producer::BlockProducer, rpc::HimshaRpcServer,
    state::NodeState,
};
use himsha_runtime::{
    account::{AccountInfo, StoredAccount},
    pubkey::Pubkey,
    transaction::{Block, RuntimeTransaction},
    utxo::UtxoInfo,
};
use himsha_vm::{
    executor::{ExecutionInput, ProgramExecutor},
    registry::ProgramRegistry,
};
use jsonrpsee::{core::RpcResult, server::Server, types::ErrorObjectOwned};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
};
use tokio::sync::mpsc;
use tower_http::cors::{Any, CorsLayer};
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

// ---- RPC implementation ----

/// How many slots back a transaction's `recent_blockhash` may reference before it
/// expires. Bounds the replay-protection window.
const MAX_BLOCKHASH_AGE: u64 = 150;

/// Deterministic chain id from a Bitcoin network name, so clients on the same
/// network agree without a handshake.
fn chain_id_for_network(network: &str) -> u64 {
    match network {
        "mainnet" | "bitcoin" => 1,
        "testnet" => 2,
        "signet" => 3,
        _ => 4, // regtest / default
    }
}

struct HimshaNode {
    state: NodeState,
    registry: Arc<Mutex<ProgramRegistry>>,
    pending_tx: mpsc::Sender<RuntimeTransaction>,
    election: Arc<Mutex<himsha_node::election::ElectionState>>,
    /// Network this node accepts transactions for (see [`chain_id_for_network`]).
    chain_id: u64,
    /// FROST/Taproot settlement custody, when `HIMSHA_THRESHOLD` is configured;
    /// `None` falls back to single-hot-wallet settlement.
    custody: Option<Arc<himsha_node::custody::Custody>>,
}

impl HimshaNode {
    /// Drain the lending program's queued settlements. Repayments to a BOLT-11
    /// invoice are paid over **Lightning** (instant/cheap) when LND is configured;
    /// everything else settles on-chain via the Bitcoin indexer. Followers reuse the
    /// no-payment `settlement::drain_lending` instead.
    async fn settle_lending(&self, accounts: &mut [AccountInfo], himsha_txid: [u8; 32]) {
        use himsha_lending_program::{take_settlements, CollectionAccount, SettlementKind};
        use himsha_node::lightning::{is_invoice, LightningClient};

        let indexer = BitcoinIndexer::from_env();
        let lightning = LightningClient::from_env();

        for account in accounts.iter_mut() {
            let mut coll: CollectionAccount = match account.read_data() {
                Ok(c) => c,
                Err(_) => continue,
            };
            if coll.pending_settlements.is_empty() {
                continue;
            }
            for s in take_settlements(&mut coll) {
                // Lightning fast-path: a repayment addressed to a BOLT-11 invoice.
                if matches!(s.kind, SettlementKind::Repayment) && is_invoice(&s.recipient) {
                    match &lightning {
                        Some(ln) => match ln.pay_invoice(&s.recipient).await {
                            Ok(h) => info!(
                                "settled {} over Lightning (payment_hash={h})",
                                s.inscription_id
                            ),
                            Err(e) => {
                                error!("Lightning repayment for {} failed: {e}", s.inscription_id)
                            }
                        },
                        None => info!(
                            "repayment for {} is a Lightning invoice but LND is not configured",
                            s.inscription_id
                        ),
                    }
                    continue;
                }
                // On-chain settlement for the rest.
                match &indexer {
                    Some(ix) => {
                        let r = match s.kind {
                            SettlementKind::Repayment => ix.send_payment(&s.recipient, s.amount),
                            SettlementKind::ReturnInscription
                            | SettlementKind::SeizeInscription => {
                                let txid_hex = hex::encode(s.utxo.txid);
                                // Threshold custody (when configured) signs the UTXO
                                // move with the FROST committee instead of the hot wallet.
                                match &self.custody {
                                    Some(c) => ix.transfer_utxo_committee(
                                        &c.committee,
                                        &txid_hex,
                                        s.utxo.vout,
                                        &s.recipient,
                                    ),
                                    None => ix.transfer_utxo(&txid_hex, s.utxo.vout, &s.recipient),
                                }
                            }
                        };
                        match r {
                            Ok(txid) => {
                                info!(
                                    "settled {:?} for {} via bitcoin tx {txid}",
                                    s.kind, s.inscription_id
                                );
                                // Index bitcoin_txid -> himsha_txid (settlement lookup).
                                if let Ok(raw) = hex::decode(&txid) {
                                    if let Ok(btc) = <[u8; 32]>::try_from(raw.as_slice()) {
                                        let _ = self.state.index_btc_settlement(&btc, &himsha_txid);
                                    }
                                }
                            }
                            Err(e) => error!(
                                "settlement {:?} for {} failed: {e}",
                                s.kind, s.inscription_id
                            ),
                        }
                    }
                    None => info!(
                        "settlement {:?} for {} queued (no Bitcoin RPC)",
                        s.kind, s.inscription_id
                    ),
                }
            }
            if account.write_data(&coll).is_err() {
                error!("failed to clear lending settlements for {}", account.key);
            }
        }
    }

    /// Hex of the block hash at `slot`, if that block exists.
    fn block_hash_hex(&self, slot: u64) -> Option<String> {
        let bytes = self.state.load_block(slot).ok().flatten()?;
        let block: Block = serde_json::from_slice(&bytes).ok()?;
        Some(hex::encode(block.blockhash))
    }
}

/// Bitcoin network for address derivation (`HIMSHA_NETWORK`, default regtest).
fn bitcoin_network() -> bitcoin::Network {
    match std::env::var("HIMSHA_NETWORK").as_deref() {
        Ok("mainnet") | Ok("bitcoin") => bitcoin::Network::Bitcoin,
        Ok("testnet") => bitcoin::Network::Testnet,
        Ok("signet") => bitcoin::Network::Signet,
        _ => bitcoin::Network::Regtest,
    }
}

/// Faucet gate: requires `HIMSHA_FAUCET=1` and enforces a per-request cap
/// (`HIMSHA_FAUCET_MAX`, default 1_000_000_000 lamports).
fn faucet_guard(lamports: u64) -> Result<(), ErrorObjectOwned> {
    if std::env::var("HIMSHA_FAUCET").as_deref() != Ok("1") {
        return Err(ErrorObjectOwned::owned(
            -32020,
            "faucet disabled (set HIMSHA_FAUCET=1)",
            None::<()>,
        ));
    }
    let max = std::env::var("HIMSHA_FAUCET_MAX")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(1_000_000_000);
    if lamports > max {
        return Err(ErrorObjectOwned::owned(
            -32029,
            format!("amount exceeds faucet cap {max}"),
            None::<()>,
        ));
    }
    Ok(())
}

#[jsonrpsee::core::async_trait]
impl himsha_node::rpc::HimshaRpcServer for HimshaNode {
    async fn send_transaction(&self, tx: RuntimeTransaction) -> RpcResult<String> {
        // 1. Verify every signer's BIP-340 Schnorr signature over the message hash.
        if !tx.verify_signatures() {
            return Err(ErrorObjectOwned::owned(
                -32000,
                "invalid transaction signature",
                None::<()>,
            ));
        }

        // 2. Replay protection: the chain id must match and the recent_blockhash must
        //    still be within the node's recent window (it expires once it ages out).
        let valid = self
            .state
            .recent_blockhashes(MAX_BLOCKHASH_AGE)
            .map_err(|e| {
                ErrorObjectOwned::owned(-32603, format!("state error: {e}"), None::<()>)
            })?;
        if let Err(reason) = tx.check_chain_and_blockhash(self.chain_id, &valid) {
            return Err(ErrorObjectOwned::owned(-32001, reason, None::<()>));
        }

        // 3. Reject a transaction that has already been processed (replay within the
        //    validity window). Caught for any tx already included in a block.
        let txid = tx.message_hash();
        if self.state.tx_slot(&txid).ok().flatten().is_some() {
            return Err(ErrorObjectOwned::owned(
                -32002,
                "duplicate transaction",
                None::<()>,
            ));
        }

        let tx_id = hex::encode(txid);

        // 4. Execute instructions in order. Each instruction gets exactly the
        //    accounts it declares, in the declared order — programs index
        //    `accounts[0]`, `accounts[1]`… positionally, so order matters.
        //    Accounts that don't exist yet (e.g. a pool being initialized) are
        //    materialized as empty accounts owned by the invoked program.
        //
        //    Writes are staged into an in-memory `overlay` rather than persisted
        //    per-instruction, so a later instruction sees earlier writes while the
        //    whole transaction commits atomically (or not at all) at the end.
        let mut overlay: HashMap<Pubkey, StoredAccount> = HashMap::new();
        for instr in &tx.message.instructions {
            let mut accounts: Vec<AccountInfo> = Vec::with_capacity(instr.accounts.len());
            for meta in &instr.accounts {
                let mut account = if let Some(stored) = overlay.get(&meta.pubkey) {
                    stored.clone().into_account(meta.pubkey)
                } else if let Ok(Some(stored)) = self.state.load_account(&meta.pubkey) {
                    stored.into_account(meta.pubkey)
                } else {
                    AccountInfo::new(meta.pubkey, instr.program_id, 0, 0)
                };
                // Propagate the signer flag so programs can enforce authority. The
                // per-signer Schnorr verification above is the node's cryptographic gate.
                account.is_signer = meta.is_signer;
                // Propagate writability so a program can't mutate an account the
                // instruction declared read-only (enforced by AccountInfo::write_data).
                account.is_writable = meta.is_writable;
                accounts.push(account);
            }

            let input = ExecutionInput {
                accounts,
                instruction_data: instr.data.clone(),
                timestamp: tx.message.timestamp,
            };

            // Execute in a scope so the (non-Send) registry guard is dropped before
            // any `.await` (settlement may pay over Lightning asynchronously).
            let result = {
                let registry_guard = self.registry.lock().unwrap();
                let executor = ProgramExecutor::new(&registry_guard);
                executor.execute_program(&instr.program_id, input, vec![])
            };

            match result {
                Ok(mut transition) => {
                    // Gate on the execution receipt: it must commit to exactly the
                    // accounts the program produced before any of them are persisted.
                    transition.verify().map_err(|reason| {
                        ErrorObjectOwned::owned(
                            -32004,
                            format!("invalid execution receipt: {reason}"),
                            None::<()>,
                        )
                    })?;

                    // Settle Ordinals loans: drain the lending program's queued
                    // settlements (return/seize inscription, pay lender) and move
                    // the UTXOs on Bitcoin. The cleared collection is then persisted.
                    if instr.program_id == himsha_runtime::program_ids::lending_program() {
                        self.settle_lending(&mut transition.updated_accounts, tx.message_hash())
                            .await;
                    }

                    // Stage updated accounts into the overlay (not yet persisted).
                    for account in &transition.updated_accounts {
                        overlay.insert(account.key, StoredAccount::from(account));
                    }
                }
                Err(e) => {
                    // Abort the whole transaction; the overlay is dropped, so nothing
                    // from any instruction is persisted (atomic rollback).
                    return Err(ErrorObjectOwned::owned(-32002, e.to_string(), None::<()>));
                }
            }
        }

        // 5. Commit every account the transaction touched in one write transaction.
        self.state
            .save_accounts_atomic(&overlay)
            .map_err(|e| ErrorObjectOwned::owned(-32001, e.to_string(), None::<()>))?;

        // 6. Queue for block inclusion
        let _ = self.pending_tx.send(tx).await;
        Ok(tx_id)
    }

    async fn get_account_info(&self, pubkey: String) -> RpcResult<Option<AccountInfo>> {
        let key = Pubkey::from_base58(&pubkey)
            .map_err(|e| ErrorObjectOwned::owned(-32003, e, None::<()>))?;
        Ok(self
            .state
            .load_account(&key)
            .map_err(|e| ErrorObjectOwned::owned(-32004, e.to_string(), None::<()>))?
            .map(|s| s.into_account(key)))
    }

    async fn get_program_accounts(&self, program_id: String) -> RpcResult<Vec<AccountInfo>> {
        let owner = Pubkey::from_base58(&program_id)
            .map_err(|e| ErrorObjectOwned::owned(-32009, e, None::<()>))?;
        self.state
            .accounts_by_owner(&owner)
            .map_err(|e| ErrorObjectOwned::owned(-32010, e.to_string(), None::<()>))
    }

    async fn deploy_program(&self, elf_hex: String, image_id_hex: String) -> RpcResult<String> {
        let elf = hex::decode(&elf_hex)
            .map_err(|e| ErrorObjectOwned::owned(-32005, e.to_string(), None::<()>))?;

        let image_id_bytes = hex::decode(&image_id_hex)
            .map_err(|e| ErrorObjectOwned::owned(-32006, e.to_string(), None::<()>))?;
        let mut image_id = [0u8; 32];
        image_id.copy_from_slice(&image_id_bytes);

        // Derive program ID from image_id
        let program_id = Pubkey::from(image_id);

        self.state
            .deploy_program(&program_id, &elf, image_id)
            .map_err(|e| ErrorObjectOwned::owned(-32007, e.to_string(), None::<()>))?;

        let mut reg = self.registry.lock().unwrap();
        reg.register(program_id, elf, image_id);

        info!("deployed program {}", program_id);
        Ok(program_id.to_string())
    }

    async fn get_block(&self, slot: u64) -> RpcResult<Option<Block>> {
        let bytes = self
            .state
            .load_block(slot)
            .map_err(|e| ErrorObjectOwned::owned(-32008, e.to_string(), None::<()>))?;
        Ok(bytes.and_then(|b| serde_json::from_slice(&b).ok()))
    }

    async fn get_slot(&self) -> RpcResult<u64> {
        self.state
            .current_slot()
            .map_err(|e| ErrorObjectOwned::owned(-32008, e.to_string(), None::<()>))
    }

    async fn is_node_ready(&self) -> RpcResult<bool> {
        Ok(true)
    }

    async fn list_programs(&self) -> RpcResult<Vec<String>> {
        let reg = self.registry.lock().unwrap();
        Ok(reg.list().iter().map(|p| p.to_string()).collect())
    }

    async fn get_utxo(&self, txid: String, vout: u32) -> RpcResult<Option<UtxoInfo>> {
        // Delegate to the Bitcoin indexer when RPC is configured; otherwise the
        // node has no view of the UTXO set, so report None.
        match BitcoinIndexer::from_env() {
            Some(indexer) => indexer
                .get_utxo(&txid, vout)
                .map_err(|e| ErrorObjectOwned::owned(-32011, e.to_string(), None::<()>)),
            None => Ok(None),
        }
    }

    async fn request_airdrop(&self, pubkey: String, lamports: u64) -> RpcResult<u64> {
        faucet_guard(lamports)?;
        let key = Pubkey::from_base58(&pubkey)
            .map_err(|e| ErrorObjectOwned::owned(-32021, e, None::<()>))?;
        // Load or create the account, credit lamports, persist.
        let mut account = self
            .state
            .load_account(&key)
            .map_err(|e| ErrorObjectOwned::owned(-32022, e.to_string(), None::<()>))?
            .map(|s| s.into_account(key))
            .unwrap_or_else(|| {
                AccountInfo::new(key, himsha_runtime::program_ids::system_program(), 0, 0)
            });
        account.lamports = account.lamports.saturating_add(lamports);
        let stored = StoredAccount::from(&account);
        self.state
            .save_account(&key, &stored)
            .map_err(|e| ErrorObjectOwned::owned(-32023, e.to_string(), None::<()>))?;
        Ok(account.lamports)
    }

    async fn create_account_with_faucet(
        &self,
        pubkey: String,
        lamports: u64,
        space: u64,
    ) -> RpcResult<AccountInfo> {
        faucet_guard(lamports)?;
        let key = Pubkey::from_base58(&pubkey)
            .map_err(|e| ErrorObjectOwned::owned(-32026, e, None::<()>))?;
        // Account creation: must not already exist.
        if self.state.account_exists(&key) {
            return Err(ErrorObjectOwned::owned(
                -32027,
                "account already exists",
                None::<()>,
            ));
        }
        let account = AccountInfo::new(
            key,
            himsha_runtime::program_ids::system_program(),
            lamports,
            space as usize,
        );
        self.state
            .save_account(&key, &StoredAccount::from(&account))
            .map_err(|e| ErrorObjectOwned::owned(-32028, e.to_string(), None::<()>))?;
        Ok(account)
    }

    async fn get_multiple_accounts(
        &self,
        pubkeys: Vec<String>,
    ) -> RpcResult<Vec<Option<AccountInfo>>> {
        let mut out = Vec::with_capacity(pubkeys.len());
        for p in pubkeys {
            let key = Pubkey::from_base58(&p)
                .map_err(|e| ErrorObjectOwned::owned(-32024, e, None::<()>))?;
            let info = self
                .state
                .load_account(&key)
                .map_err(|e| ErrorObjectOwned::owned(-32025, e.to_string(), None::<()>))?
                .map(|s| s.into_account(key));
            out.push(info);
        }
        Ok(out)
    }

    async fn get_processed_transaction(
        &self,
        txid: String,
    ) -> RpcResult<Option<RuntimeTransaction>> {
        // Fast path: resolve the slot from the tx index (O(1)), then read that one block.
        if let Ok(raw) = hex::decode(&txid) {
            if let Ok(id) = <[u8; 32]>::try_from(raw.as_slice()) {
                if let Ok(Some(slot)) = self.state.tx_slot(&id) {
                    if let Ok(Some(bytes)) = self.state.load_block(slot) {
                        if let Ok(block) = serde_json::from_slice::<Block>(&bytes) {
                            for tx in block.transactions {
                                if tx.message_hash() == id {
                                    return Ok(Some(tx));
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(None)
    }

    async fn get_version(&self) -> RpcResult<String> {
        Ok(format!("himsha-node {}", env!("CARGO_PKG_VERSION")))
    }

    async fn get_peers(&self) -> RpcResult<Vec<String>> {
        // No P2P gossip layer yet; a follower knows only its primary.
        Ok(std::env::var("HIMSHA_FOLLOW").ok().into_iter().collect())
    }

    async fn send_transactions(&self, txs: Vec<RuntimeTransaction>) -> RpcResult<Vec<String>> {
        let mut ids = Vec::with_capacity(txs.len());
        for tx in txs {
            ids.push(self.send_transaction(tx).await?);
        }
        Ok(ids)
    }

    async fn recent_transactions(&self, limit: u32) -> RpcResult<Vec<RuntimeTransaction>> {
        let mut out = Vec::new();
        let tip = self
            .state
            .current_slot()
            .map_err(|e| ErrorObjectOwned::owned(-32008, e.to_string(), None::<()>))?;
        let mut slot = tip;
        let floor = tip.saturating_sub(1024);
        while slot > floor && (out.len() as u32) < limit {
            if let Ok(Some(bytes)) = self.state.load_block(slot) {
                if let Ok(block) = serde_json::from_slice::<Block>(&bytes) {
                    for tx in block.transactions {
                        out.push(tx);
                        if (out.len() as u32) >= limit {
                            break;
                        }
                    }
                }
            }
            slot -= 1;
        }
        Ok(out)
    }

    async fn get_account_address(&self, pubkey: String) -> RpcResult<String> {
        use bitcoin::{key::TweakedPublicKey, secp256k1::XOnlyPublicKey, Address, ScriptBuf};
        let key = Pubkey::from_base58(&pubkey)
            .map_err(|e| ErrorObjectOwned::owned(-32030, e, None::<()>))?;
        // Treat the account's 32-byte key as a Taproot output key → P2TR address.
        let xonly = XOnlyPublicKey::from_slice(key.as_ref())
            .map_err(|e| ErrorObjectOwned::owned(-32031, e.to_string(), None::<()>))?;
        let spk = ScriptBuf::new_p2tr_tweaked(TweakedPublicKey::dangerous_assume_tweaked(xonly));
        let addr = Address::from_script(&spk, bitcoin_network())
            .map_err(|e| ErrorObjectOwned::owned(-32032, e.to_string(), None::<()>))?;
        Ok(addr.to_string())
    }

    async fn get_block_hash(&self, slot: u64) -> RpcResult<Option<String>> {
        Ok(self.block_hash_hex(slot))
    }

    async fn get_best_block_hash(&self) -> RpcResult<Option<String>> {
        let tip = self
            .state
            .current_slot()
            .map_err(|e| ErrorObjectOwned::owned(-32008, e.to_string(), None::<()>))?;
        Ok(self.block_hash_hex(tip))
    }

    async fn get_network_pubkey(&self) -> RpcResult<String> {
        Ok(std::env::var("HIMSHA_NETWORK_PUBKEY").unwrap_or_default())
    }

    async fn pre_vote(
        &self,
        term: u64,
        _candidate: String,
    ) -> RpcResult<himsha_node::election::VoteReply> {
        // Non-binding: read the current state without mutating term/vote.
        Ok(self.election.lock().unwrap().consider_pre_vote(term))
    }

    async fn request_vote(
        &self,
        term: u64,
        candidate: String,
    ) -> RpcResult<himsha_node::election::VoteReply> {
        let mut s = self.election.lock().unwrap();
        Ok(s.consider_vote(term, &candidate))
    }

    async fn get_leader(&self) -> RpcResult<himsha_node::election::LeaderInfo> {
        Ok(self.election.lock().unwrap().leader_info())
    }

    async fn create_invoice(&self, amount_sat: u64, memo: String) -> RpcResult<String> {
        let ln = lightning_or_err()?;
        ln.create_invoice(amount_sat, &memo)
            .await
            .map_err(|e| ErrorObjectOwned::owned(-32041, e.to_string(), None::<()>))
    }

    async fn pay_invoice(&self, bolt11: String) -> RpcResult<String> {
        let ln = lightning_or_err()?;
        ln.pay_invoice(&bolt11)
            .await
            .map_err(|e| ErrorObjectOwned::owned(-32042, e.to_string(), None::<()>))
    }

    async fn lightning_balance(&self) -> RpcResult<u64> {
        let ln = lightning_or_err()?;
        ln.channel_balance_sat()
            .await
            .map_err(|e| ErrorObjectOwned::owned(-32043, e.to_string(), None::<()>))
    }

    async fn get_all_accounts(&self, limit: u64) -> RpcResult<Vec<AccountInfo>> {
        self.state
            .all_accounts(limit as usize)
            .map_err(|e| ErrorObjectOwned::owned(-32050, e.to_string(), None::<()>))
    }

    async fn get_txid_from_btc_txid(&self, btc_txid: String) -> RpcResult<Option<String>> {
        let raw = hex::decode(&btc_txid)
            .map_err(|e| ErrorObjectOwned::owned(-32051, e.to_string(), None::<()>))?;
        let btc = <[u8; 32]>::try_from(raw.as_slice()).map_err(|_| {
            ErrorObjectOwned::owned(-32052, "btc_txid must be 32 bytes (hex)", None::<()>)
        })?;
        let himsha = self
            .state
            .himsha_txid_for_btc(&btc)
            .map_err(|e| ErrorObjectOwned::owned(-32053, e.to_string(), None::<()>))?;
        Ok(himsha.map(hex::encode))
    }

    async fn get_stats(&self) -> RpcResult<himsha_node::rpc::NodeStats> {
        let (accounts, transactions, tip_slot) = self
            .state
            .stats()
            .map_err(|e| ErrorObjectOwned::owned(-32054, e.to_string(), None::<()>))?;
        let programs = self.registry.lock().unwrap().list().len() as u64;
        Ok(himsha_node::rpc::NodeStats {
            accounts,
            transactions,
            tip_slot,
            programs,
        })
    }

    async fn get_custody_info(&self) -> RpcResult<Option<himsha_node::rpc::CustodyInfo>> {
        let Some(custody) = &self.custody else {
            return Ok(None); // single-hot-wallet mode
        };
        let group_xonly = custody.group_xonly();
        let address =
            himsha_node::settlement_tx::committee_address(&group_xonly, bitcoin_network())
                .map_err(|e| ErrorObjectOwned::owned(-32055, e.to_string(), None::<()>))?;
        Ok(Some(himsha_node::rpc::CustodyInfo {
            threshold: custody.threshold,
            total: custody.total,
            group_key: hex::encode(group_xonly),
            address: address.to_string(),
        }))
    }
}

/// Resolve the configured Lightning client or a clear "not configured" error.
fn lightning_or_err() -> Result<himsha_node::lightning::LightningClient, ErrorObjectOwned> {
    himsha_node::lightning::LightningClient::from_env().ok_or_else(|| {
        ErrorObjectOwned::owned(
            -32040,
            "lightning not configured (set LND_REST_URL, LND_MACAROON_HEX)",
            None::<()>,
        )
    })
}

// ---- main ----

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("himsha_node=info".parse()?))
        .init();

    let bind_addr: SocketAddr = std::env::var("HIMSHA_BIND")
        .unwrap_or_else(|_| "127.0.0.1:9100".into())
        .parse()?;
    let db_path = std::env::var("HIMSHA_DB").unwrap_or_else(|_| "himsha.redb".into());

    info!("opening state database at {db_path}");
    let state = NodeState::open(&db_path)?;

    // Seed a genesis block at slot 0 so a fresh node has a valid recent blockhash
    // (clients fetch it via himsha_getBestBlockHash) and replay protection works
    // from the very first transaction.
    if state.load_block(0)?.is_none() {
        let genesis = Block::new(0, 0, vec![], 0);
        state.save_block(0, serde_json::to_vec(&genesis)?)?;
        info!("seeded genesis block {}", hex::encode(genesis.blockhash));
    }
    let chain_id = chain_id_for_network(
        &std::env::var("BITCOIN_NETWORK").unwrap_or_else(|_| "regtest".into()),
    );
    info!("chain_id = {chain_id}");

    // Pre-register the genesis built-in programs. They execute natively (no ELF),
    // so they are registered with empty bytecode purely so `himsha_listPrograms`
    // and `contains()` report them.
    let registry = Arc::new(Mutex::new(ProgramRegistry::new()));
    {
        let mut reg = registry.lock().unwrap();

        #[cfg(feature = "zkvm")]
        {
            // ZK path: every built-in is proven through the universal guest ELF.
            himsha_vm::zk::register_builtins(&mut reg);
            info!(
                "registered {} built-in programs (zkVM guest)",
                reg.list().len()
            );
        }
        #[cfg(not(feature = "zkvm"))]
        {
            // Native path: built-ins run via dispatch, so they only need a marker
            // (empty ELF) entry so `himsha_listPrograms` reports them.
            for id in himsha_runtime::program_ids::builtins() {
                reg.register(id, Vec::new(), [0u8; 32]);
            }
            info!(
                "registered {} built-in programs (native dispatch)",
                reg.list().len()
            );
        }
    }

    let (pending_tx, pending_rx) = mpsc::channel(4096);

    // Threshold settlement custody (HIMSHA_THRESHOLD="M/N"), generated once at
    // startup so settlement reuses one committee rather than re-keying per tx.
    let custody = himsha_node::custody::Custody::from_env().map(Arc::new);

    // Shared leader-election state (terms + per-term vote + leader view), used by the
    // RequestVote / GetLeader RPCs and a candidate follower.
    let election = Arc::new(Mutex::new(himsha_node::election::ElectionState::default()));
    let self_id = std::env::var("HIMSHA_SELF").unwrap_or_else(|_| format!("http://{bind_addr}"));

    // Follower (read replica) vs primary (block producer).
    match std::env::var("HIMSHA_FOLLOW").ok() {
        Some(primary_url) if !primary_url.is_empty() => {
            // Replicate the primary's state by re-deriving every block. If failover
            // is enabled (HIMSHA_FAILOVER_MISSES) and the primary goes unreachable,
            // self-promote to sequencer and start producing from the replicated tip.
            let interval_secs = std::env::var("HIMSHA_FOLLOW_INTERVAL_SECS")
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(2);
            let max_misses = std::env::var("HIMSHA_FAILOVER_MISSES")
                .ok()
                .and_then(|s| s.parse::<u32>().ok());
            // Higher-priority standbys to defer to (comma-separated URLs) for
            // split-brain-safe election; empty = this node is top priority.
            let higher_peers: Vec<String> = std::env::var("HIMSHA_STANDBY_PEERS")
                .ok()
                .map(|s| {
                    s.split(',')
                        .map(|p| p.trim().to_string())
                        .filter(|p| !p.is_empty())
                        .collect()
                })
                .unwrap_or_default();
            // Quorum election (partition-safe): full electable member set + this node's id.
            let members: Vec<String> = std::env::var("HIMSHA_ELECTION_MEMBERS")
                .ok()
                .map(|s| {
                    s.split(',')
                        .map(|p| p.trim().to_string())
                        .filter(|p| !p.is_empty())
                        .collect()
                })
                .unwrap_or_default();
            let f_state = state.clone();
            let f_reg = registry.clone();
            let f_election = election.clone();
            let f_self = self_id.clone();
            tokio::spawn(async move {
                let follower =
                    himsha_node::follower::Follower::new(f_state.clone(), f_reg, primary_url)
                        .with_higher_peers(higher_peers)
                        .with_election(f_election, members, f_self);
                let promote = follower
                    .run_until_promote(std::time::Duration::from_secs(interval_secs), max_misses)
                    .await;
                if promote {
                    info!("FAILOVER: promoted to sequencer — block production enabled");
                    himsha_node::block_producer::BlockProducer::new(f_state, pending_rx)
                        .run()
                        .await;
                }
            });
            match max_misses {
                Some(n) => info!("running as FOLLOWER with failover after {n} missed polls"),
                None => info!("running as FOLLOWER (pure replica) — block production disabled"),
            }
        }
        _ => {
            // Primary: it is the leader; advertise via GetLeader, collect txs, produce blocks.
            election.lock().unwrap().become_leader(&self_id);
            let state_clone = state.clone();
            tokio::spawn(async move {
                BlockProducer::new(state_clone, pending_rx).run().await;
            });
        }
    }

    // Spawn the Bitcoin indexer auto-sync (only when RPC is configured).
    match BitcoinIndexer::from_env() {
        Some(indexer) => {
            // Observe and log the sync events on the async runtime.
            let mut events = indexer.event_stream();
            tokio::spawn(async move {
                use tokio::sync::broadcast::error::RecvError;
                loop {
                    match events.recv().await {
                        Ok(ev) => info!("bitcoin indexer event: {ev:?}"),
                        Err(RecvError::Lagged(n)) => warn!("indexer events lagged by {n}"),
                        Err(RecvError::Closed) => break,
                    }
                }
            });
            // Run the blocking poll loop on a dedicated thread.
            let interval_secs = std::env::var("BITCOIN_SYNC_INTERVAL_SECS")
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(10);
            std::thread::spawn(move || {
                indexer.run_sync_blocking(std::time::Duration::from_secs(interval_secs));
            });
            info!("bitcoin indexer auto-sync enabled ({interval_secs}s interval)");
        }
        None => info!("bitcoin indexer auto-sync disabled (set BITCOIN_RPC_URL to enable)"),
    }

    // Build the RPC module
    let himsha_node = HimshaNode {
        state: state.clone(),
        registry: registry.clone(),
        pending_tx,
        election: election.clone(),
        chain_id,
        custody: custody.clone(),
    };

    let cors = CorsLayer::new()
        .allow_methods([http::Method::POST])
        .allow_origin(Any)
        .allow_headers([http::header::CONTENT_TYPE]);

    let server = Server::builder()
        .set_http_middleware(tower::ServiceBuilder::new().layer(cors))
        .build(bind_addr)
        .await?;

    let module = himsha_node.into_rpc();
    let addr = server.local_addr()?;
    let handle = server.start(module);

    info!("HIMSHA node listening on http://{addr}");
    info!("RPC methods: himsha_sendTransaction, himsha_getAccountInfo, himsha_deployProgram, himsha_getSlot, himsha_isNodeReady");

    handle.stopped().await;
    Ok(())
}
