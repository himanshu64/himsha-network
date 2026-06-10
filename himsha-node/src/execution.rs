//! Shared transaction execution — the single code path that runs a HIMSHA
//! transaction's instructions, owned by both the RPC (for a non-committing
//! **preflight**) and the block producer (for the **authoritative** commit).
//!
//! A transaction's instructions execute in order against an in-memory `overlay`
//! so a later instruction sees earlier writes, and the whole transaction either
//! commits atomically or not at all. In [`Mode::Preflight`] nothing is persisted
//! and no Bitcoin/Lightning settlement is broadcast — it only surfaces
//! deterministic execution errors synchronously to the RPC caller. In
//! [`Mode::Commit`] the overlay is persisted in one DB transaction and lending
//! settlements are drained on-chain.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use himsha_runtime::{
    account::{AccountInfo, StoredAccount},
    pubkey::Pubkey,
    transaction::RuntimeTransaction,
};
use himsha_vm::{
    executor::{ExecutionInput, ProgramExecutor},
    registry::ProgramRegistry,
};
use tracing::{error, info};

use crate::{bitcoin_indexer::BitcoinIndexer, custody::Custody, state::NodeState};

/// Whether execution persists its effects (and settles on-chain) or is a
/// throwaway dry-run used only to validate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Validate only — no persistence, no settlement broadcast.
    Preflight,
    /// Persist the result atomically and drain lending settlements on-chain.
    Commit,
}

/// A structured execution failure: a JSON-RPC error code plus a human message.
#[derive(Clone, Debug)]
pub struct ExecError {
    pub code: i32,
    pub message: String,
}

impl ExecError {
    fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// Runs transactions against node state. Cheap to clone (all fields are handles).
#[derive(Clone)]
pub struct Executor {
    state: NodeState,
    registry: Arc<Mutex<ProgramRegistry>>,
    custody: Option<Arc<Custody>>,
}

impl Executor {
    pub fn new(
        state: NodeState,
        registry: Arc<Mutex<ProgramRegistry>>,
        custody: Option<Arc<Custody>>,
    ) -> Self {
        Self {
            state,
            registry,
            custody,
        }
    }

    /// Execute every instruction of `tx` in order. In [`Mode::Commit`] the result
    /// is persisted atomically and lending settlements are broadcast; in
    /// [`Mode::Preflight`] nothing is written and no settlement occurs.
    pub async fn apply(&self, tx: &RuntimeTransaction, mode: Mode) -> Result<(), ExecError> {
        let mut overlay: HashMap<Pubkey, StoredAccount> = HashMap::new();

        for instr in &tx.message.instructions {
            // Materialize the instruction's accounts: prefer the intra-tx overlay,
            // then committed state, else a fresh account owned by the program.
            let mut accounts: Vec<AccountInfo> = Vec::with_capacity(instr.accounts.len());
            for meta in &instr.accounts {
                let mut account = if let Some(stored) = overlay.get(&meta.pubkey) {
                    stored.clone().into_account(meta.pubkey)
                } else if let Ok(Some(stored)) = self.state.load_account(&meta.pubkey) {
                    stored.into_account(meta.pubkey)
                } else {
                    AccountInfo::new(meta.pubkey, instr.program_id, 0, 0)
                };
                account.is_signer = meta.is_signer;
                account.is_writable = meta.is_writable;
                accounts.push(account);
            }

            // Reject an instruction that lists the same account writable twice —
            // otherwise a program (e.g. a token Transfer with source == dest) sees
            // two independent copies and last-write-wins inflates the balance.
            himsha_runtime::account::reject_duplicate_writable(&accounts)
                .map_err(|e| ExecError::new(-32002, e.to_string()))?;

            let input = ExecutionInput {
                accounts,
                instruction_data: instr.data.clone(),
                timestamp: tx.message.timestamp,
            };

            // Scope the (non-Send) registry guard so it drops before any `.await`.
            let result = {
                let registry_guard = self.registry.lock().unwrap();
                let executor = ProgramExecutor::new(&registry_guard);
                executor.execute_program(&instr.program_id, input, vec![])
            };

            let mut transition = result.map_err(|e| ExecError::new(-32002, e.to_string()))?;

            // The receipt must commit to exactly the accounts produced.
            transition.verify().map_err(|reason| {
                ExecError::new(-32004, format!("invalid execution receipt: {reason}"))
            })?;

            // Lending settlement only happens on the authoritative commit — never
            // during preflight (it broadcasts real Bitcoin/Lightning payments).
            if mode == Mode::Commit
                && instr.program_id == himsha_runtime::program_ids::lending_program()
            {
                self.settle_lending(&mut transition.updated_accounts, tx.message_hash())
                    .await;
            }

            for account in &transition.updated_accounts {
                overlay.insert(account.key, StoredAccount::from(account));
            }
        }

        if mode == Mode::Commit {
            self.state
                .save_accounts_atomic(&overlay)
                .map_err(|e| ExecError::new(-32001, e.to_string()))?;
        }
        Ok(())
    }

    /// Drain the lending program's queued settlements. Lightning fast-path for
    /// BOLT-11 repayments; otherwise on-chain via the Bitcoin indexer (threshold
    /// custody when configured, else the hot wallet). Mirrors the follower's
    /// no-broadcast `settlement::drain_lending` on the primary's settling side.
    async fn settle_lending(&self, accounts: &mut [AccountInfo], himsha_txid: [u8; 32]) {
        use crate::lightning::{is_invoice, LightningClient};
        use himsha_lending_program::{take_settlements, CollectionAccount, SettlementKind};

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
                match &indexer {
                    Some(ix) => {
                        let r = match s.kind {
                            SettlementKind::Repayment => ix.send_payment(&s.recipient, s.amount),
                            SettlementKind::ReturnInscription
                            | SettlementKind::SeizeInscription => {
                                let txid_hex = hex::encode(s.utxo.txid);
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
}

// =============================================================
#[cfg(test)]
mod tests {
    use super::*;
    use himsha_runtime::transaction::Message;

    fn tmp_executor(tag: &str) -> (Executor, NodeState, std::path::PathBuf) {
        let path =
            std::env::temp_dir().join(format!("himsha-exec-{tag}-{}.redb", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let state = NodeState::open(path.to_str().unwrap()).unwrap();
        let registry = Arc::new(Mutex::new(ProgramRegistry::new()));
        let exec = Executor::new(state.clone(), registry, None);
        (exec, state, path)
    }

    /// Fund a system-owned account so it can pay a transfer.
    fn fund(state: &NodeState, key: &Pubkey, lamports: u64) {
        state
            .save_account(
                key,
                &StoredAccount {
                    lamports,
                    data: vec![],
                    owner: himsha_runtime::program_ids::system_program().into(),
                    executable: false,
                    utxo_txid: None,
                    utxo_vout: None,
                },
            )
            .unwrap();
    }

    /// A system Transfer tx (apply() doesn't verify signatures — that's the RPC's
    /// job — so an unsigned tx with the signer meta set exercises execution).
    fn transfer_tx(from: Pubkey, to: Pubkey, lamports: u64) -> RuntimeTransaction {
        let ix = himsha_system_program::transfer(from, to, lamports);
        RuntimeTransaction::unsigned(Message::new(vec![from], vec![ix], 0))
    }

    #[tokio::test]
    async fn preflight_does_not_persist_commit_does() {
        let (exec, state, path) = tmp_executor("preflight");
        let from = Pubkey::from_seed(b"payer");
        let to = Pubkey::from_seed(b"dest");
        fund(&state, &from, 1_000);
        let tx = transfer_tx(from, to, 250);

        // Preflight succeeds but persists nothing.
        exec.apply(&tx, Mode::Preflight).await.unwrap();
        assert_eq!(state.load_account(&from).unwrap().unwrap().lamports, 1_000);
        assert!(state.load_account(&to).unwrap().is_none());

        // Commit applies the effect.
        exec.apply(&tx, Mode::Commit).await.unwrap();
        assert_eq!(state.load_account(&from).unwrap().unwrap().lamports, 750);
        assert_eq!(state.load_account(&to).unwrap().unwrap().lamports, 250);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn self_transfer_is_rejected_and_balance_unchanged() {
        // P0: a Transfer with source == destination lists the same account writable
        // twice. Without the runtime guard the program debits one copy and credits
        // the other from the same balance, last-write-wins inflating the balance.
        // The executor must reject the instruction before running it.
        let (exec, state, path) = tmp_executor("selftransfer");
        let acct = Pubkey::from_seed(b"selfpayer");
        fund(&state, &acct, 1_000);
        let tx = transfer_tx(acct, acct, 250); // from == to

        let err = exec.apply(&tx, Mode::Commit).await.unwrap_err();
        assert_eq!(err.code, -32002);
        assert!(err.message.contains("writable"), "got: {}", err.message);
        // Balance is exactly unchanged — not inflated by 250.
        assert_eq!(state.load_account(&acct).unwrap().unwrap().lamports, 1_000);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn failing_tx_surfaces_error_and_persists_nothing() {
        let (exec, state, path) = tmp_executor("failing");
        let from = Pubkey::from_seed(b"payer");
        let to = Pubkey::from_seed(b"dest");
        fund(&state, &from, 100);
        // Transfer more than the balance → InsufficientFunds.
        let tx = transfer_tx(from, to, 5_000);

        let err = exec.apply(&tx, Mode::Commit).await.unwrap_err();
        assert_eq!(err.code, -32002);
        // Nothing committed: balances unchanged, recipient never created.
        assert_eq!(state.load_account(&from).unwrap().unwrap().lamports, 100);
        assert!(state.load_account(&to).unwrap().is_none());
        let _ = std::fs::remove_file(&path);
    }
}
