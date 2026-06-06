use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

/// Minimal reference to a Bitcoin UTXO — used to anchor account state on-chain.
///
/// When an account's state changes, the node creates a new Bitcoin output
/// and updates `utxo` on the account. The previous UTXO is spent in the
/// same transaction, forming a chain of provenance on Bitcoin.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    BorshSerialize,
    BorshDeserialize,
    Serialize,
    Deserialize,
)]
pub struct UtxoMeta {
    /// Transaction ID (little-endian bytes, as stored by Bitcoin Core).
    pub txid: [u8; 32],
    /// Output index within that transaction.
    pub vout: u32,
}

impl UtxoMeta {
    pub fn new(txid: [u8; 32], vout: u32) -> Self {
        Self { txid, vout }
    }

    pub fn txid_hex(&self) -> String {
        hex::encode(self.txid)
    }
}

impl std::fmt::Display for UtxoMeta {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.txid_hex(), self.vout)
    }
}

/// Full UTXO data returned by the Bitcoin indexer.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UtxoInfo {
    pub meta: UtxoMeta,
    /// Value in satoshis.
    pub value: u64,
    /// Hex-encoded Bitcoin scriptPubKey.
    pub script_pubkey: String,
    /// Block confirmations (0 = mempool).
    pub confirmations: u32,
}
