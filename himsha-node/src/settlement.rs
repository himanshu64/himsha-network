//! Shared Ordinals-loan settlement draining, used by both the primary node and
//! follower replicas.
//!
//! The lending program queues `Settlement` directives; after a lending instruction
//! executes, the node drains them. The **primary** (with a Bitcoin indexer) builds
//! and broadcasts the real UTXO moves; a **follower** passes `None` and simply clears
//! the queue so its replicated state matches the primary's (no broadcast).

use himsha_runtime::account::AccountInfo;
use tracing::{error, info};

use crate::bitcoin_indexer::BitcoinIndexer;

/// Drain queued lending settlements from any collection accounts in `accounts`.
/// With `Some(indexer)` the UTXO moves are broadcast; with `None` they are only
/// logged and the queue is cleared (follower/replica behavior).
pub fn drain_lending(accounts: &mut [AccountInfo], indexer: Option<&BitcoinIndexer>) {
    use himsha_lending_program::{take_settlements, CollectionAccount, SettlementKind};

    for account in accounts.iter_mut() {
        let mut coll: CollectionAccount = match account.read_data() {
            Ok(c) => c,
            Err(_) => continue, // not a collection account
        };
        if coll.pending_settlements.is_empty() {
            continue;
        }
        for s in take_settlements(&mut coll) {
            match indexer {
                Some(ix) => {
                    let result = match s.kind {
                        SettlementKind::Repayment => ix.send_payment(&s.recipient, s.amount),
                        SettlementKind::ReturnInscription | SettlementKind::SeizeInscription => {
                            ix.transfer_utxo(&hex::encode(s.utxo.txid), s.utxo.vout, &s.recipient)
                        }
                    };
                    match result {
                        Ok(txid) => info!(
                            "settled {:?} for {} via bitcoin tx {txid}",
                            s.kind, s.inscription_id
                        ),
                        Err(e) => error!(
                            "settlement {:?} for {} failed: {e}",
                            s.kind, s.inscription_id
                        ),
                    }
                }
                None => info!(
                    "settlement {:?} replicated (no broadcast): inscription={} -> {} ({} sats)",
                    s.kind, s.inscription_id, s.recipient, s.amount,
                ),
            }
        }
        if account.write_data(&coll).is_err() {
            error!("failed to clear lending settlements for {}", account.key);
        }
    }
}
