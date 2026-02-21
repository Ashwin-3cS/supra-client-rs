//! Core types for the Supra RPC client.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

// ─── Address ─────────────────────────────────────────────────────────────────

/// A 32-byte MoveVM account address (hex-encoded in JSON, e.g. "0x000...abc").
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AccountAddress(pub String);

impl AccountAddress {
    /// Normalise to lowercase hex with 0x prefix.
    pub fn normalise(&self) -> String {
        let s = self.0.trim_start_matches("0x");
        format!("0x{:0>64}", s.to_lowercase())
    }
}

impl fmt::Display for AccountAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.normalise())
    }
}

impl FromStr for AccountAddress {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let clean = s.trim_start_matches("0x");
        if clean.is_empty() || clean.len() > 64 {
            anyhow::bail!("Invalid address length: {}", s);
        }
        // validate hex chars
        if clean.chars().any(|c| !c.is_ascii_hexdigit()) {
            anyhow::bail!("Address contains non-hex characters: {}", s);
        }
        Ok(Self(format!("0x{}", clean.to_lowercase())))
    }
}

// ─── Account Info ─────────────────────────────────────────────────────────────

/// Raw account object returned by GET /rpc/v1/accounts/{address}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    pub sequence_number: String,
    pub authentication_key: String,
}

// ─── Balance ──────────────────────────────────────────────────────────────────

/// Coin data embedded inside the CoinStore resource.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoinData {
    pub value: String,
}

/// CoinStore resource returned under /rpc/v1/accounts/{addr}/resources/...
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoinStore {
    pub coin: CoinData,
    pub deposit_events: serde_json::Value,
    pub withdraw_events: serde_json::Value,
    pub frozen: bool,
}

/// Convenience balance response.
#[derive(Debug, Clone)]
pub struct Balance {
    pub address: AccountAddress,
    /// Raw token units (1 SUPRA = 10^8 units = 100_000_000)
    pub raw: u64,
}

impl Balance {
    /// Human-readable SUPRA amount.
    pub fn supra(&self) -> f64 {
        self.raw as f64 / 1_000_000_000.0
    }
}

impl fmt::Display for Balance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Address : {}\nBalance : {} SUPRA  ({} raw units)",
            self.address,
            self.supra(),
            self.raw
        )
    }
}

// ─── View ─────────────────────────────────────────────────────────────────────

/// POST body for /rpc/v1/view
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewRequest {
    pub function: String,
    pub type_arguments: Vec<String>,
    pub arguments: Vec<serde_json::Value>,
}

/// Response is a flat JSON array of Move values.
pub type ViewResponse = Vec<serde_json::Value>;

// ─── Faucet ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaucetResponse {
    #[serde(default)]
    pub status: Option<String>,
    // Different faucet versions return different fields — keep it flexible.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

// ─── Transaction ─────────────────────────────────────────────────────────────

/// Minimal tx submission result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxResult {
    pub hash: Option<String>,
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub vm_status: Option<String>,
}

// ─── RPC wrapper ─────────────────────────────────────────────────────────────

/// Generic RPC response wrapper some endpoints use.
#[derive(Debug, Deserialize)]
pub struct RpcResponse<T> {
    pub data: Option<T>,
    pub error: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_address_from_str_valid() {
        let addr = "0x1".parse::<AccountAddress>().unwrap();
        assert_eq!(addr.normalise(), "0x0000000000000000000000000000000000000000000000000000000000000001");
    }

    #[test]
    fn test_address_from_str_no_prefix() {
        let addr = "dead".parse::<AccountAddress>().unwrap();
        assert_eq!(addr.normalise(), "0x000000000000000000000000000000000000000000000000000000000000dead");
    }

    #[test]
    fn test_address_invalid_chars() {
        assert!("0xZZZZ".parse::<AccountAddress>().is_err());
    }

    #[test]
    fn test_balance_supra() {
        let bal = Balance {
            address: "0x1".parse().unwrap(),
            raw: 1_000_000_000,
        };
        assert!((bal.supra() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_view_request_serializes() {
        let req = ViewRequest {
            function: "0x1::supra_coin::supply".into(),
            type_arguments: vec![],
            arguments: vec![],
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("supra_coin"));
    }
}
