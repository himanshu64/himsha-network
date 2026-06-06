use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

use crate::{
    instruction::Instruction,
    pubkey::{Keypair, Pubkey},
    signature::Signature,
};

/// The signable body of a HIMSHA transaction.
///
/// This is what gets hashed and signed by each required signer.
/// Programs see this alongside their accounts when executing inside the zkVM.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct Message {
    /// Accounts that must co-sign (ordered; indices match `signatures` in `RuntimeTransaction`).
    pub signers: Vec<Pubkey>,
    /// Instructions executed atomically left-to-right.
    pub instructions: Vec<Instruction>,
    /// A recent block hash from the target chain. Binds the transaction to a point
    /// in chain history: the node rejects it once this hash ages out of the recent
    /// window, which — together with txid dedup — gives replay protection and a
    /// natural expiry. Part of the signed body, so it cannot be swapped after signing.
    pub recent_blockhash: [u8; 32],
    /// Identifies the target network; the node rejects a transaction whose `chain_id`
    /// doesn't match its own, preventing cross-network replay.
    pub chain_id: u64,
    /// Unix-second timestamp supplied by the submitter; used inside programs for time-locks.
    pub timestamp: u64,
}

impl Message {
    /// Construct a message with **no** replay protection (`recent_blockhash` = 0,
    /// `chain_id` = 0). Convenient for tests and internally-replayed blocks; a node
    /// enforcing replay protection will reject it (chain_id 0 / unknown blockhash).
    pub fn new(signers: Vec<Pubkey>, instructions: Vec<Instruction>, timestamp: u64) -> Self {
        Self {
            signers,
            instructions,
            recent_blockhash: [0u8; 32],
            chain_id: 0,
            timestamp,
        }
    }

    /// Construct a replay-protected message bound to `recent_blockhash` and `chain_id`.
    pub fn new_signed(
        signers: Vec<Pubkey>,
        instructions: Vec<Instruction>,
        recent_blockhash: [u8; 32],
        chain_id: u64,
        timestamp: u64,
    ) -> Self {
        Self {
            signers,
            instructions,
            recent_blockhash,
            chain_id,
            timestamp,
        }
    }

    /// SHA-256 of the borsh-encoded message — this is what signers sign.
    pub fn hash(&self) -> [u8; 32] {
        Sha256::digest(borsh::to_vec(self).expect("borsh")).into()
    }
}

/// A fully formed, signed HIMSHA transaction submitted to the node via JSON-RPC.
///
/// Flow:
///   1. Client builds a `Message` and hashes it.
///   2. Each signer signs the hash with their Bitcoin Schnorr key.
///   3. Client wraps it in `RuntimeTransaction` and calls `himsha_sendTransaction`.
///   4. Node passes the transaction to `himsha-vm` for ZK execution.
///   5. If the ZK receipt is valid, node broadcasts the resulting Bitcoin tx.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct RuntimeTransaction {
    /// Protocol version (currently 0).
    pub version: u32,
    /// One Schnorr signature per entry in `message.signers`.
    pub signatures: Vec<Signature>,
    pub message: Message,
}

impl RuntimeTransaction {
    pub fn new(message: Message, signatures: Vec<Signature>) -> Self {
        Self {
            version: 0,
            signatures,
            message,
        }
    }

    pub fn unsigned(message: Message) -> Self {
        let n = message.signers.len();
        Self {
            version: 0,
            signatures: vec![Signature::zeroed(); n],
            message,
        }
    }

    /// Build a fully-signed transaction: each keypair signs the message hash in the
    /// same order as `message.signers`. Caller ensures `keypairs[i].pubkey()` equals
    /// `message.signers[i]` (otherwise [`verify_signatures`](Self::verify_signatures)
    /// will reject it).
    pub fn signed(message: Message, keypairs: &[&Keypair]) -> Self {
        let h = message.hash();
        let signatures = keypairs.iter().map(|kp| kp.sign(&h)).collect();
        Self {
            version: 0,
            signatures,
            message,
        }
    }

    pub fn message_hash(&self) -> [u8; 32] {
        self.message.hash()
    }

    /// True when signature count matches signer count. Cheap structural check;
    /// [`verify_signatures`](Self::verify_signatures) is the real cryptographic gate.
    pub fn verify_signature_count(&self) -> bool {
        self.signatures.len() == self.message.signers.len()
    }

    /// Verify every signer's BIP-340 Schnorr signature over the message hash.
    /// Returns `false` if the signature count is wrong or any signature fails to
    /// verify against its signer pubkey. This is the node's admission gate.
    pub fn verify_signatures(&self) -> bool {
        if self.signatures.len() != self.message.signers.len() {
            return false;
        }
        let h = self.message.hash();
        self.signatures
            .iter()
            .zip(&self.message.signers)
            .all(|(sig, signer)| sig.verify(&h, signer))
    }

    /// Replay & cross-network checks: the message's `chain_id` must match `chain_id`,
    /// and its `recent_blockhash` must be in `valid_blockhashes` (the node's current
    /// recent-window set). Returns `Err(reason)` on failure. Signature verification
    /// ([`verify_signatures`](Self::verify_signatures)) and txid dedup are separate.
    pub fn check_chain_and_blockhash(
        &self,
        chain_id: u64,
        valid_blockhashes: &HashSet<[u8; 32]>,
    ) -> Result<(), &'static str> {
        if self.message.chain_id != chain_id {
            return Err("wrong chain id");
        }
        if !valid_blockhashes.contains(&self.message.recent_blockhash) {
            return Err("blockhash not found or expired");
        }
        Ok(())
    }
}

/// A committed block produced by the HIMSHA node.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Block {
    pub slot: u64,
    pub parent_slot: u64,
    pub transactions: Vec<RuntimeTransaction>,
    /// SHA-256 of (slot || all message hashes).
    pub blockhash: [u8; 32],
    pub timestamp: u64,
}

impl Block {
    pub fn new(
        slot: u64,
        parent_slot: u64,
        transactions: Vec<RuntimeTransaction>,
        timestamp: u64,
    ) -> Self {
        let mut h = Sha256::new();
        h.update(slot.to_le_bytes());
        for tx in &transactions {
            h.update(tx.message_hash());
        }
        Self {
            slot,
            parent_slot,
            transactions,
            blockhash: h.finalize().into(),
            timestamp,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pubkey::Keypair;

    fn msg_for(signers: Vec<Pubkey>) -> Message {
        // A trivial instruction payload — content is irrelevant to signing.
        let ix = Instruction::new(Pubkey::from([1u8; 32]), vec![], vec![1, 2, 3]);
        Message::new(signers, vec![ix], 42)
    }

    #[test]
    fn signed_tx_verifies() {
        let kp = Keypair::generate();
        let tx = RuntimeTransaction::signed(msg_for(vec![kp.pubkey()]), &[&kp]);
        assert!(tx.verify_signatures());
        assert!(tx.verify_signature_count());
    }

    #[test]
    fn unsigned_tx_is_rejected() {
        let kp = Keypair::generate();
        let tx = RuntimeTransaction::unsigned(msg_for(vec![kp.pubkey()]));
        assert!(tx.verify_signature_count()); // count is fine...
        assert!(!tx.verify_signatures()); // ...but the zero signature fails
    }

    #[test]
    fn wrong_signer_is_rejected() {
        // Signed by `b`, but the message claims `a` is the signer.
        let a = Keypair::generate();
        let b = Keypair::generate();
        let tx = RuntimeTransaction::signed(msg_for(vec![a.pubkey()]), &[&b]);
        assert!(!tx.verify_signatures());
    }

    #[test]
    fn tampered_message_is_rejected() {
        let kp = Keypair::generate();
        let mut tx = RuntimeTransaction::signed(msg_for(vec![kp.pubkey()]), &[&kp]);
        assert!(tx.verify_signatures());
        tx.message.timestamp += 1; // change the signed body after the fact
        assert!(!tx.verify_signatures());
    }

    #[test]
    fn multisig_all_valid_passes_one_bad_fails() {
        let a = Keypair::generate();
        let b = Keypair::generate();
        let msg = msg_for(vec![a.pubkey(), b.pubkey()]);
        let good = RuntimeTransaction::signed(msg.clone(), &[&a, &b]);
        assert!(good.verify_signatures());

        // Swap one signature for a wrong-key signature → must fail.
        let c = Keypair::generate();
        let bad = RuntimeTransaction::signed(msg, &[&a, &c]);
        assert!(!bad.verify_signatures());
    }

    #[test]
    fn hash_derived_pubkey_cannot_sign() {
        // A `from_seed` ID is not a valid x-only key, so no signature verifies for it.
        let kp = Keypair::generate();
        let tx = RuntimeTransaction::signed(msg_for(vec![Pubkey::from_seed(b"not-a-key")]), &[&kp]);
        assert!(!tx.verify_signatures());
    }

    #[test]
    fn deterministic_signature() {
        let kp = Keypair::generate();
        let h = msg_for(vec![kp.pubkey()]).hash();
        assert_eq!(kp.sign(&h), kp.sign(&h)); // no-aux-rand → reproducible
    }

    // ---- replay protection ----

    fn signed_with(kp: &Keypair, blockhash: [u8; 32], chain_id: u64) -> RuntimeTransaction {
        let ix = Instruction::new(Pubkey::from([1u8; 32]), vec![], vec![9]);
        let msg = Message::new_signed(vec![kp.pubkey()], vec![ix], blockhash, chain_id, 7);
        RuntimeTransaction::signed(msg, &[kp])
    }

    #[test]
    fn recent_blockhash_and_chain_id_are_signed() {
        // Swapping either field after signing must invalidate the signature.
        let kp = Keypair::generate();
        let mut tx = signed_with(&kp, [7u8; 32], 1);
        assert!(tx.verify_signatures());

        let mut bh = tx.clone();
        bh.message.recent_blockhash = [8u8; 32];
        assert!(
            !bh.verify_signatures(),
            "recent_blockhash must be bound into the signature"
        );

        tx.message.chain_id = 2;
        assert!(
            !tx.verify_signatures(),
            "chain_id must be bound into the signature"
        );
    }

    #[test]
    fn admission_accepts_known_blockhash_and_chain() {
        let kp = Keypair::generate();
        let tx = signed_with(&kp, [7u8; 32], 1);
        let valid: HashSet<[u8; 32]> = [[7u8; 32]].into_iter().collect();
        assert_eq!(tx.check_chain_and_blockhash(1, &valid), Ok(()));
    }

    #[test]
    fn admission_rejects_wrong_chain() {
        let kp = Keypair::generate();
        let tx = signed_with(&kp, [7u8; 32], 1);
        let valid: HashSet<[u8; 32]> = [[7u8; 32]].into_iter().collect();
        assert_eq!(
            tx.check_chain_and_blockhash(2, &valid),
            Err("wrong chain id")
        );
    }

    #[test]
    fn admission_rejects_unknown_or_expired_blockhash() {
        let kp = Keypair::generate();
        let tx = signed_with(&kp, [7u8; 32], 1);
        // The node's recent window no longer contains [7;32] (it aged out).
        let valid: HashSet<[u8; 32]> = [[9u8; 32]].into_iter().collect();
        assert_eq!(
            tx.check_chain_and_blockhash(1, &valid),
            Err("blockhash not found or expired")
        );
    }
}
