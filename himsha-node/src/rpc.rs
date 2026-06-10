use himsha_runtime::{
    account::AccountInfo,
    transaction::{Block, RuntimeTransaction},
};
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use serde::{Deserialize, Serialize};

// ---- RPC method trait ----

/// Full HIMSHA Network JSON-RPC API.
///
/// Clients connect to `http://localhost:9100` (default) and call these methods.
#[rpc(server, client)]
pub trait HimshaRpc {
    /// Submit a signed `RuntimeTransaction` for ZK execution and Bitcoin settlement.
    /// Returns the transaction identifier (SHA-256 of the message hash, hex).
    #[method(name = "himsha_sendTransaction")]
    async fn send_transaction(&self, tx: RuntimeTransaction) -> RpcResult<String>;

    /// Fetch the current on-chain state of an account.
    #[method(name = "himsha_getAccountInfo")]
    async fn get_account_info(&self, pubkey: String) -> RpcResult<Option<AccountInfo>>;

    /// Return all accounts owned by a given program.
    #[method(name = "himsha_getProgramAccounts")]
    async fn get_program_accounts(&self, program_id: String) -> RpcResult<Vec<AccountInfo>>;

    /// Deploy a compiled RISC-V ELF binary as a new HIMSHA program.
    /// `elf_hex` is the hex-encoded ELF, `image_id_hex` is the RISC Zero image ID.
    /// Returns the new program's public key (hex).
    #[method(name = "himsha_deployProgram")]
    async fn deploy_program(&self, elf_hex: String, image_id_hex: String) -> RpcResult<String>;

    /// Get a block by slot number.
    #[method(name = "himsha_getBlock")]
    async fn get_block(&self, slot: u64) -> RpcResult<Option<Block>>;

    /// Return the latest committed slot.
    #[method(name = "himsha_getSlot")]
    async fn get_slot(&self) -> RpcResult<u64>;

    /// Liveness check — returns true when the node is fully initialized.
    #[method(name = "himsha_isNodeReady")]
    async fn is_node_ready(&self) -> RpcResult<bool>;

    /// Return the list of all deployed program IDs.
    #[method(name = "himsha_listPrograms")]
    async fn list_programs(&self) -> RpcResult<Vec<String>>;

    /// Fetch UTXO details from the Bitcoin indexer.
    #[method(name = "himsha_getUtxo")]
    async fn get_utxo(
        &self,
        txid: String,
        vout: u32,
    ) -> RpcResult<Option<himsha_runtime::utxo::UtxoInfo>>;

    // ---- breadth: batch reads, faucet, tx lookup, introspection ----

    /// Dev faucet: credit `lamports` to `pubkey` (creating the account if needed).
    /// Returns the account's new balance. Enabled only when `HIMSHA_FAUCET=1`.
    #[method(name = "himsha_requestAirdrop")]
    async fn request_airdrop(&self, pubkey: String, lamports: u64) -> RpcResult<u64>;

    /// Dev faucet: create a fresh system-owned account at `pubkey` with `space`
    /// bytes, funded with `lamports`. Fails if the account already exists.
    /// Enabled only when `HIMSHA_FAUCET=1`.
    #[method(name = "himsha_createAccountWithFaucet")]
    async fn create_account_with_faucet(
        &self,
        pubkey: String,
        lamports: u64,
        space: u64,
    ) -> RpcResult<AccountInfo>;

    /// Batch account read — one entry per input key, `null` where missing.
    #[method(name = "himsha_getMultipleAccounts")]
    async fn get_multiple_accounts(
        &self,
        pubkeys: Vec<String>,
    ) -> RpcResult<Vec<Option<AccountInfo>>>;

    /// Look up a processed transaction by id (hex message hash) across recent blocks.
    #[method(name = "himsha_getProcessedTransaction")]
    async fn get_processed_transaction(
        &self,
        txid: String,
    ) -> RpcResult<Option<RuntimeTransaction>>;

    /// Node software version.
    #[method(name = "himsha_getVersion")]
    async fn get_version(&self) -> RpcResult<String>;

    /// Connected peers (followers/primary). Single-node returns an empty list.
    #[method(name = "himsha_getPeers")]
    async fn get_peers(&self) -> RpcResult<Vec<String>>;

    // ---- breadth RPCs (full account & settlement-lookup coverage) ----

    /// Submit a batch of transactions; returns a tx id per input (in order).
    #[method(name = "himsha_sendTransactions")]
    async fn send_transactions(&self, txs: Vec<RuntimeTransaction>) -> RpcResult<Vec<String>>;

    /// Most recent processed transactions, newest first, up to `limit`.
    #[method(name = "himsha_recentTransactions")]
    async fn recent_transactions(&self, limit: u32) -> RpcResult<Vec<RuntimeTransaction>>;

    /// Derive the Bitcoin (Taproot/P2TR) address linked to an account public key.
    #[method(name = "himsha_getAccountAddress")]
    async fn get_account_address(&self, pubkey: String) -> RpcResult<String>;

    /// Block hash (hex) at a given slot.
    #[method(name = "himsha_getBlockHash")]
    async fn get_block_hash(&self, slot: u64) -> RpcResult<Option<String>>;

    /// Hash (hex) of the current tip block.
    #[method(name = "himsha_getBestBlockHash")]
    async fn get_best_block_hash(&self) -> RpcResult<Option<String>>;

    /// The node's configured network identity key (hex), or empty if unset.
    #[method(name = "himsha_getNetworkPubkey")]
    async fn get_network_pubkey(&self) -> RpcResult<String>;

    /// Raft **PreVote**: a non-binding poll a candidate runs before a real election —
    /// grant iff `term` is current-or-newer and we are not a live leader. Does not mutate
    /// term or recorded vote, so it cannot inflate terms or unseat a healthy leader.
    #[method(name = "himsha_preVote")]
    async fn pre_vote(&self, term: u64, candidate: String)
        -> RpcResult<crate::election::VoteReply>;

    /// Raft-style leader-election vote: grant iff `term` is current-or-newer and we
    /// haven't already voted for a different candidate this term.
    #[method(name = "himsha_requestVote")]
    async fn request_vote(
        &self,
        term: u64,
        candidate: String,
    ) -> RpcResult<crate::election::VoteReply>;

    /// This node's view of the current leader (used as a heartbeat / for re-pointing
    /// standbys to a newly-elected leader).
    #[method(name = "himsha_getLeader")]
    async fn get_leader(&self) -> RpcResult<crate::election::LeaderInfo>;

    // ---- Lightning Network (off-chain settlement rail; needs LND configured) ----

    /// Create a BOLT-11 invoice for `amount_sat`; returns the payment request.
    #[method(name = "himsha_createInvoice")]
    async fn create_invoice(&self, amount_sat: u64, memo: String) -> RpcResult<String>;

    /// Pay a BOLT-11 invoice; returns the payment hash.
    #[method(name = "himsha_payInvoice")]
    async fn pay_invoice(&self, bolt11: String) -> RpcResult<String>;

    /// Spendable Lightning channel balance, in sats.
    #[method(name = "himsha_lightningBalance")]
    async fn lightning_balance(&self) -> RpcResult<u64>;

    // ---- indexer-backed breadth RPCs ----

    /// All accounts in state, bounded by `limit` (0 = unbounded).
    #[method(name = "himsha_getAllAccounts")]
    async fn get_all_accounts(&self, limit: u64) -> RpcResult<Vec<AccountInfo>>;

    /// HIMSHA txid (hex) that produced a given Bitcoin settlement txid (hex), or null.
    /// Served from the settlement index — maps an on-chain txid back to its L2 tx.
    #[method(name = "himsha_getTxidFromBtcTxid")]
    async fn get_txid_from_btc_txid(&self, btc_txid: String) -> RpcResult<Option<String>>;

    /// Aggregate chain stats for the explorer overview (indexed counters, no scans).
    #[method(name = "himsha_getStats")]
    async fn get_stats(&self) -> RpcResult<NodeStats>;

    /// Threshold-custody status: the M-of-N config, the committee group key, and
    /// the Taproot address to fund so the committee can key-spend settlements.
    /// Returns `null` when `HIMSHA_THRESHOLD` is unset (single-hot-wallet mode).
    #[method(name = "himsha_getCustodyInfo")]
    async fn get_custody_info(&self) -> RpcResult<Option<CustodyInfo>>;

    /// Merkle inclusion proof that `pubkey`'s account is committed in the current
    /// state root, plus the latest root anchored to Bitcoin (if any). A client
    /// verifies the account against a root it trusts from the OP_RETURN anchor.
    /// Returns `null` if the account does not exist.
    #[method(name = "himsha_getStateProof")]
    async fn get_state_proof(&self, pubkey: String) -> RpcResult<Option<StateProof>>;
}

/// A state-root inclusion proof for one account (see [`crate::state`] and
/// [`himsha_runtime::merkle`]). The client recomputes the leaf from the account
/// it holds and walks `siblings` to the `state_root`; if that root equals the
/// `anchored_state_root` committed in `anchored_btc_txid`, the account is proven
/// to be in the Bitcoin-anchored state.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StateProof {
    /// Current Merkle state root (hex) this proof verifies against.
    pub state_root: String,
    /// The proven leaf hash (hex) = SHA-256(0x00 ‖ key ‖ account_bytes).
    pub leaf: String,
    /// Leaf position among the ordered accounts.
    pub index: u64,
    /// Sibling hashes (hex), leaf→root, needed to recompute the root.
    pub siblings: Vec<String>,
    /// Slot of the most recently Bitcoin-anchored root, if any.
    pub anchored_slot: Option<u64>,
    /// The most recently anchored state root (hex), if any.
    pub anchored_state_root: Option<String>,
    /// The OP_RETURN txid committing that anchored root, if any.
    pub anchored_btc_txid: Option<String>,
}

/// Threshold-custody settlement configuration (see [`crate::custody`]).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CustodyInfo {
    /// Signatures required to settle.
    pub threshold: u16,
    /// Total committee members.
    pub total: u16,
    /// 32-byte x-only group key (hex) — the Taproot output key.
    pub group_key: String,
    /// P2TR address controlled by the committee; fund this to give it custody.
    pub address: String,
}

/// Indexed counters for the explorer overview.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeStats {
    pub accounts: u64,
    pub transactions: u64,
    pub tip_slot: u64,
    pub programs: u64,
}
