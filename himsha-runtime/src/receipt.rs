use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{account::AccountInfo, pubkey::Pubkey, utxo::UtxoMeta};

/// A ZK execution receipt produced by the `himsha-vm` after running a program.
///
/// The receipt proves that:
///   - The program identified by `program_id` ran correctly.
///   - The provided `accounts` (before) transitioned to `new_accounts` (after).
///   - The `bitcoin_outputs` are the exact outputs to be anchored on Bitcoin.
///
/// Anyone can verify the receipt without re-running the program, using only
/// the public inputs (program_id, method_id, journal_hash).
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct ExecutionReceipt {
    /// Program that was executed.
    pub program_id: Pubkey,
    /// RISC Zero image ID (SHA-256 of the compiled ELF guest).
    pub image_id: [u8; 32],
    /// SHA-256 of the journal emitted by the guest.
    pub journal_hash: [u8; 32],
    /// Raw ZK proof bytes (STARK, can be verified with risc0-zkvm crate).
    pub proof_bytes: Vec<u8>,
    /// Whether the proof has been verified by the node.
    pub verified: bool,
}

impl ExecutionReceipt {
    pub fn unverified(
        program_id: Pubkey,
        image_id: [u8; 32],
        journal_hash: [u8; 32],
        proof_bytes: Vec<u8>,
    ) -> Self {
        Self {
            program_id,
            image_id,
            journal_hash,
            proof_bytes,
            verified: false,
        }
    }
}

impl ExecutionReceipt {
    /// Canonical journal hash that an [`ExecutionReceipt`] must commit to for a
    /// given set of post-execution accounts on the native path: SHA-256 of the
    /// borsh-encoded accounts. The zkVM executor binds the same accounts through
    /// the guest journal.
    pub fn journal_hash_for(accounts: &[AccountInfo]) -> [u8; 32] {
        let bytes = borsh::to_vec(accounts).unwrap_or_default();
        Sha256::digest(bytes).into()
    }
}

/// The state change committed after a verified execution.
///
/// This is what the node writes to its database and broadcasts to Bitcoin.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StateTransition {
    /// Receipt proving the transition is valid.
    pub receipt: ExecutionReceipt,
    /// Accounts whose data changed.
    pub updated_accounts: Vec<AccountInfo>,
    /// New Bitcoin UTXOs that anchor the updated state.
    pub new_utxos: Vec<UtxoMeta>,
    /// Bitcoin transaction id that committed these UTXOs.
    pub bitcoin_txid: Option<[u8; 32]>,
}

impl StateTransition {
    /// Verify the receipt actually commits to `updated_accounts` before the node
    /// persists them — turning the receipt from a write-only record into an enforced
    /// gate (no state is committed without a receipt that binds it).
    ///
    /// - **zkVM path** (`receipt.verified`): the STARK proof was already verified
    ///   against the image id at execution time, and the guest journal commits to
    ///   the output, so the transition is accepted.
    /// - **Native path**: recomputes the canonical journal hash of the accounts and
    ///   requires it to equal `receipt.journal_hash`, catching any tampering of the
    ///   transition between execution and persistence.
    ///
    /// (Native verification re-checks integrity, not cryptographic soundness — the
    /// node trusts its own native execution; soundness comes from the zkVM path.)
    pub fn verify(&self) -> Result<(), &'static str> {
        if self.receipt.verified {
            return Ok(());
        }
        if ExecutionReceipt::journal_hash_for(&self.updated_accounts) != self.receipt.journal_hash {
            return Err("receipt journal_hash does not commit to the updated accounts");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn native_transition(accounts: Vec<AccountInfo>) -> StateTransition {
        let receipt = ExecutionReceipt::unverified(
            Pubkey::default(),
            [0u8; 32],
            ExecutionReceipt::journal_hash_for(&accounts),
            Vec::new(),
        );
        StateTransition {
            receipt,
            updated_accounts: accounts,
            new_utxos: vec![],
            bitcoin_txid: None,
        }
    }

    fn acct(lamports: u64) -> AccountInfo {
        AccountInfo::new(Pubkey::from_seed(b"a"), Pubkey::default(), lamports, 0)
    }

    #[test]
    fn native_receipt_binds_accounts() {
        assert!(native_transition(vec![acct(10)]).verify().is_ok());
    }

    #[test]
    fn tampered_accounts_are_rejected() {
        let mut t = native_transition(vec![acct(10)]);
        t.updated_accounts[0].lamports = 999; // change state after the receipt was made
        assert!(t.verify().is_err());
    }

    #[test]
    fn verified_zkvm_receipt_is_trusted() {
        // A `verified` receipt is accepted without recomputing the native hash
        // (its proof was checked at execution time).
        let mut t = native_transition(vec![acct(10)]);
        t.receipt.verified = true;
        t.receipt.journal_hash = [0u8; 32]; // wouldn't match natively, but verified=true
        assert!(t.verify().is_ok());
    }
}
