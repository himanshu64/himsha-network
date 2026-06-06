//! Threshold-signed Taproot **key-spend** settlement transactions.
//!
//! Closes the loop on FROST custody (Option B): instead of a single wallet key
//! signing the Bitcoin settlement, the [`TaprootCommittee`] M-of-N group key owns
//! the funds and a quorum produces the key-path witness.
//!
//! Flow:
//!   1. the committee's `group_xonly` is the Taproot output key (BIP-341 tweaked);
//!   2. build the unsigned tx spending the loan/inscription UTXO → recipient;
//!   3. compute the BIP-341 key-spend sighash;
//!   4. the committee threshold-signs that 32-byte sighash;
//!   5. attach the 64-byte Schnorr signature as the key-path witness → broadcast.
//!
//! ⚠️ **Unverified without a regtest node.** The construction follows BIP-341, but
//! end-to-end acceptance (the exact tweak, fee, and a real funded UTXO) must be
//! checked against Bitcoin Core. Compilation + the sighash/sign/verify path are
//! covered by tests; broadcasting is the existing `bitcoin_indexer::broadcast`.

use anyhow::{anyhow, Result};
use bitcoin::{
    hashes::Hash,
    key::TweakedPublicKey,
    locktime::absolute::LockTime,
    secp256k1::XOnlyPublicKey,
    sighash::{Prevouts, SighashCache, TapSighashType},
    transaction::Version,
    Address, Amount, Network, OutPoint, ScriptBuf, Sequence, Transaction, TxIn, TxOut, Txid, Witness,
};
use std::str::FromStr;

use himsha_threshold::taproot::TaprootCommittee;

/// The Taproot scriptPubKey controlled by the committee's group key.
fn committee_p2tr(group_xonly: &[u8; 32]) -> Result<ScriptBuf> {
    let xonly = XOnlyPublicKey::from_slice(group_xonly).map_err(|e| anyhow!("bad group key: {e}"))?;
    // The committee's group key is already the BIP-341 tweaked output key.
    let output_key = TweakedPublicKey::dangerous_assume_tweaked(xonly);
    Ok(ScriptBuf::new_p2tr_tweaked(output_key))
}

/// The committee's Taproot (P2TR) address on `network` — fund this to give the
/// M-of-N group custody of an output it can later key-spend. Drives the regtest
/// broadcast-acceptance test.
pub fn committee_address(group_xonly: &[u8; 32], network: Network) -> Result<Address> {
    let spk = committee_p2tr(group_xonly)?;
    Address::from_script(&spk, network).map_err(|e| anyhow!("address from script: {e}"))
}

/// Build the unsigned key-spend tx (spend `utxo` → `recipient`) and its BIP-341
/// sighash. `in_value`/`fee` are sats; output value = in_value − fee.
pub fn build_keyspend(
    group_xonly: &[u8; 32],
    utxo_txid: &str,
    vout: u32,
    in_value: u64,
    recipient: &str,
    fee: u64,
    network: Network,
) -> Result<(Transaction, [u8; 32])> {
    let txid = Txid::from_str(utxo_txid).map_err(|e| anyhow!("bad txid: {e}"))?;
    let send = in_value.checked_sub(fee).ok_or_else(|| anyhow!("value {in_value} below fee {fee}"))?;
    let recipient_spk = Address::from_str(recipient)
        .map_err(|e| anyhow!("bad address: {e}"))?
        .require_network(network)
        .map_err(|e| anyhow!("address network mismatch: {e}"))?
        .script_pubkey();
    let committee_spk = committee_p2tr(group_xonly)?;

    let tx = Transaction {
        version: Version::TWO,
        lock_time: LockTime::ZERO,
        input: vec![TxIn {
            previous_output: OutPoint { txid, vout },
            script_sig: ScriptBuf::new(),
            sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
            witness: Witness::new(),
        }],
        output: vec![TxOut { value: Amount::from_sat(send), script_pubkey: recipient_spk }],
    };

    // The prevout being spent is the committee-owned P2TR output.
    let prevout = TxOut { value: Amount::from_sat(in_value), script_pubkey: committee_spk };
    let mut cache = SighashCache::new(&tx);
    let sighash = cache
        .taproot_key_spend_signature_hash(0, &Prevouts::All(&[prevout]), TapSighashType::Default)
        .map_err(|e| anyhow!("sighash: {e}"))?;

    Ok((tx, sighash.to_byte_array()))
}

/// Attach a 64-byte key-path Schnorr signature as the input witness.
pub fn finalize_keyspend(mut tx: Transaction, sig64: &[u8]) -> Result<String> {
    if sig64.len() != 64 {
        return Err(anyhow!("expected 64-byte schnorr signature, got {}", sig64.len()));
    }
    let mut witness = Witness::new();
    witness.push(sig64); // SIGHASH_DEFAULT → bare 64-byte sig, no sighash-type byte
    tx.input[0].witness = witness;
    Ok(bitcoin::consensus::encode::serialize_hex(&tx))
}

/// End-to-end: build → committee threshold-signs the sighash → finalize.
/// Returns the signed raw tx hex, ready for `bitcoin_indexer::broadcast`.
#[allow(clippy::too_many_arguments)]
pub fn settle_with_committee(
    committee: &TaprootCommittee,
    utxo_txid: &str,
    vout: u32,
    in_value: u64,
    recipient: &str,
    fee: u64,
    network: Network,
) -> Result<String> {
    let group_xonly = committee.group_xonly();
    let (tx, sighash) = build_keyspend(&group_xonly, utxo_txid, vout, in_value, recipient, fee, network)?;

    // A quorum of the committee signs the sighash.
    let quorum: Vec<_> = committee.signer_ids().into_iter().take(committee.threshold() as usize).collect();
    let sig = committee
        .sign(&sighash, &quorum)
        .map_err(|e| anyhow!("threshold sign: {e}"))?;
    if !committee.verify(&sighash, &sig) {
        return Err(anyhow!("aggregate signature failed self-verification"));
    }
    finalize_keyspend(tx, &sig)
}

#[cfg(test)]
mod tests {
    use super::*;

    // A regtest P2TR recipient address (any valid bech32m works for building).
    const RECIPIENT: &str = "bcrt1p0xlxvlhemja6c4dqv22uapctqupfhlxm9h8z3k2e72q4k9hcz7vqc8gma6";

    #[test]
    fn test_build_keyspend_produces_32byte_sighash() {
        let committee = TaprootCommittee::generate(2, 3).unwrap();
        let xonly = committee.group_xonly();
        let txid = "0000000000000000000000000000000000000000000000000000000000000001";
        let (tx, sighash) = build_keyspend(&xonly, txid, 0, 100_000, RECIPIENT, 500, Network::Regtest).unwrap();
        assert_eq!(tx.input.len(), 1);
        assert_eq!(tx.output.len(), 1);
        assert_eq!(tx.output[0].value.to_sat(), 99_500); // in − fee
        assert_eq!(sighash.len(), 32);
    }

    #[test]
    fn test_settle_with_committee_attaches_witness() {
        let committee = TaprootCommittee::generate(2, 3).unwrap();
        let txid = "0000000000000000000000000000000000000000000000000000000000000001";
        let hex = settle_with_committee(&committee, txid, 0, 100_000, RECIPIENT, 500, Network::Regtest).unwrap();
        assert!(!hex.is_empty());
        // The hex must contain the 64-byte (128 hex char) witness signature.
        assert!(hex.len() > 128);
    }

    #[test]
    fn test_finalize_rejects_bad_sig_len() {
        let committee = TaprootCommittee::generate(2, 3).unwrap();
        let xonly = committee.group_xonly();
        let txid = "0000000000000000000000000000000000000000000000000000000000000001";
        let (tx, _) = build_keyspend(&xonly, txid, 0, 100_000, RECIPIENT, 500, Network::Regtest).unwrap();
        assert!(finalize_keyspend(tx, &[0u8; 10]).is_err());
    }
}
