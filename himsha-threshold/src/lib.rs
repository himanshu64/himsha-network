//! FROST threshold-Schnorr signing committee for HIMSHA's Bitcoin settlement key.
//!
//! The settlement key is split **M-of-N** across independent signers so no single
//! party can move funds — the custody-decentralization half of the ZK-native design
//! (see docs/decentralization.md). A coordinator gathers ≥ `threshold` partial
//! signatures and aggregates them into **one** Schnorr signature that verifies under
//! the group public key.
//!
//! This wraps the audited `frost-secp256k1` crate. The signing flow:
//!   1. keygen (trusted dealer here; DKG is a drop-in upgrade) → key shares + group key
//!   2. round 1: each chosen signer commits nonces
//!   3. round 2: each signs a share over the message (the Bitcoin sighash)
//!   4. aggregate → group signature; verify under the group key
//!
//! Bitcoin note: this produces a secp256k1 Schnorr signature. For Taproot key-spend
//! the production path uses the Taproot variant (`frost-secp256k1-tr`) to handle the
//! BIP-341 tweak / even-Y normalization; the M-of-N flow is identical.

use std::collections::BTreeMap;

use frost::Identifier;
use frost_secp256k1 as frost;
use thiserror::Error;

pub mod taproot;

#[derive(Debug, Error)]
pub enum ThresholdError {
    #[error("need at least {needed} signers, got {got}")]
    NotEnoughSigners { needed: u16, got: usize },
    #[error("unknown signer id")]
    UnknownSigner,
    #[error("frost error: {0}")]
    Frost(String),
    #[error("robust signing exhausted: {excluded} signer(s) disruptive, fewer than threshold honest remain")]
    RobustExhausted { excluded: usize },
}

/// Result of a [`Committee::sign_robust`] / [`taproot::TaprootCommittee::sign_robust`]
/// session: the aggregate signature plus how many retry rounds it took and how many
/// disruptive signers were identified and excluded (the ROAST robustness guarantee).
#[derive(Debug, Clone)]
pub struct RobustSignature {
    /// The aggregate Schnorr signature (verifies under the group key).
    pub signature: Vec<u8>,
    /// Number of signing rounds attempted (1 if no signer misbehaved).
    pub rounds: u32,
    /// Number of disruptive signers identified by the aggregator and dropped.
    pub excluded: usize,
}

impl From<frost::Error> for ThresholdError {
    fn from(e: frost::Error) -> Self {
        ThresholdError::Frost(e.to_string())
    }
}

/// An M-of-N FROST signing committee holding the settlement key shares.
///
/// In a real deployment each `KeyPackage` lives on a *separate* signer and the
/// rounds happen over the network; here they're held together so the flow is
/// self-contained and testable.
pub struct Committee {
    threshold: u16,
    key_packages: BTreeMap<Identifier, frost::keys::KeyPackage>,
    pubkeys: frost::keys::PublicKeyPackage,
}

impl Committee {
    /// Generate an `threshold`-of-`total` committee via a trusted dealer.
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

    /// Generate the committee via **distributed key generation** (DKG) — no trusted
    /// dealer ever holds the full key. Runs FROST's 3-round DKG for all participants
    /// (here simulated in-process; in production each round-trip is over the network).
    /// The resulting key shares + group key are identical in form to `generate`.
    pub fn generate_dkg(threshold: u16, total: u16) -> Result<Self, ThresholdError> {
        use frost::keys::dkg;
        let mut rng = rand::thread_rng();

        let ids: Vec<Identifier> = (1..=total)
            .map(|i| Identifier::try_from(i).map_err(ThresholdError::from))
            .collect::<Result<_, _>>()?;

        // Round 1 — each participant broadcasts a commitment package.
        let mut r1_secret: BTreeMap<Identifier, dkg::round1::SecretPackage> = BTreeMap::new();
        let mut r1_pkg: BTreeMap<Identifier, dkg::round1::Package> = BTreeMap::new();
        for id in &ids {
            let (sec, pkg) = dkg::part1(*id, total, threshold, &mut rng)?;
            r1_secret.insert(*id, sec);
            r1_pkg.insert(*id, pkg);
        }

        // Round 2 — each participant sends a package to every other participant.
        let mut r2_secret: BTreeMap<Identifier, dkg::round2::SecretPackage> = BTreeMap::new();
        let mut r2_inbox: BTreeMap<Identifier, BTreeMap<Identifier, dkg::round2::Package>> =
            ids.iter().map(|id| (*id, BTreeMap::new())).collect();
        for id in &ids {
            let others: BTreeMap<Identifier, dkg::round1::Package> = r1_pkg
                .iter()
                .filter(|(k, _)| **k != *id)
                .map(|(k, v)| (*k, v.clone()))
                .collect();
            let secret = r1_secret.remove(id).ok_or(ThresholdError::UnknownSigner)?;
            let (sec2, outgoing) = dkg::part2(secret, &others)?;
            r2_secret.insert(*id, sec2);
            for (recipient, pkg) in outgoing {
                r2_inbox
                    .get_mut(&recipient)
                    .ok_or(ThresholdError::UnknownSigner)?
                    .insert(*id, pkg);
            }
        }

        // Round 3 — each participant derives its key share + the shared group key.
        let mut key_packages = BTreeMap::new();
        let mut pubkeys: Option<frost::keys::PublicKeyPackage> = None;
        for id in &ids {
            let others: BTreeMap<Identifier, dkg::round1::Package> = r1_pkg
                .iter()
                .filter(|(k, _)| **k != *id)
                .map(|(k, v)| (*k, v.clone()))
                .collect();
            let (kp, pubpkg) = dkg::part3(&r2_secret[id], &others, &r2_inbox[id])?;
            key_packages.insert(*id, kp);
            pubkeys = Some(pubpkg);
        }

        Ok(Self {
            threshold,
            key_packages,
            pubkeys: pubkeys.ok_or(ThresholdError::NotEnoughSigners { needed: 1, got: 0 })?,
        })
    }

    pub fn threshold(&self) -> u16 {
        self.threshold
    }
    pub fn total(&self) -> usize {
        self.key_packages.len()
    }

    /// All signer identifiers (e.g. for choosing a signing quorum).
    pub fn signer_ids(&self) -> Vec<Identifier> {
        self.key_packages.keys().copied().collect()
    }

    /// The group (aggregate) public key — this is the on-chain settlement key.
    pub fn group_public_key(&self) -> Vec<u8> {
        self.pubkeys.verifying_key().serialize().unwrap_or_default()
    }

    /// Threshold-sign `message` (the Bitcoin sighash) with the given signer quorum.
    /// Runs both FROST rounds and aggregates into one Schnorr signature.
    pub fn sign(&self, message: &[u8], signers: &[Identifier]) -> Result<Vec<u8>, ThresholdError> {
        if (signers.len() as u16) < self.threshold {
            return Err(ThresholdError::NotEnoughSigners {
                needed: self.threshold,
                got: signers.len(),
            });
        }
        let mut rng = rand::thread_rng();

        // Round 1 — each signer produces nonces + public commitments.
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

        // Coordinator binds the commitments + message into a signing package.
        let signing_package = frost::SigningPackage::new(commitments, message);

        // Round 2 — each signer produces a signature share.
        let mut shares = BTreeMap::new();
        for id in signers {
            let kp = self
                .key_packages
                .get(id)
                .ok_or(ThresholdError::UnknownSigner)?;
            let share = frost::round2::sign(&signing_package, &nonces[id], kp)?;
            shares.insert(*id, share);
        }

        // Aggregate the shares into a single group Schnorr signature.
        let group_sig = frost::aggregate(&signing_package, &shares, &self.pubkeys)?;
        group_sig.serialize().map_err(ThresholdError::from)
    }

    /// **ROAST-style robust signing.** Produce a valid aggregate signature even when
    /// some signers are *disruptive* (send invalid/equivocating shares), as long as at
    /// least `threshold` honest signers are online.
    ///
    /// Plain [`sign`](Self::sign) aborts the whole round if any chosen signer misbehaves.
    /// ROAST instead makes the coordinator resilient: it attempts a quorum, and if the
    /// aggregator identifies a signer whose share fails verification (`culprit`), that
    /// signer is excluded and the round retried with the remaining honest signers —
    /// repeating until a valid signature is produced or fewer than `threshold` honest
    /// signers remain — t-of-n **ROAST** robust-threshold custody.
    ///
    /// `online` is the set believed available; `disruptive` is a Byzantine-fault model
    /// for testing — those signers submit invalid shares. **In production `disruptive`
    /// is empty**: real faults (a bad/missing share) are discovered the same way, via the
    /// aggregator's culprit identification, and trigger the same exclude-and-retry path.
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

            // Round 1 — nonce commitments from the chosen quorum.
            let mut nonces = BTreeMap::new();
            let mut commitments = BTreeMap::new();
            for id in &quorum {
                let kp = &self.key_packages[id];
                let (n, c) = frost::round1::commit(kp.signing_share(), &mut rng);
                nonces.insert(*id, n);
                commitments.insert(*id, c);
            }
            let signing_package = frost::SigningPackage::new(commitments.clone(), message);
            // A disruptive signer signs over a different package → its share won't verify.
            let bad_package = frost::SigningPackage::new(commitments, b"BYZANTINE-EQUIVOCATION");

            // Round 2 — honest vs disruptive shares.
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

            // Aggregate. On a bad share the aggregator names the culprit → drop & retry.
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

    /// Verify an aggregated signature against the group key.
    pub fn verify(&self, message: &[u8], signature: &[u8]) -> bool {
        let sig = match frost::Signature::deserialize(signature) {
            Ok(s) => s,
            Err(_) => return false,
        };
        self.pubkeys.verifying_key().verify(message, &sig).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_2_of_3_signs_and_verifies() {
        let committee = Committee::generate(2, 3).unwrap();
        assert_eq!(committee.threshold(), 2);
        assert_eq!(committee.total(), 3);

        let msg = b"bitcoin-settlement-sighash-32byte!"; // stand-in sighash
        let ids = committee.signer_ids();

        // Any quorum of 2 produces a valid aggregate signature.
        let sig = committee.sign(msg, &ids[..2]).unwrap();
        assert!(committee.verify(msg, &sig));

        // A different quorum of 2 also verifies (subset-independence).
        let sig2 = committee.sign(msg, &ids[1..3]).unwrap();
        assert!(committee.verify(msg, &sig2));
    }

    #[test]
    fn test_full_quorum_signs() {
        let committee = Committee::generate(2, 3).unwrap();
        let ids = committee.signer_ids();
        let sig = committee.sign(b"msg", &ids).unwrap();
        assert!(committee.verify(b"msg", &sig));
    }

    #[test]
    fn test_below_threshold_rejected() {
        let committee = Committee::generate(3, 5).unwrap();
        let ids = committee.signer_ids();
        // Only 2 signers for a 3-of-5 committee → refused before signing.
        let err = committee.sign(b"msg", &ids[..2]).unwrap_err();
        assert!(matches!(
            err,
            ThresholdError::NotEnoughSigners { needed: 3, got: 2 }
        ));
    }

    #[test]
    fn test_tampered_message_fails_verification() {
        let committee = Committee::generate(2, 3).unwrap();
        let ids = committee.signer_ids();
        let sig = committee.sign(b"original", &ids[..2]).unwrap();
        assert!(!committee.verify(b"tampered", &sig));
    }

    #[test]
    fn test_dkg_committee_signs_and_verifies() {
        // No trusted dealer — keys generated via distributed key generation.
        let committee = Committee::generate_dkg(2, 3).unwrap();
        assert_eq!(committee.threshold(), 2);
        assert_eq!(committee.total(), 3);
        assert!(!committee.group_public_key().is_empty());

        let msg = b"dkg-settlement-sighash";
        let ids = committee.signer_ids();
        let sig = committee.sign(msg, &ids[..2]).unwrap();
        assert!(committee.verify(msg, &sig));
        assert!(!committee.verify(b"other", &sig));
    }

    #[test]
    fn test_group_key_is_stable() {
        let committee = Committee::generate(2, 3).unwrap();
        let k1 = committee.group_public_key();
        let k2 = committee.group_public_key();
        assert_eq!(k1, k2);
        assert!(!k1.is_empty());
    }

    #[test]
    fn test_robust_signing_excludes_disruptive_and_succeeds() {
        // 3-of-5: up to 2 signers may be Byzantine and we still get a valid signature.
        let committee = Committee::generate(3, 5).unwrap();
        let ids = committee.signer_ids();
        let msg = b"settlement-with-byzantine-signers";

        // Two disruptive signers (the ones the quorum picks first) get identified & dropped.
        let robust = committee.sign_robust(msg, &ids, &ids[..2]).unwrap();
        assert_eq!(robust.excluded, 2);
        assert!(robust.rounds >= 3); // 2 culprit-discovery rounds + 1 clean
        assert!(committee.verify(msg, &robust.signature));
    }

    #[test]
    fn test_robust_signing_clean_path_single_round() {
        let committee = Committee::generate(2, 3).unwrap();
        let ids = committee.signer_ids();
        let robust = committee.sign_robust(b"msg", &ids, &[]).unwrap();
        assert_eq!(robust.rounds, 1);
        assert_eq!(robust.excluded, 0);
        assert!(committee.verify(b"msg", &robust.signature));
    }

    #[test]
    fn test_robust_signing_exhausts_when_too_many_disruptive() {
        // 3-of-5 with 3 disruptive signers → only 2 honest remain (< threshold) → error.
        let committee = Committee::generate(3, 5).unwrap();
        let ids = committee.signer_ids();
        let err = committee.sign_robust(b"msg", &ids, &ids[..3]).unwrap_err();
        assert!(matches!(err, ThresholdError::RobustExhausted { .. }));
    }
}
