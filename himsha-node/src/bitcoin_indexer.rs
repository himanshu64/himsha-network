use anyhow::{anyhow, Result};
use bitcoin::{Address, Amount, Txid};
use bitcoincore_rpc::{json, Auth, Client, RpcApi};
use himsha_runtime::utxo::{UtxoInfo, UtxoMeta};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::str::FromStr;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{info, warn};

/// Real-time mempool event emitted to subscribers.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum MempoolEvent {
    Seen      { txid: String },
    Confirmed { txid: String, block_height: u64 },
    Evicted   { txid: String },
}

fn txid_to_bytes(txid: &Txid) -> [u8; 32] {
    let mut b = [0u8; 32];
    b.copy_from_slice(txid.as_ref());
    b
}

/// Bitcoin node indexer — UTXO queries, mempool events, inscription lookups.
pub struct BitcoinIndexer {
    rpc:               Client,
    network:           bitcoin::Network,
    mempool_tx:        broadcast::Sender<MempoolEvent>,
    mempool_space_url: Option<String>,
}

impl BitcoinIndexer {
    pub fn new(
        rpc_url: &str,
        rpc_user: &str,
        rpc_pass: &str,
        network: bitcoin::Network,
        mempool_space_url: Option<String>,
    ) -> Result<Self> {
        let rpc = Client::new(rpc_url, Auth::UserPass(rpc_user.into(), rpc_pass.into()))?;
        let (tx, _) = broadcast::channel(1024);
        Ok(Self { rpc, network, mempool_tx: tx, mempool_space_url })
    }

    /// Build an indexer from environment variables, or `None` if Bitcoin RPC
    /// isn't configured. Reads `BITCOIN_RPC_URL`, `BITCOIN_RPC_USER`,
    /// `BITCOIN_RPC_PASS`, optional `BITCOIN_NETWORK` (default regtest) and
    /// optional `MEMPOOL_SPACE_URL`.
    pub fn from_env() -> Option<Self> {
        let url  = std::env::var("BITCOIN_RPC_URL").ok()?;
        let user = std::env::var("BITCOIN_RPC_USER").ok()?;
        let pass = std::env::var("BITCOIN_RPC_PASS").ok()?;
        let network = match std::env::var("BITCOIN_NETWORK").as_deref() {
            Ok("mainnet") | Ok("bitcoin") => bitcoin::Network::Bitcoin,
            Ok("testnet")                 => bitcoin::Network::Testnet,
            Ok("signet")                  => bitcoin::Network::Signet,
            _                              => bitcoin::Network::Regtest,
        };
        let mempool = std::env::var("MEMPOOL_SPACE_URL").ok();
        Self::new(&url, &user, &pass, network, mempool).ok()
    }

    pub fn event_stream(&self) -> broadcast::Receiver<MempoolEvent> {
        self.mempool_tx.subscribe()
    }

    pub fn get_utxo(&self, txid: &str, vout: u32) -> Result<Option<UtxoInfo>> {
        let txid_parsed = Txid::from_str(txid)?;
        let tx_out = self.rpc.get_tx_out(&txid_parsed, vout, Some(true))?;
        Ok(tx_out.map(|o| UtxoInfo {
            meta: UtxoMeta {
                txid: txid_to_bytes(&txid_parsed),
                vout,
            },
            value:         o.value.to_sat(),
            script_pubkey: hex::encode(&o.script_pub_key.hex),
            confirmations: o.confirmations,
        }))
    }

    pub fn list_address_utxos(&self, address: &str) -> Result<Vec<UtxoInfo>> {
        let addr    = Address::from_str(address)?.require_network(self.network)?;
        let unspent = self.rpc.list_unspent(Some(0), None, Some(&[&addr]), None, None)?;
        Ok(unspent.into_iter().map(|u| UtxoInfo {
            meta: UtxoMeta {
                txid: txid_to_bytes(&u.txid),
                vout: u.vout,
            },
            value:         u.amount.to_sat(),
            script_pubkey: u.script_pub_key.to_hex_string(),
            confirmations: u.confirmations,
        }).collect())
    }

    pub fn broadcast(&self, raw_tx_hex: &str) -> Result<String> {
        let raw  = hex::decode(raw_tx_hex)?;
        let txid = self.rpc.send_raw_transaction(raw.as_slice())?;
        info!("broadcast bitcoin tx {txid}");
        Ok(txid.to_string())
    }

    /// Current best block height.
    pub fn block_height(&self) -> Result<u64> {
        Ok(self.rpc.get_block_count()?)
    }

    /// Spendable balance of the loaded wallet, in sats.
    pub fn wallet_balance_sats(&self) -> Result<u64> {
        Ok(self.rpc.get_balance(None, None)?.to_sat())
    }

    /// Send `sats` to `recipient` from the node's Bitcoin wallet. Returns the txid.
    /// Used to forward loan repayments to the lender.
    pub fn send_payment(&self, recipient: &str, sats: u64) -> Result<String> {
        let addr = Address::from_str(recipient)?.require_network(self.network)?;
        let txid = self.rpc.send_to_address(
            &addr,
            Amount::from_sat(sats),
            None, None, None, None, None, None,
        )?;
        info!("sent {sats} sats to {recipient} in tx {txid}");
        Ok(txid.to_string())
    }

    /// Move one specific UTXO (e.g. an inscription) to `recipient`, spending the
    /// whole output minus a flat fee. Signs with the node wallet and broadcasts.
    /// Used to return/seize the inscription on repay/default.
    pub fn transfer_utxo(&self, txid: &str, vout: u32, recipient: &str) -> Result<String> {
        const FEE_SATS: u64 = 500;
        let txid_parsed = Txid::from_str(txid)?;
        let addr = Address::from_str(recipient)?.require_network(self.network)?;

        let out = self
            .rpc
            .get_tx_out(&txid_parsed, vout, Some(true))?
            .ok_or_else(|| anyhow!("utxo {txid}:{vout} not found or already spent"))?;
        let value = out.value.to_sat();
        let send = value
            .checked_sub(FEE_SATS)
            .ok_or_else(|| anyhow!("utxo value {value} below fee {FEE_SATS}"))?;

        let inputs = vec![json::CreateRawTransactionInput {
            txid: txid_parsed,
            vout,
            sequence: None,
        }];
        let mut outputs = std::collections::HashMap::new();
        outputs.insert(addr.to_string(), Amount::from_sat(send));

        let raw    = self.rpc.create_raw_transaction(&inputs, &outputs, None, None)?;
        let signed = self.rpc.sign_raw_transaction_with_wallet(&raw, None, None)?;
        if !signed.complete {
            return Err(anyhow!("wallet could not fully sign the utxo transfer"));
        }
        let txid = self.rpc.send_raw_transaction(&signed.hex)?;
        info!("transferred utxo {txid_parsed}:{vout} -> {recipient} in tx {txid}");
        Ok(txid.to_string())
    }

    pub async fn get_inscription(&self, inscription_id: &str) -> Option<InscriptionInfo> {
        let base = self.mempool_space_url.as_deref()?;
        let url  = format!("{base}/api/v1/inscription/{inscription_id}");
        reqwest::get(&url).await.ok()?.json::<InscriptionInfo>().await.ok()
    }

    // ---- auto-sync ----

    /// Blocking sync loop: every `interval`, poll the chain tip and mempool and
    /// emit [`MempoolEvent`]s to subscribers. Runs forever; intended for a
    /// dedicated thread (the underlying RPC client is blocking).
    pub fn run_sync_blocking(self, interval: Duration) {
        let mut last_height = 0u64;
        let mut seen: HashSet<String> = HashSet::new();
        info!("bitcoin indexer auto-sync started (poll every {interval:?})");
        loop {
            if let Err(e) = self.poll_once(&mut last_height, &mut seen) {
                warn!("indexer sync poll failed: {e}");
            }
            std::thread::sleep(interval);
        }
    }

    /// One sync tick: diff the mempool against the previous snapshot and the chain
    /// tip against the last height, emitting Seen / Confirmed / Evicted events.
    fn poll_once(&self, last_height: &mut u64, seen: &mut HashSet<String>) -> Result<()> {
        let height = self.rpc.get_block_count()?;
        let advanced = height > *last_height;
        if advanced {
            info!("indexer: chain tip -> height {height}");
            *last_height = height;
        }

        let current: HashSet<String> = self
            .rpc
            .get_raw_mempool()?
            .into_iter()
            .map(|t| t.to_string())
            .collect();

        // Newly observed mempool transactions.
        for txid in current.difference(seen) {
            let _ = self.mempool_tx.send(MempoolEvent::Seen { txid: txid.clone() });
        }
        // Transactions that left the mempool: confirmed if a block arrived this
        // tick, otherwise treated as evicted (RBF/expiry).
        for txid in seen.difference(&current) {
            let ev = if advanced {
                MempoolEvent::Confirmed { txid: txid.clone(), block_height: height }
            } else {
                MempoolEvent::Evicted { txid: txid.clone() }
            };
            let _ = self.mempool_tx.send(ev);
        }

        *seen = current;
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InscriptionInfo {
    pub id:           String,
    pub number:       u64,
    pub content_type: Option<String>,
    pub address:      Option<String>,
    pub sat:          Option<u64>,
    pub output:       Option<String>,
    pub offset:       Option<u64>,
}
