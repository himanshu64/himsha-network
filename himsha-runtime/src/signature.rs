use borsh::{BorshDeserialize, BorshSerialize};
use secp256k1::{schnorr, Message as Secp256k1Message, Secp256k1, XOnlyPublicKey};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::pubkey::Pubkey;

/// 64-byte Schnorr signature (BIP-340).
/// Serialized as a hex string in JSON; raw bytes in borsh.
#[derive(Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Signature(pub [u8; 64]);

impl Signature {
    pub fn new(b: [u8; 64]) -> Self {
        Self(b)
    }
    pub fn zeroed() -> Self {
        Self([0u8; 64])
    }
    pub fn as_bytes(&self) -> &[u8; 64] {
        &self.0
    }

    /// Verify this BIP-340 Schnorr signature over `msg_hash` against `pubkey`,
    /// interpreting the 32-byte pubkey as an x-only secp256k1 public key.
    ///
    /// Returns `false` on any parse failure — notably when `pubkey` is not a valid
    /// curve point (true for hash-derived IDs and program-derived addresses), which
    /// is correct: such accounts can never be transaction signers. The all-zero
    /// signature also fails, so an unsigned transaction is rejected.
    pub fn verify(&self, msg_hash: &[u8; 32], pubkey: &Pubkey) -> bool {
        let Ok(sig) = schnorr::Signature::from_slice(&self.0) else {
            return false;
        };
        let Ok(xonly) = XOnlyPublicKey::from_slice(pubkey.as_bytes()) else {
            return false;
        };
        let msg = Secp256k1Message::from_digest(*msg_hash);
        Secp256k1::verification_only()
            .verify_schnorr(&sig, &msg, &xonly)
            .is_ok()
    }
}

impl AsRef<[u8]> for Signature {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<[u8; 64]> for Signature {
    fn from(b: [u8; 64]) -> Self {
        Self(b)
    }
}

impl std::fmt::Debug for Signature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Sig({}…)", hex::encode(&self.0[..8]))
    }
}

impl std::fmt::Display for Signature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

// ---- serde: hex string ----

impl Serialize for Signature {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        serde::Serialize::serialize(&hex::encode(self.0), s)
    }
}

impl<'de> Deserialize<'de> for Signature {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let hex_str = <String as serde::Deserialize>::deserialize(d)?;
        let bytes = hex::decode(&hex_str).map_err(serde::de::Error::custom)?;
        if bytes.len() != 64 {
            return Err(serde::de::Error::custom("expected 64-byte signature"));
        }
        let mut arr = [0u8; 64];
        arr.copy_from_slice(&bytes);
        Ok(Self(arr))
    }
}
