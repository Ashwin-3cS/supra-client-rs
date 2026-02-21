//! Async HTTP client for the Supra MoveVM RPC API.

use crate::types::{
    AccountAddress, AccountInfo, Balance, CoinStore, FaucetResponse, ViewRequest, ViewResponse,
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

        let coin_store: CoinStore = resp
            .json()
            .await
            .context("Failed to parse CoinStore resource JSON")?;

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
    /// POST {faucet_url}/faucet/v1/fund
    pub async fn faucet(&self, addr: &AccountAddress) -> Result<FaucetResponse> {
        // Supra testnet faucet endpoint (may vary by version).
        let url = format!("{}/faucet/v1/fund", self.faucet_url);

        let body = serde_json::json!({
            "address": addr.normalise(),
            "coin_type_args": "0x1::supra_coin::SupraCoin"
        });

        let resp = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .with_context(|| format!("POST {}", url))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Faucet error {}: {}", status, body_text);
        }

        let faucet_resp: FaucetResponse = resp
            .json()
            .await
            .context("Failed to parse faucet response")?;

        Ok(faucet_resp)
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
