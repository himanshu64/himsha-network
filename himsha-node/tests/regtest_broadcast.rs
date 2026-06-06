//! End-to-end **on-chain broadcast proof** for FROST→Taproot key-spend settlement.
//!
//! This is the test that closes the "broadcast-unverified" gap: it funds the M-of-N
//! committee's Taproot address on a real regtest node, has the committee threshold-sign
//! the key-spend, **broadcasts it, and asserts Bitcoin Core accepts and confirms it** —
//! proving the tweak, sighash, witness, and fee are all correct against the real
//! consensus rules, not just in code.
//!
//! It is `#[ignore]`d so the normal `cargo test` run stays offline. To run it:
//!
//! ```bash
//! # 1. a regtest bitcoind with a loaded wallet (see docs/testing-locally.md §1):
//! bitcoind -regtest -daemon -rpcuser=himsha -rpcpassword=himsha -rpcport=18443 -fallbackfee=0.0001 -txindex=1
//! bitcoin-cli -regtest -rpcuser=himsha -rpcpassword=himsha -rpcport=18443 createwallet test
//!
//! # 2. point the test at it and run the ignored test:
//! HIMSHA_REGTEST=1 \
//! BITCOIN_RPC_URL=http://127.0.0.1:18443 \
//! BITCOIN_RPC_USER=himsha BITCOIN_RPC_PASS=himsha \
//!   cargo test -p himsha-node --test regtest_broadcast -- --ignored --nocapture
//! ```

use bitcoin::{Address, Amount, Network};
use bitcoincore_rpc::{Auth, Client, RpcApi};
use himsha_node::settlement_tx::{committee_address, settle_with_committee};
use himsha_threshold::taproot::TaprootCommittee;

/// Read regtest RPC config from the environment, or `None` to skip the test.
fn regtest_client() -> Option<Client> {
    if std::env::var("HIMSHA_REGTEST").as_deref() != Ok("1") {
        eprintln!("skipping: set HIMSHA_REGTEST=1 (+ BITCOIN_RPC_URL/USER/PASS) to run");
        return None;
    }
    let url = std::env::var("BITCOIN_RPC_URL").ok()?;
    let user = std::env::var("BITCOIN_RPC_USER").ok()?;
    let pass = std::env::var("BITCOIN_RPC_PASS").ok()?;
    Client::new(&url, Auth::UserPass(user, pass)).ok()
}

/// Mine `n` blocks to a fresh wallet address (matures coinbase / confirms txs).
fn mine(rpc: &Client, n: u64) -> Address {
    let addr = rpc
        .get_new_address(None, None)
        .expect("get_new_address")
        .require_network(Network::Regtest)
        .expect("regtest addr");
    rpc.generate_to_address(n, &addr)
        .expect("generate_to_address");
    addr
}

#[test]
#[ignore = "requires a running regtest bitcoind; see module docs"]
fn committee_keyspend_is_accepted_onchain() {
    let Some(rpc) = regtest_client() else { return };

    // Ensure the wallet has spendable coins (coinbase matures after 100 blocks).
    if rpc.get_balance(None, None).unwrap_or(Amount::ZERO) < Amount::from_btc(1.0).unwrap() {
        mine(&rpc, 101);
    }

    // 1. A 2-of-3 committee owns the settlement key; its group key is a P2TR output.
    let committee = TaprootCommittee::generate(2, 3).expect("committee");
    let group_xonly = committee.group_xonly();
    let committee_addr = committee_address(&group_xonly, Network::Regtest).expect("committee addr");
    eprintln!("committee P2TR address: {committee_addr}");

    // 2. Fund the committee address, then confirm it.
    let funding_amt = Amount::from_sat(200_000);
    let funding_txid = rpc
        .send_to_address(
            &committee_addr,
            funding_amt,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("fund committee");
    mine(&rpc, 1);

    // 3. Find which vout paid the committee.
    let funding_tx = rpc
        .get_raw_transaction(&funding_txid, None)
        .expect("get funding tx");
    let committee_spk = committee_addr.script_pubkey();
    let (vout, in_value) = funding_tx
        .output
        .iter()
        .enumerate()
        .find(|(_, o)| o.script_pubkey == committee_spk)
        .map(|(i, o)| (i as u32, o.value.to_sat()))
        .expect("committee output present");

    // 4. Threshold-sign a key-spend of that UTXO back to the wallet, minus a fee.
    let recipient = mine(&rpc, 0).to_string(); // a fresh wallet address (no mining)
    let fee = 1_000u64;
    let signed_hex = settle_with_committee(
        &committee,
        &funding_txid.to_string(),
        vout,
        in_value,
        &recipient,
        fee,
        Network::Regtest,
    )
    .expect("settle_with_committee");

    // 5. THE PROOF: Bitcoin Core must accept the threshold-signed key-spend.
    let raw = hex::decode(&signed_hex).expect("hex");
    let settle_txid = rpc
        .send_raw_transaction(raw.as_slice())
        .expect("regtest REJECTED the committee key-spend — tweak/sighash/witness/fee is wrong");
    eprintln!("✅ committee key-spend accepted: {settle_txid}");

    // 6. Confirm it and assert it really landed on-chain.
    mine(&rpc, 1);
    let confirmed = rpc
        .get_raw_transaction_info(&settle_txid, None)
        .expect("tx info");
    assert!(
        confirmed.confirmations.unwrap_or(0) >= 1,
        "settlement tx did not confirm"
    );
    eprintln!("✅ confirmed in a block — FROST→Taproot settlement verified end-to-end");
}
