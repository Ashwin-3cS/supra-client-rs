//! Core types for the Supra RPC client.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

// ─── Address ─────────────────────────────────────────────────────────────────

/// A 32-byte MoveVM account address (hex-encoded in JSON, e.g. "0x000...abc").
#[derive(Clone, Debug, PartialEq, Eq)]
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

impl serde::Serialize for AccountAddress {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if serializer.is_human_readable() {
            serializer.serialize_str(&self.normalise())
        } else {
            use serde::ser::SerializeTuple;
            // Pad to exactly 64 hex chars (32 bytes) before decoding
            let padded = format!("{:0>64}", self.0.trim_start_matches("0x"));
            let bytes = hex::decode(&padded).map_err(serde::ser::Error::custom)?;
            assert_eq!(bytes.len(), 32, "AccountAddress must be exactly 32 bytes");
            let mut tup = serializer.serialize_tuple(32)?;
            for byte in &bytes {
                tup.serialize_element(byte)?;
            }
            tup.end()
        }
    }
}

impl<'de> serde::Deserialize<'de> for AccountAddress {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            <AccountAddress as FromStr>::from_str(&s).map_err(serde::de::Error::custom)
        } else {
            struct AddressVisitor;
            impl<'de> serde::de::Visitor<'de> for AddressVisitor {
                type Value = AccountAddress;

                fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                    formatter.write_str("32 bytes for AccountAddress")
                }

                fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
                where
                    A: serde::de::SeqAccess<'de>,
                {
                    let mut bytes = [0u8; 32];
                    for (i, byte) in bytes.iter_mut().enumerate() {
                        *byte = seq
                            .next_element()?
                            .ok_or_else(|| serde::de::Error::invalid_length(i, &self))?;
                    }
                    Ok(AccountAddress(format!("0x{}", hex::encode(bytes))))
                }
            }
            deserializer.deserialize_tuple(32, AddressVisitor)
        }
    }
}

// ─── Account Info ─────────────────────────────────────────────────────────────

/// Basic account information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    pub sequence_number: u64,
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

// ─── Phase 2: Transaction Building Types ─────────────────────────────────────

/// The Ed25519 signature wrapping format.
#[derive(Debug, Clone)]
pub struct Ed25519Signature(pub [u8; 64]);

impl serde::Serialize for Ed25519Signature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if serializer.is_human_readable() {
            serializer.serialize_str(&format!("0x{}", hex::encode(self.0)))
        } else {
            serializer.serialize_bytes(&self.0)
        }
    }
}

impl<'de> serde::Deserialize<'de> for Ed25519Signature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            let bytes =
                hex::decode(s.trim_start_matches("0x")).map_err(serde::de::Error::custom)?;
            let mut arr = [0u8; 64];
            arr.copy_from_slice(&bytes);
            Ok(Ed25519Signature(arr))
        } else {
            let bytes: Vec<u8> = serde::Deserialize::deserialize(deserializer)?;
            let mut arr = [0u8; 64];
            arr.copy_from_slice(&bytes);
            Ok(Ed25519Signature(arr))
        }
    }
}

/// The Ed25519 public key wrapping format.
#[derive(Debug, Clone)]
pub struct Ed25519PublicKey(pub [u8; 32]);

impl serde::Serialize for Ed25519PublicKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if serializer.is_human_readable() {
            serializer.serialize_str(&format!("0x{}", hex::encode(self.0)))
        } else {
            serializer.serialize_bytes(&self.0)
        }
    }
}

impl<'de> serde::Deserialize<'de> for Ed25519PublicKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            let bytes =
                hex::decode(s.trim_start_matches("0x")).map_err(serde::de::Error::custom)?;
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            Ok(Ed25519PublicKey(arr))
        } else {
            let bytes: Vec<u8> = serde::Deserialize::deserialize(deserializer)?;
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            Ok(Ed25519PublicKey(arr))
        }
    }
}

/// Specifies the authenticator logic used to verify the transaction.
/// In Supra's testnet, we primarily use single-signer Ed25519.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransactionAuthenticator {
    /// 0: Ed25519 (pubkey, signature)
    Ed25519 {
        public_key: Ed25519PublicKey,
        signature: Ed25519Signature,
    },
    // MultiEd25519, MultiAgent, etc. omitted for MVP
}

/// A structurally identical enum to TypeTag in MoveVM.
/// We use it to pass type arguments to entry functions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TypeTag {
    Bool,
    U8,
    U64,
    U128,
    Address,
    Signer,
    Vector(Box<TypeTag>),
    Struct(StructTag),
    U16,
    U32,
    U256,
}

/// Identifies a specific Move struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructTag {
    pub address: AccountAddress,
    pub module: Identifier,
    pub name: Identifier,
    pub type_params: Vec<TypeTag>,
}

/// Short wrapper for string identifiers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identifier(pub String);

/// Identifies a Move module (Address + Name).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleId {
    pub address: AccountAddress,
    pub name: Identifier,
}

/// Represents the actual execution point and arguments for a Smart Contract call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryFunction {
    pub module: ModuleId,
    pub function: Identifier,
    pub ty_args: Vec<TypeTag>,
    pub args: Vec<Vec<u8>>, // BCS-encoded arguments
}

/// The inner payload of a transaction.
/// BCS serializes enums using their index in the definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransactionPayload {
    /// 0 = Script
    Script(Vec<u8>), // Placeholder
    /// 1 = ModuleBundle (Legacy)
    ModuleBundle(Vec<u8>), // Placeholder
    /// 2 = EntryFunction
    EntryFunction(EntryFunction),
}

/// The base transaction structure containing sequence numbers, payload, gas limits.
/// MUST exactly match the layout of the Move blockchain `RawTransaction`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawTransaction {
    pub sender: AccountAddress,
    pub sequence_number: u64,
    pub payload: TransactionPayload,
    pub max_gas_amount: u64,
    pub gas_unit_price: u64,
    pub expiration_timestamp_secs: u64,
    pub chain_id: u8,
}

/// A fully signed and constructed transaction ready for `POST /transactions`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedTransaction {
    pub raw_txn: RawTransaction,
    pub authenticator: TransactionAuthenticator,
}

// ─── Transaction ─────────────────────────────────────────────────────────────

/// Struct returned by the node after submitting a transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxResult {
    pub hash: Option<String>,
    // other fields omitted
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
        assert_eq!(
            addr.normalise(),
            "0x0000000000000000000000000000000000000000000000000000000000000001"
        );
    }

    #[test]
    fn test_address_from_str_no_prefix() {
        let addr = "dead".parse::<AccountAddress>().unwrap();
        assert_eq!(
            addr.normalise(),
            "0x000000000000000000000000000000000000000000000000000000000000dead"
        );
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
