use himsha_runtime::transaction::{Block, RuntimeTransaction};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tracing::info;

use crate::state::NodeState;

/// Collects pending transactions, forms blocks, and writes them to storage.
pub struct BlockProducer {
    state: NodeState,
    pending_rx: mpsc::Receiver<RuntimeTransaction>,
}

impl BlockProducer {
    pub fn new(state: NodeState, pending_rx: mpsc::Receiver<RuntimeTransaction>) -> Self {
        Self { state, pending_rx }
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
                Err(e) => { tracing::error!("advance_slot: {e}"); continue; }
            };

            let parent_slot = slot.saturating_sub(1);
            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            let block = Block::new(slot, parent_slot, batch, ts);
            let bytes = serde_json::to_vec(&block).unwrap_or_default();

            if let Err(e) = self.state.save_block(slot, bytes) {
                tracing::error!("save_block slot={slot}: {e}");
            } else {
                // Index each tx for O(1) lookup + explorer counters.
                for tx in &block.transactions {
                    if let Err(e) = self.state.index_transaction(&tx.message_hash(), slot) {
                        tracing::error!("index_transaction slot={slot}: {e}");
                    }
                }
                info!("produced block slot={slot} txs={}", block.transactions.len());
            }
        }
    }
}
