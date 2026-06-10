//! Threshold-custody configuration for Bitcoin settlement.
//!
//! When `HIMSHA_THRESHOLD="M/N"` is set (e.g. `2/3`), the node builds a FROST
//! [`TaprootCommittee`] whose M-of-N group key owns the settlement funds, and
//! routes inscription settlement through a threshold-signed Taproot key-spend
//! ([`crate::settlement_tx::settle_with_committee`]) instead of a single hot
//! wallet. Unset → the hot-wallet path is used (the prior default).
//!
//! ⚠️ **EDUCATIONAL co-location.** This in-process committee generates and holds
//! *all* M-of-N key shares in one process, so it does not actually decentralize
//! custody — it exercises the FROST signing path end-to-end. A real deployment
//! runs each signer on a separate host (DKG over the network, see
//! [`himsha_threshold::Committee::generate_dkg`]) and never co-locates shares.
//! End-to-end Bitcoin acceptance is also unverified without a regtest node that
//! funds the committee's P2TR address — see [`crate::settlement_tx`].

use std::sync::Arc;

use himsha_threshold::taproot::TaprootCommittee;
use tracing::{info, warn};

/// A configured threshold-signing committee for settlement.
pub struct Custody {
    pub committee: Arc<TaprootCommittee>,
    pub threshold: u16,
    pub total: u16,
}

impl Custody {
    /// Build from `HIMSHA_THRESHOLD="M/N"`. Returns `None` when the variable is
    /// unset (hot-wallet settlement) or malformed (logged, then disabled).
    pub fn from_env() -> Option<Self> {
        let spec = std::env::var("HIMSHA_THRESHOLD").ok()?;
        let (m, n) = match parse_spec(&spec) {
            Ok(mn) => mn,
            Err(e) => {
                warn!("threshold custody disabled: bad HIMSHA_THRESHOLD '{spec}': {e}");
                return None;
            }
        };
        match TaprootCommittee::generate(m, n) {
            Ok(committee) => {
                warn!(
                    "threshold custody ENABLED ({m}-of-{n}) — EDUCATIONAL: all key \
                     shares are co-located in this process (not real custody \
                     decentralization). See docs/decentralization.md."
                );
                info!(
                    "committee group key (x-only): {}",
                    hex::encode(committee.group_xonly())
                );
                Some(Self {
                    committee: Arc::new(committee),
                    threshold: m,
                    total: n,
                })
            }
            Err(e) => {
                warn!("threshold custody disabled: committee generation failed: {e}");
                None
            }
        }
    }

    /// The committee's 32-byte x-only group key — the Taproot output key that
    /// must own the settlement UTXOs for a key-spend to verify.
    pub fn group_xonly(&self) -> [u8; 32] {
        self.committee.group_xonly()
    }
}

/// Parse an `"M/N"` threshold spec into `(threshold, total)`, enforcing
/// `1 <= M <= N`.
fn parse_spec(s: &str) -> Result<(u16, u16), String> {
    let (m, n) = s
        .split_once('/')
        .ok_or("expected the form M/N (e.g. 2/3)")?;
    let m: u16 = m.trim().parse().map_err(|_| "M is not a number")?;
    let n: u16 = n.trim().parse().map_err(|_| "N is not a number")?;
    if m == 0 || n == 0 {
        return Err("M and N must be >= 1".into());
    }
    if m > n {
        return Err("threshold M must not exceed total N".into());
    }
    Ok((m, n))
}

// =============================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_specs() {
        assert_eq!(parse_spec("2/3").unwrap(), (2, 3));
        assert_eq!(parse_spec(" 3 / 5 ").unwrap(), (3, 5));
        assert_eq!(parse_spec("1/1").unwrap(), (1, 1));
    }

    #[test]
    fn reject_invalid_specs() {
        assert!(parse_spec("23").is_err()); // no slash
        assert!(parse_spec("0/3").is_err()); // M = 0
        assert!(parse_spec("3/0").is_err()); // N = 0
        assert!(parse_spec("4/3").is_err()); // M > N
        assert!(parse_spec("a/3").is_err()); // non-numeric
    }

    #[test]
    fn custody_builds_a_usable_committee() {
        // Construct directly (not via env, to avoid global-env races in tests):
        // a 2-of-3 committee must expose a 32-byte group key it can sign under.
        let committee = TaprootCommittee::generate(2, 3).unwrap();
        let custody = Custody {
            threshold: 2,
            total: 3,
            committee: Arc::new(committee),
        };
        assert_eq!(custody.group_xonly().len(), 32);
        assert_eq!(custody.threshold, 2);
        assert_eq!(custody.total, 3);
    }
}
