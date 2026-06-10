use himsha_runtime::transaction::{Block, RuntimeTransaction};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tracing::{error, info};

use crate::{
    bitcoin_indexer::BitcoinIndexer,
    execution::{Executor, Mode},
    state::{NodeState, TxStatus},
};

/// Collects pending transactions, executes them authoritatively, forms blocks,
/// and writes them to storage. This is the **execution point**: the RPC only
/// validates + preflights + queues, so block production is where a transaction's
/// effects actually commit, in a single deterministic order.
pub struct BlockProducer {
    state: NodeState,
    pending_rx: mpsc::Receiver<RuntimeTransaction>,
    /// Shared executor (state + program registry + settlement custody).
    executor: Executor,
    /// Commit the state root to Bitcoin every N blocks (0 = disabled). Set via
    /// `HIMSHA_ANCHOR_INTERVAL`; requires Bitcoin RPC (`BITCOIN_RPC_URL`).
    anchor_interval: u64,
    /// Bitcoin indexer used for OP_RETURN anchoring, built once when enabled.
    indexer: Option<BitcoinIndexer>,
}

impl BlockProducer {
    pub fn new(
        state: NodeState,
        pending_rx: mpsc::Receiver<RuntimeTransaction>,
        executor: Executor,
    ) -> Self {
        let anchor_interval = std::env::var("HIMSHA_ANCHOR_INTERVAL")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        // Only build the indexer when anchoring is actually enabled.
        let indexer = if anchor_interval > 0 {
            match BitcoinIndexer::from_env() {
                Some(ix) => {
                    info!("L1 state-root anchoring enabled (every {anchor_interval} blocks)");
                    Some(ix)
                }
                None => {
                    error!(
                        "HIMSHA_ANCHOR_INTERVAL set but Bitcoin RPC unconfigured — \
                         anchoring disabled"
                    );
                    None
                }
            }
        } else {
            None
        };
        Self {
            state,
            pending_rx,
            executor,
            anchor_interval,
            indexer,
        }
    }

    /// Main loop: every 300 ms collect pending txs → form block → persist.
    pub async fn run(mut self) {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

            let mut batch: Vec<RuntimeTransaction> = Vec::new();
            // drain all pending without blocking
            while let Ok(tx) = self.pending_rx.try_recv() {
                batch.push(tx);
            }

            if batch.is_empty() {
                continue;
            }

            let slot = match self.state.advance_slot() {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("advance_slot: {e}");
                    continue;
                }
            };

            let parent_slot = slot.saturating_sub(1);
            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            // Execute each queued transaction authoritatively, in order. A tx that
            // commits is included in the block and marked Succeeded; one that fails
            // here (e.g. state changed since its preflight) is excluded and marked
            // Failed with the reason — never a silent drop. Each commit persists
            // atomically, so by the loop's end the account table is fully updated.
            let mut included: Vec<RuntimeTransaction> = Vec::with_capacity(batch.len());
            for tx in batch {
                let txid = tx.message_hash();
                match self.executor.apply(&tx, Mode::Commit).await {
                    Ok(()) => included.push(tx),
                    Err(e) => {
                        error!(
                            "tx {} failed at slot {slot}: {}",
                            hex::encode(txid),
                            e.message
                        );
                        // Never downgrade a tx that already Succeeded in an earlier
                        // slot: an identical txid re-submitted and failing later must
                        // not overwrite its recorded success.
                        let _ = self
                            .state
                            .mark_failed_unless_succeeded(&txid, slot, e.message);
                    }
                }
            }

            // The state root now reflects every committed transaction in this slot.
            let state_root = self.state.compute_state_root().unwrap_or_else(|e| {
                error!("compute_state_root slot={slot}: {e}");
                [0u8; 32]
            });
            let block = Block::new_with_root(slot, parent_slot, included, ts, state_root);
            let bytes = serde_json::to_vec(&block).unwrap_or_default();

            if let Err(e) = self.state.save_block(slot, bytes) {
                error!("save_block slot={slot}: {e}");
                continue;
            }
            // Index each tx for O(1) lookup + explorer counters, and record that
            // it succeeded in this slot (the tx executed before block inclusion).
            for tx in &block.transactions {
                let txid = tx.message_hash();
                if let Err(e) = self.state.index_transaction(&txid, slot) {
                    error!("index_transaction slot={slot}: {e}");
                }
                let _ = self
                    .state
                    .set_tx_status(&txid, &TxStatus::Succeeded { slot });
            }
            info!(
                "produced block slot={slot} txs={} state_root={}",
                block.transactions.len(),
                hex::encode(state_root)
            );

            // Periodically commit the state root to Bitcoin L1 via OP_RETURN.
            if self.anchor_interval > 0 && slot % self.anchor_interval == 0 {
                if let Some(ix) = &self.indexer {
                    match ix.anchor_state_root(slot, &state_root) {
                        Ok(txid) => {
                            if let Err(e) = self.state.record_anchor(slot, state_root, &txid) {
                                error!("record_anchor slot={slot}: {e}");
                            } else {
                                info!("anchored state root slot={slot} btc_txid={txid}");
                            }
                        }
                        Err(e) => error!("anchor state root slot={slot} failed: {e}"),
                    }
                }
            }
        }
    }
}
