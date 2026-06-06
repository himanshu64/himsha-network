//! Taproot-compatible FROST committee (BIP-340/341) for Bitcoin **key-spend**.
//!
//! Same M-of-N flow as [`crate::Committee`], but over `frost-secp256k1-tr`, which
//! applies the BIP-341 taproot tweak / even-Y normalization so the aggregate
//! signature is valid for a Taproot key-path spend under the group output key.
//! Use [`group_xonly`] as the 32-byte Taproot output key and feed the BIP-341
//! sighash to [`sign`].

use std::collections::BTreeMap;

use frost::Identifier;
use frost_secp256k1_tr as frost;

use crate::{RobustSignature, ThresholdError};

impl From<frost::Error> for ThresholdError {
    fn from(e: frost::Error) -> Self {
        ThresholdError::Frost(e.to_string())
    }
}

/// An M-of-N Taproot signing committee.
pub struct TaprootCommittee {
    threshold: u16,
    key_packages: BTreeMap<Identifier, frost::keys::KeyPackage>,
    pubkeys: frost::keys::PublicKeyPackage,
}

impl TaprootCommittee {
    /// Trusted-dealer keygen.
    pub fn generate(threshold: u16, total: u16) -> Result<Self, ThresholdError> {
        let mut rng = rand::thread_rng();
        let (shares, pubkeys) = frost::keys::generate_with_dealer(
            total,
            threshold,
            frost::keys::IdentifierList::Default,
            &mut rng,
        )?;
        let mut key_packages = BTreeMap::new();
        for (id, secret_share) in shares {
            key_packages.insert(id, frost::keys::KeyPackage::try_from(secret_share)?);
        }
        Ok(Self {
            threshold,
            key_packages,
            pubkeys,
        })
    }

    pub fn threshold(&self) -> u16 {
        self.threshold
    }
    pub fn total(&self) -> usize {
        self.key_packages.len()
    }
    pub fn signer_ids(&self) -> Vec<Identifier> {
        self.key_packages.keys().copied().collect()
    }

    /// Serialized group verifying key.
    pub fn group_key(&self) -> Vec<u8> {
        self.pubkeys.verifying_key().serialize().unwrap_or_default()
    }

    /// 32-byte x-only Taproot output key (drops the parity byte if present).
    pub fn group_xonly(&self) -> [u8; 32] {
        let bytes = self.group_key();
        let mut x = [0u8; 32];
        let src = if bytes.len() == 33 {
            &bytes[1..33]
        } else if bytes.len() >= 32 {
            &bytes[..32]
        } else {
            &bytes[..]
        };
        x[..src.len().min(32)].copy_from_slice(&src[..src.len().min(32)]);
        x
    }

    /// Threshold-sign a 32-byte message (the BIP-341 key-spend sighash). Returns the
    /// 64-byte Schnorr signature for the Taproot key-path witness.
    pub fn sign(&self, message: &[u8], signers: &[Identifier]) -> Result<Vec<u8>, ThresholdError> {
        if (signers.len() as u16) < self.threshold {
            return Err(ThresholdError::NotEnoughSigners {
                needed: self.threshold,
                got: signers.len(),
            });
        }
        let mut rng = rand::thread_rng();
        let mut nonces = BTreeMap::new();
        let mut commitments = BTreeMap::new();
        for id in signers {
            let kp = self
                .key_packages
                .get(id)
                .ok_or(ThresholdError::UnknownSigner)?;
            let (n, c) = frost::round1::commit(kp.signing_share(), &mut rng);
            nonces.insert(*id, n);
            commitments.insert(*id, c);
        }
        let signing_package = frost::SigningPackage::new(commitments, message);
        let mut shares = BTreeMap::new();
        for id in signers {
            let kp = self
                .key_packages
                .get(id)
                .ok_or(ThresholdError::UnknownSigner)?;
            shares.insert(*id, frost::round2::sign(&signing_package, &nonces[id], kp)?);
        }
        let group_sig = frost::aggregate(&signing_package, &shares, &self.pubkeys)?;
        group_sig.serialize().map_err(ThresholdError::from)
    }

    /// **ROAST-style robust signing** for Taproot key-spend (see
    /// [`crate::Committee::sign_robust`] for the full rationale). Tolerates disruptive
    /// signers by identifying the culprit, excluding it, and retrying with the remaining
    /// honest signers until a valid Taproot Schnorr signature is produced.
    pub fn sign_robust(
        &self,
        message: &[u8],
        online: &[Identifier],
        disruptive: &[Identifier],
    ) -> Result<RobustSignature, ThresholdError> {
        let mut candidates: Vec<Identifier> = online
            .iter()
            .copied()
            .filter(|id| self.key_packages.contains_key(id))
            .collect();
        let mut excluded = 0usize;
        let mut rounds = 0u32;
        let mut rng = rand::thread_rng();

        loop {
            if (candidates.len() as u16) < self.threshold {
                return Err(ThresholdError::RobustExhausted { excluded });
            }
            rounds += 1;
            let quorum: Vec<Identifier> = candidates
                .iter()
                .copied()
                .take(self.threshold as usize)
                .collect();

            let mut nonces = BTreeMap::new();
            let mut commitments = BTreeMap::new();
            for id in &quorum {
                let kp = &self.key_packages[id];
                let (n, c) = frost::round1::commit(kp.signing_share(), &mut rng);
                nonces.insert(*id, n);
                commitments.insert(*id, c);
            }
            let signing_package = frost::SigningPackage::new(commitments.clone(), message);
            let bad_package = frost::SigningPackage::new(commitments, b"BYZANTINE-EQUIVOCATION");

            let mut shares = BTreeMap::new();
            for id in &quorum {
                let kp = &self.key_packages[id];
                let pkg = if disruptive.contains(id) {
                    &bad_package
                } else {
                    &signing_package
                };
                shares.insert(*id, frost::round2::sign(pkg, &nonces[id], kp)?);
            }

            match frost::aggregate(&signing_package, &shares, &self.pubkeys) {
                Ok(group_sig) => {
                    return Ok(RobustSignature {
                        signature: group_sig.serialize().map_err(ThresholdError::from)?,
                        rounds,
                        excluded,
                    });
                }
                Err(e) => match e.culprit() {
                    Some(bad) => {
                        excluded += 1;
                        candidates.retain(|c| *c != bad);
                    }
                    None => return Err(ThresholdError::from(e)),
                },
            }
        }
    }

    pub fn verify(&self, message: &[u8], signature: &[u8]) -> bool {
        match frost::Signature::deserialize(signature) {
            Ok(sig) => self.pubkeys.verifying_key().verify(message, &sig).is_ok(),
            Err(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_taproot_committee_signs_and_verifies() {
        let committee = TaprootCommittee::generate(2, 3).unwrap();
        assert_eq!(committee.group_xonly().len(), 32);

        // A 32-byte stand-in for a BIP-341 key-spend sighash.
        let sighash = [0x11u8; 32];
        let ids = committee.signer_ids();
        let sig = committee.sign(&sighash, &ids[..2]).unwrap();
        assert_eq!(sig.len(), 64); // Taproot key-path Schnorr sig
        assert!(committee.verify(&sighash, &sig));
        assert!(!committee.verify(&[0x22u8; 32], &sig));
    }

    #[test]
    fn test_taproot_below_threshold_rejected() {
        let committee = TaprootCommittee::generate(3, 5).unwrap();
        let ids = committee.signer_ids();
        assert!(committee.sign(&[0u8; 32], &ids[..2]).is_err());
    }

    #[test]
    fn test_taproot_robust_signing_tolerates_disruptive_signer() {
        // 3-of-5 Taproot committee; one signer is Byzantine. ROAST still settles.
        let committee = TaprootCommittee::generate(3, 5).unwrap();
        let ids = committee.signer_ids();
        let sighash = [0x42u8; 32];

        // The first signer (which the quorum would pick first) is disruptive.
        let robust = committee.sign_robust(&sighash, &ids, &ids[..1]).unwrap();
        assert!(
            robust.excluded >= 1,
            "the disruptive signer must be identified & dropped"
        );
        assert!(
            robust.rounds >= 2,
            "at least one retry after excluding the culprit"
        );
        assert_eq!(robust.signature.len(), 64);
        assert!(committee.verify(&sighash, &robust.signature));
    }

    #[test]
    fn test_taproot_robust_signing_clean_path() {
        // No disruptive signers → succeeds in a single round, no exclusions.
        let committee = TaprootCommittee::generate(2, 3).unwrap();
        let ids = committee.signer_ids();
        let robust = committee.sign_robust(&[0x07u8; 32], &ids, &[]).unwrap();
        assert_eq!(robust.rounds, 1);
        assert_eq!(robust.excluded, 0);
        assert!(committee.verify(&[0x07u8; 32], &robust.signature));
    }
}
