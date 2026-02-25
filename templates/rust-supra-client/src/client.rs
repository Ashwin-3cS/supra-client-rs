//! Async HTTP client for the Supra MoveVM RPC API.

use crate::types::{
    AccountAddress, AccountInfo, Balance, CoinStore, FaucetResponse, ViewRequest, ViewResponse,
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
    /// GET /rpc/v1/accounts/{address}
    pub async fn get_account(&self, addr: &AccountAddress) -> Result<AccountInfo> {
        let url = format!("{}/rpc/v1/accounts/{}", self.rpc_url, addr.normalise());
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
    /// GET /rpc/v1/accounts/{address}/resources/0x1::coin::CoinStore<0x1::supra_coin::SupraCoin>
    pub async fn get_balance(&self, addr: AccountAddress) -> Result<Balance> {
        let encoded_resource = urlencoding::encode(SUPRA_COIN_RESOURCE);
        let url = format!(
            "{}/rpc/v1/accounts/{}/resources/{}",
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

        // The API returns { "result": [ { "coin": ... } ] }
        let result_array = raw
            .get("result")
            .and_then(|r| r.as_array())
            .context("Missing 'result' array in response")?;

        if result_array.is_empty() || result_array.first().unwrap().is_null() {
            // Unfunded accounts don't have the CoinStore resource yet.
            return Ok(Balance { address: addr, raw: 0 });
        }

        let coin_store_json = result_array.first().unwrap();

        let coin_store: CoinStore = serde_json::from_value(coin_store_json.clone())
            .context("Failed to parse inner CoinStore object")?;

        let raw: u64 = coin_store
            .coin
            .value
            .parse()
            .context("Balance value is not a valid u64")?;

        Ok(Balance { address: addr, raw })
    }

    // ─── View ─────────────────────────────────────────────────────────────────

    /// Call a Move view function (read-only, no gas).
    ///
    /// POST /rpc/v1/view
    pub async fn view(&self, req: ViewRequest) -> Result<ViewResponse> {
        let url = format!("{}/rpc/v1/view", self.rpc_url);

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
    /// GET {rpc_url}/rpc/v1/wallet/faucet/{address}
    pub async fn faucet(&self, addr: &AccountAddress) -> Result<FaucetResponse> {
        let url = format!("{}/rpc/v1/wallet/faucet/{}", self.rpc_url, addr.normalise());

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
    /// POST /rpc/v1/transactions/submit
    pub async fn submit_transaction(&self, tx: &SignedTransaction) -> Result<TxResult> {
        let url = format!("{}/rpc/v1/transactions/submit", self.rpc_url);

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

        resp.json()
            .await
            .context("Failed to parse TxResult JSON")
    }

    /// Simulate a transaction to estimate gas and verify success without executing it.
    ///
    /// POST /rpc/v1/transactions/simulate
    pub async fn dry_run_transaction(&self, tx: &SignedTransaction) -> Result<serde_json::Value> {
        let url = format!("{}/rpc/v1/transactions/simulate", self.rpc_url);

        // The endpoint requires wrapping the transaction in a "Move" variant enum
        let payload = serde_json::json!({
            "Move": tx
        });

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

        resp.json()
            .await
            .context("Failed to parse simulation result JSON")
    }

    /// Wait for a transaction to hit finality by polling its hash.
    /// Polling runs for up to ~15 seconds default.
    pub async fn wait_for_transaction(&self, tx_hash: &str) -> Result<serde_json::Value> {
        let url = format!("{}/rpc/v1/transactions/by_hash/{}", self.rpc_url, tx_hash);
        
        let max_retries = 30;
        let mut attempts = 0;

        while attempts < max_retries {
            let resp = self.http.get(&url).send().await;
            
            if let Ok(r) = resp {
                if r.status().is_success() {
                    let json_val: serde_json::Value = r.json().await?;
                    // Return the payload immediately if the node has processed it
                    return Ok(json_val);
                }
            }
            
            attempts += 1;
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }

        anyhow::bail!("Transaction {} not found after {} retries", tx_hash, max_retries);
    }

    // ─── Ledger Info ─────────────────────────────────────────────────────────

    /// Fetch ledger/chain info (useful for checking connectivity).
    ///
    /// GET /rpc/v1/
    pub async fn get_ledger_info(&self) -> Result<serde_json::Value> {
        let url = format!("{}/rpc/v1/", self.rpc_url);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {}", url))?;
        resp.json().await.context("Failed to parse ledger info")
    }
}
