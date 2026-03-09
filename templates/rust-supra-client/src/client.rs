//! Async HTTP client for the Supra MoveVM RPC API.

use crate::types::{
    AccountAddress, AccountInfo, Balance, FaucetResponse, ViewRequest, ViewResponse,
    SignedTransaction, TxResult
};
use anyhow::{Context, Result};
use reqwest::Client;

/// Default RPC endpoint for Supra testnet.
pub const DEFAULT_RPC_URL: &str = "https://rpc-testnet.supra.com";
/// Default faucet endpoint.
pub const DEFAULT_FAUCET_URL: &str = "https://faucet-testnet.supra.com";
/// Supra testnet chain ID.
pub const CHAIN_ID: u8 = 6;

/// The fully-typed resource path for the native SUPRA coin.
const SUPRA_COIN_RESOURCE: &str =
    "0x1::coin::CoinStore<0x1::supra_coin::SupraCoin>";

// ─────────────────────────────────────────────────────────────────────────────

/// Async client for Supra's JSON-RPC API.
///
/// # Example
/// ```no_run
/// # tokio_test::block_on(async {
/// use supra_rust_client::SupraClient;
/// let client = SupraClient::new(None, None);
/// let balance = client.get_balance("0x1".parse().unwrap()).await.unwrap();
/// println!("{}", balance);
/// # });
/// ```
#[derive(Clone, Debug)]
pub struct SupraClient {
    /// Underlying reqwest HTTP client.
    http: Client,
    /// Base RPC URL (no trailing slash).
    pub rpc_url: String,
    /// Faucet base URL.
    pub faucet_url: String,
    /// Chain ID used when building transactions.
    pub chain_id: u8,
}

impl SupraClient {
    /// Create a new client. Defaults to the public Supra testnet.
    ///
    /// `rpc_url`    — override with `SUPRA_RPC_URL` env var or pass `Some(url)`  
    /// `faucet_url` — override or `None` to use default  
    pub fn new(rpc_url: Option<String>, faucet_url: Option<String>) -> Self {
        let rpc_url = rpc_url
            .or_else(|| std::env::var("SUPRA_RPC_URL").ok())
            .unwrap_or_else(|| DEFAULT_RPC_URL.into());

        let faucet_url = faucet_url
            .or_else(|| std::env::var("SUPRA_FAUCET_URL").ok())
            .unwrap_or_else(|| DEFAULT_FAUCET_URL.into());

        let http = Client::builder()
            .user_agent("supra-rust-client/0.1.0")
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to build HTTP client");

        Self {
            http,
            rpc_url: rpc_url.trim_end_matches('/').to_string(),
            faucet_url: faucet_url.trim_end_matches('/').to_string(),
            chain_id: CHAIN_ID,
        }
    }

    // ─── Account ─────────────────────────────────────────────────────────────

    /// Fetch account metadata (sequence number, auth key).
    ///
    /// GET /rpc/v3/accounts/{address}
    pub async fn get_account(&self, addr: &AccountAddress) -> Result<AccountInfo> {
        let url = format!("{}/rpc/v3/accounts/{}", self.rpc_url, addr.normalise());
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {}", url))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("RPC error {}: {}", status, body);
        }

        resp.json::<AccountInfo>()
            .await
            .context("Failed to parse AccountInfo JSON")
    }

    // ─── Balance ─────────────────────────────────────────────────────────────

    /// Fetch native SUPRA coin balance for an address.
    ///
    /// GET /rpc/v3/accounts/{address}/resources/0x1::coin::CoinStore<0x1::supra_coin::SupraCoin>
    pub async fn get_balance(&self, addr: AccountAddress) -> Result<Balance> {
        let encoded_resource = urlencoding::encode(SUPRA_COIN_RESOURCE);
        let url = format!(
            "{}/rpc/v3/accounts/{}/resources/{}",
            self.rpc_url,
            addr.normalise(),
            encoded_resource,
        );

        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {}", url))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("RPC error {} fetching balance: {}", status, body);
        }

        let raw: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse resource JSON")?;

        // v3 API returns the resource object directly:
        // { "type": "...", "data": { "coin": { "value": "500000000" }, ... } }
        // Unfunded accounts return 404 (caught above) or missing data.
        let balance_str = raw
            .pointer("/data/coin/value")
            .and_then(|v| v.as_str())
            .unwrap_or("0");

        let balance_raw: u64 = balance_str.parse().unwrap_or(0);
        Ok(Balance { address: addr, raw: balance_raw })
    }

    // ─── View ─────────────────────────────────────────────────────────────────

    /// Call a Move view function (read-only, no gas).
    ///
    /// POST /rpc/v3/view
    pub async fn view(&self, req: ViewRequest) -> Result<ViewResponse> {
        let url = format!("{}/rpc/v3/view", self.rpc_url);

        let resp = self
            .http
            .post(&url)
            .json(&req)
            .send()
            .await
            .with_context(|| format!("POST {}", url))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("View RPC error {}: {}", status, body);
        }

        let raw: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse view response")?;

        // The API wraps results differently on some versions — normalise.
        if let Some(arr) = raw.as_array() {
            return Ok(arr.clone());
        }
        if let Some(result) = raw.get("result") {
            if let Some(arr) = result.as_array() {
                return Ok(arr.clone());
            }
        }
        // Return the whole value as a single-element array.
        Ok(vec![raw])
    }

    // ─── Faucet ─────────────────────────────────────────────────────────────

    /// Request testnet SUPRA from the faucet.
    ///
    /// GET {rpc_url}/rpc/v3/wallet/faucet/{address}
    pub async fn faucet(&self, addr: &AccountAddress) -> Result<FaucetResponse> {
        let url = format!("{}/rpc/v3/wallet/faucet/{}", self.rpc_url, addr.normalise());

        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {}", url))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Faucet error {}: {}", status, body_text);
        }

        // Faucet may return an empty body or simple JSON, try to parse but fallback.
        let body_bytes = resp.bytes().await?;
        if body_bytes.is_empty() {
             return Ok(FaucetResponse { status: Some("OK".into()), extra: serde_json::json!({}) });
        }
        
        let faucet_resp: FaucetResponse = serde_json::from_slice(&body_bytes)
            .context("Failed to parse faucet response")?;

        Ok(faucet_resp)
    }

    // ─── Transactions ────────────────────────────────────────────────────────

    /// Submit a signed transaction to the network.
    ///
    /// POST /rpc/v3/transactions/submit
    pub async fn submit_transaction(&self, tx: &SignedTransaction) -> Result<TxResult> {
        let url = format!("{}/rpc/v3/transactions/submit", self.rpc_url);

        // The endpoint requires wrapping the transaction in a "Move" variant enum
        let payload = serde_json::json!({
            "Move": tx
        });


        let resp = self
            .http
            .post(&url)
            .header("Content-Type", "application/json") // Note: Some nodes require application/x.aptos.signed_transaction+bcs which we may support later
            .json(&payload)
            .send()
            .await
            .with_context(|| format!("POST {}", url))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Submit transaction RPC error {}: {}", status, body);
        }

        // The submit endpoint may return either a raw hash string or a TxResult object.
        let body = resp.text().await.context("Failed to read submit response body")?;
        
        // Try parsing as TxResult first, fall back to interpreting as a raw hash string
        if let Ok(tx_result) = serde_json::from_str::<TxResult>(&body) {
            Ok(tx_result)
        } else {
            // Server returned a raw string like "0xabc..."
            let hash = body.trim().trim_matches('"').to_string();
            Ok(TxResult {
                hash: Some(hash),
                success: true, // Accepted by mempool
                vm_status: None,
            })
        }
    }

    /// Simulate a transaction to estimate gas and verify success without executing it.
    ///
    /// POST /rpc/v3/transactions/simulate
    pub async fn dry_run_transaction(&self, tx: &SignedTransaction) -> Result<serde_json::Value> {
        let url = format!("{}/rpc/v3/transactions/simulate", self.rpc_url);

        // The endpoint requires wrapping the transaction in a "Move" variant enum
        // but with a zeroed-out signature (to signal simulation mode).
        let mut payload = serde_json::json!({
            "Move": tx
        });

        // Zero out the signature for simulation (TS SDK does this via unsetAuthenticatorSignatures)
        let zero_sig = format!("0x{}", "0".repeat(128)); // 64 zero bytes as hex
        if let Some(auth) = payload.pointer_mut("/Move/authenticator/Ed25519/signature") {
            *auth = serde_json::Value::String(zero_sig);
        }

        let resp = self
            .http
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .with_context(|| format!("POST {}", url))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Simulate transaction RPC error {}: {}", status, body);
        }

        let raw: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse simulation result JSON")?;

        // The simulation API might return an array directly, or a `{ "result": [...] }` wrapper.
        if let Some(arr) = raw.as_array() {
            return Ok(serde_json::Value::Array(arr.clone()));
        }
        if let Some(result) = raw.get("result") {
            if let Some(arr) = result.as_array() {
                return Ok(serde_json::Value::Array(arr.clone()));
            }
        }
        
        // If it's something else, just return the raw value.
        Ok(raw)
    }

    /// Wait for a transaction to hit finality by polling its hash.
    /// Polling runs for up to ~30 seconds.
    pub async fn wait_for_transaction(&self, tx_hash: &str) -> Result<serde_json::Value> {
        let url = format!("{}/rpc/v3/transactions/{}", self.rpc_url, tx_hash);
        
        let max_retries = 60;
        let mut attempts = 0;

        while attempts < max_retries {
            let resp = self.http.get(&url).send().await;
            
            if let Ok(r) = resp {
                if r.status().is_success() {
                    let json_val: serde_json::Value = r.json().await?;
                    // Check if transaction is still pending/unexecuted
                    let status = json_val.get("status")
                        .and_then(|s| s.as_str())
                        .unwrap_or("");
                    if status != "Pending" && status != "Unexecuted" {
                        return Ok(json_val);
                    }
                }
            }
            
            attempts += 1;
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }

        anyhow::bail!("Transaction {} not finalized after {} retries", tx_hash, max_retries);
    }

    // ─── Ledger Info ─────────────────────────────────────────────────────────

    /// Fetch ledger/chain info (useful for checking connectivity).
    ///
    /// GET /rpc/v3/
    pub async fn get_ledger_info(&self) -> Result<serde_json::Value> {
        let url = format!("{}/rpc/v3/", self.rpc_url);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {}", url))?;
        resp.json().await.context("Failed to parse ledger info")
    }

    // ─── Gas Price ───────────────────────────────────────────────────────────

    /// Fetch the current minimum gas unit price from the node.
    ///
    /// Mirrors the TS SDK's `getMinGasUnitPrice()` which calls
    /// GET /rpc/v3/transactions/estimate_gas_price
    /// and returns `min_configured_gas_price`.
    pub async fn get_gas_price(&self) -> Result<u64> {
        let url = format!("{}/rpc/v3/transactions/estimate_gas_price", self.rpc_url);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {}", url))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Failed to fetch gas price {}: {}", status, body);
        }

        let json: serde_json::Value = resp.json().await.context("Failed to parse gas price response")?;

        // The response contains both `median_gas_price` and `min_configured_gas_price`.
        // We use `min_configured_gas_price` like the TS SDK does.
        json.get("min_configured_gas_price")
            .and_then(|v| v.as_u64())
            .or_else(|| json.get("median_gas_price").and_then(|v| v.as_u64()))
            .context("Could not parse gas price from response")
    }
}
