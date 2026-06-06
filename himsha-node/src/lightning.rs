//! Lightning Network integration (LND REST).
//!
//! HIMSHA isn't a Lightning node — it integrates an existing one (LND here; CLN/LDK
//! would mirror this) over its REST API to use Lightning as a **fast off-chain
//! settlement rail**: create/pay BOLT-11 invoices for sat-denominated payouts
//! (e.g. loan repayments, yield distributions) instead of on-chain UTXO moves.
//!
//! Configured via env: `LND_REST_URL` (e.g. `https://127.0.0.1:8080`) and
//! `LND_MACAROON_HEX` (admin/invoice macaroon, hex). TLS is accepted as-is so a
//! local self-signed LND works.
//!
//! ⚠️ **Unverified without a running LND node + open channels.** Compilation and the
//! request-shaping are covered; live invoice create/pay needs a funded Lightning node.

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

/// Heuristic: BOLT-11 invoices are bech32 and start with an `ln` HRP
/// (`lnbc`, `lntb`, `lnbcrt`, `lnsb`…). Used to route a settlement recipient to
/// Lightning vs. an on-chain address.
pub fn is_invoice(recipient: &str) -> bool {
    let r = recipient.trim().to_ascii_lowercase();
    r.starts_with("lnbc") || r.starts_with("lntb") || r.starts_with("lnbcrt") || r.starts_with("lnsb")
}

/// Thin LND REST client.
pub struct LightningClient {
    base: String,
    macaroon_hex: String,
    http: reqwest::Client,
}

impl LightningClient {
    /// Build from `LND_REST_URL` + `LND_MACAROON_HEX`, or `None` if unconfigured.
    pub fn from_env() -> Option<Self> {
        let base = std::env::var("LND_REST_URL").ok()?;
        let macaroon_hex = std::env::var("LND_MACAROON_HEX").ok()?;
        let http = reqwest::Client::builder()
            .danger_accept_invalid_certs(true) // local LND self-signed TLS
            .build()
            .ok()?;
        Some(Self { base, macaroon_hex, http })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base.trim_end_matches('/'), path)
    }

    async fn post(&self, path: &str, body: Value) -> Result<Value> {
        let v = self
            .http
            .post(self.url(path))
            .header("Grpc-Metadata-macaroon", &self.macaroon_hex)
            .json(&body)
            .send()
            .await?
            .json::<Value>()
            .await?;
        Ok(v)
    }

    async fn get(&self, path: &str) -> Result<Value> {
        let v = self
            .http
            .get(self.url(path))
            .header("Grpc-Metadata-macaroon", &self.macaroon_hex)
            .send()
            .await?
            .json::<Value>()
            .await?;
        Ok(v)
    }

    /// Create a BOLT-11 invoice for `amount_sat`; returns the payment request string.
    pub async fn create_invoice(&self, amount_sat: u64, memo: &str) -> Result<String> {
        let v = self.post("/v1/invoices", json!({ "value": amount_sat.to_string(), "memo": memo })).await?;
        v.get("payment_request")
            .and_then(|p| p.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow!("LND: no payment_request in response ({v})"))
    }

    /// Pay a BOLT-11 invoice; returns the payment hash (base64) on success.
    pub async fn pay_invoice(&self, bolt11: &str) -> Result<String> {
        let v = self.post("/v1/channels/transactions", json!({ "payment_request": bolt11 })).await?;
        if let Some(err) = v.get("payment_error").and_then(|e| e.as_str()) {
            if !err.is_empty() {
                return Err(anyhow!("LND pay error: {err}"));
            }
        }
        Ok(v.get("payment_hash").and_then(|h| h.as_str()).unwrap_or_default().to_string())
    }

    /// Total spendable channel balance, in sats.
    pub async fn channel_balance_sat(&self) -> Result<u64> {
        let v = self.get("/v1/balance/channels").await?;
        Ok(v.get("balance").and_then(|b| b.as_str()).and_then(|s| s.parse().ok()).unwrap_or(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_invoice() {
        assert!(is_invoice("lnbc100n1p..."));
        assert!(is_invoice("lnbcrt500u1p..."));   // regtest
        assert!(is_invoice("LNTB10u1p..."));      // case-insensitive
        assert!(!is_invoice("bcrt1p0xlxv..."));   // a Bitcoin address, not an invoice
        assert!(!is_invoice("3J98t1...legacy"));
    }
}
