use borsh::{BorshDeserialize, BorshSerialize};
use secp256k1::{Keypair as Secp256k1Keypair, Message as Secp256k1Message, Secp256k1};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

use crate::signature::Signature;

/// Fixed bump used by [`Pubkey::find_program_address`]. This PoC derivation does
/// no off-curve search, so the bump is constant rather than discovered.
pub const PDA_BUMP: u8 = 255;

/// 32-byte identifier for accounts, programs, and signers.
#[derive(
    Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default,
    BorshSerialize, BorshDeserialize,
    Serialize, Deserialize,
)]
pub struct Pubkey([u8; 32]);

impl Pubkey {
    pub const fn new(bytes: [u8; 32]) -> Self { Self(bytes) }

    /// Deterministic key from a human-readable seed (used for well-known IDs).
    pub fn from_seed(seed: &[u8]) -> Self {
        let hash: [u8; 32] = Sha256::digest(seed).into();
        Self(hash)
    }

    /// Generate a cryptographically random key (for testing or new accounts).
    pub fn new_unique() -> Self {
        use rand::RngCore;
        let mut bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        Self(bytes)
    }

    /// Derive a Program Derived Address from seeds + program id.
    ///
    /// NOTE: simplified, educational derivation. Unlike Solana's
    /// `find_program_address`, it does **not** search bumps for an address that
    /// lies *off* the signing curve — there is no curve check at all. It hashes
    /// the seeds, a fixed bump, the program id, and a domain tag, so the bump is
    /// always [`PDA_BUMP`]. Do not rely on the "no private key exists for this
    /// address" guarantee that real PDAs provide.
    pub fn find_program_address(seeds: &[&[u8]], program_id: &Pubkey) -> (Pubkey, u8) {
        let mut hasher = Sha256::new();
        for s in seeds { hasher.update(s); }
        hasher.update(&[PDA_BUMP]);
        hasher.update(program_id.as_ref());
        hasher.update(b"himsha::pda");
        let hash: [u8; 32] = hasher.finalize().into();
        (Pubkey(hash), PDA_BUMP)
    }

    pub fn as_bytes(&self) -> &[u8; 32] { &self.0 }

    pub fn to_base58(&self) -> String { bs58::encode(&self.0).into_string() }

    pub fn from_base58(s: &str) -> Result<Self, String> {
        bs58::decode(s)
            .into_vec()
            .map_err(|e| e.to_string())
            .and_then(|v| {
                v.try_into()
                    .map(Self)
                    .map_err(|_| "expected 32 bytes".into())
            })
    }
}

impl AsRef<[u8]> for Pubkey {
    fn as_ref(&self) -> &[u8] { &self.0 }
}

impl From<[u8; 32]> for Pubkey {
    fn from(b: [u8; 32]) -> Self { Self(b) }
}

impl From<Pubkey> for [u8; 32] {
    fn from(p: Pubkey) -> Self { p.0 }
}

impl fmt::Display for Pubkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_base58())
    }
}

impl fmt::Debug for Pubkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Pk({}…)", &self.to_base58()[..8])
    }
}

/// A secp256k1 signing keypair whose BIP-340 x-only public key *is* the account
/// [`Pubkey`]. Sign a transaction's message hash with [`Keypair::sign`]; the node
/// verifies the resulting [`Signature`] against the signer pubkey. Note: only
/// keypair-backed pubkeys can sign — hash-derived IDs and PDAs cannot.
pub struct Keypair(Secp256k1Keypair);

impl Keypair {
    /// Generate a fresh random keypair.
    pub fn generate() -> Self {
        Self(Secp256k1Keypair::new(&Secp256k1::new(), &mut rand::thread_rng()))
    }

    /// Reconstruct a keypair from a 32-byte secret key.
    pub fn from_secret_bytes(secret: &[u8; 32]) -> Result<Self, String> {
        Secp256k1Keypair::from_seckey_slice(&Secp256k1::new(), secret)
            .map(Self)
            .map_err(|e| e.to_string())
    }

    /// The x-only public key, as a HIMSHA [`Pubkey`].
    pub fn pubkey(&self) -> Pubkey {
        Pubkey(self.0.x_only_public_key().0.serialize())
    }

    /// Sign a 32-byte message hash (e.g. [`crate::transaction::Message::hash`]),
    /// producing a BIP-340 Schnorr signature. Deterministic (no auxiliary
    /// randomness) so the same message always yields the same signature.
    pub fn sign(&self, msg_hash: &[u8; 32]) -> Signature {
        let msg = Secp256k1Message::from_digest(*msg_hash);
        let sig = Secp256k1::new().sign_schnorr_no_aux_rand(&msg, &self.0);
        let mut bytes = [0u8; 64];
        bytes.copy_from_slice(sig.as_ref());
        Signature::new(bytes)
    }
}
