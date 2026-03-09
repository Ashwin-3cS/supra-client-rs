//! High-level transaction builder for the Supra MoveVM SDK.
//!
//! `TxBuilder` abstracts the manual BCS serialization of `EntryFunction` arguments
//! into a simple, typed API — identical in ergonomics to the TypeScript `supra-l1-sdk`.

use anyhow::{Context, Result};

use crate::{
    client::SupraClient,
    signing::Keypair,
    types::{
        AccountAddress, EntryFunction, Identifier, ModuleId, RawTransaction, SignedTransaction,
        TransactionPayload, TypeTag,
    },
};

// ─── Gas constants ──────────────────────────────────────────────────────────

/// Default gas budget — matches TS SDK DEFAULT_MAX_GAS_UNITS = 500_000.
const DEFAULT_MAX_GAS: u64 = 500_000;
/// Fallback gas price if live fetch fails. TS SDK DEFAULT_GAS_PRICE = 100_000.
const DEFAULT_GAS_PRICE: u64 = 100_000;
/// Default TTL: 300 seconds (5 minutes) to match TS SDK DEFAULT_TX_EXPIRATION_DURATION.
const DEFAULT_TTL_SECS: u64 = 300;
/// Safety multiplier applied on top of simulated gas usage.
const GAS_BUFFER: f64 = 1.3;
/// Minimum gas allowed by the Supra testnet node.
const MIN_GAS_FLOOR: u64 = 10_000;
/// Max gas for SUPRA coin transfer when the receiver account already exists.
/// From TS SDK: DEFAULT_MAX_GAS_FOR_SUPRA_TRANSFER_WHEN_RECEIVER_EXISTS = 10
const SUPRA_TRANSFER_MAX_GAS_EXISTING: u64 = 10;
/// Max gas for SUPRA coin transfer when the receiver account does NOT exist yet.
/// From TS SDK: DEFAULT_MAX_GAS_FOR_SUPRA_TRANSFER_WHEN_RECEIVER_NOT_EXISTS = 1020
const SUPRA_TRANSFER_MAX_GAS_NEW: u64 = 1020;

// ─── MoveArg ────────────────────────────────────────────────────────────────

/// A strongly-typed Move function argument.
///
/// Each variant maps to the corresponding Move primitive type and is automatically
/// BCS-serialized when passed to `TxBuilder::entry_function`.
#[derive(Debug, Clone)]
pub enum MoveArg {
    /// Move `address` — a 32-byte account address.
    Address(AccountAddress),
    /// Move `bool`.
    Bool(bool),
    /// Move `u8`.
    U8(u8),
    /// Move `u16`.
    U16(u16),
    /// Move `u32`.
    U32(u32),
    /// Move `u64`.
    U64(u64),
    /// Move `u128`.
    U128(u128),
    /// Move `vector<u8>` — raw bytes, BCS-serialized as a length-prefixed sequence.
    Bytes(Vec<u8>),
    /// Move `0x1::string::String` — UTF-8 string, BCS-serialized as a `vector<u8>`.
    Str(String),
}

impl MoveArg {
    /// Serialize this argument to BCS bytes suitable for `EntryFunction::args`.
    pub fn to_bcs(&self) -> Result<Vec<u8>> {
        match self {
            MoveArg::Address(addr) => bcs::to_bytes(addr).context("BCS serialize Address"),
            MoveArg::Bool(b) => bcs::to_bytes(b).context("BCS serialize Bool"),
            MoveArg::U8(v) => bcs::to_bytes(v).context("BCS serialize U8"),
            MoveArg::U16(v) => bcs::to_bytes(v).context("BCS serialize U16"),
            MoveArg::U32(v) => bcs::to_bytes(v).context("BCS serialize U32"),
            MoveArg::U64(v) => bcs::to_bytes(v).context("BCS serialize U64"),
            MoveArg::U128(v) => bcs::to_bytes(v).context("BCS serialize U128"),
            MoveArg::Bytes(b) => bcs::to_bytes(b).context("BCS serialize Bytes"),
            // Move String is represented on-chain as vector<u8> (UTF-8).
            MoveArg::Str(s) => bcs::to_bytes(s.as_bytes()).context("BCS serialize Str"),
        }
    }
}

// ─── GasEstimate ────────────────────────────────────────────────────────────

/// Gas estimate returned from a dry-run simulation.
#[derive(Debug, Clone)]
pub struct GasEstimate {
    /// Actual gas consumed by the simulated transaction.
    pub gas_used: u64,
    /// Suggested `max_gas_amount` with a safety buffer applied.
    pub suggested_max: u64,
    /// Gas unit price reported by the simulation (in octas).
    pub gas_unit_price: u64,
}

// ─── TxBuilder ──────────────────────────────────────────────────────────────

/// High-level transaction builder.
///
/// Handles sequence number fetching, argument serialization, expiry, and signing.
/// Use `build_with_gas_estimate` to also auto-size gas from a dry-run simulation.
pub struct TxBuilder<'a> {
    client: &'a SupraClient,
    keypair: &'a Keypair,
}

impl<'a> TxBuilder<'a> {
    /// Create a new builder backed by the given client and signing keypair.
    pub fn new(client: &'a SupraClient, keypair: &'a Keypair) -> Self {
        Self { client, keypair }
    }

    // ─── Entry-function builder ──────────────────────────────────────────────

    /// Build and sign a transaction that calls an arbitrary Move entry function.
    ///
    /// - `module` — fully-qualified module path, e.g. `"0x1::supra_account"`.
    /// - `function` — entry function name, e.g. `"transfer"`.
    /// - `type_args` — Move type arguments (e.g. coin type tags); pass `vec![]` if none.
    /// - `args` — typed Move arguments in call order; see [`MoveArg`].
    ///
    /// Uses hardcoded gas defaults. For a gas-estimated version, see
    /// [`build_with_gas_estimate`].
    pub async fn entry_function(
        &self,
        module: &str,
        function: &str,
        type_args: Vec<TypeTag>,
        args: Vec<MoveArg>,
    ) -> Result<SignedTransaction> {
        let raw = self.build_raw(module, function, type_args, args, None).await?;
        self.keypair.sign_transaction(&raw)
    }

    /// Build, dry-run to estimate gas, then sign a final transaction with safe gas bounds.
    ///
    /// This is the recommended production path: it avoids both wasted gas (from
    /// overestimation) and failed transactions (from underestimation).
    pub async fn build_with_gas_estimate(
        &self,
        module: &str,
        function: &str,
        type_args: Vec<TypeTag>,
        args: Vec<MoveArg>,
    ) -> Result<SignedTransaction> {
        // 1. Build with default gas for the dry-run.
        let draft_raw = self.build_raw(module, function, type_args.clone(), args.clone(), None).await?;
        let draft_signed = self.keypair.sign_transaction(&draft_raw)?;

        // 2. Simulate to get actual gas usage.
        let estimate = self.estimate_gas(&draft_signed).await?;

        // 3. Re-build with the estimated gas.
        let final_raw = self
            .build_raw(module, function, type_args, args, Some(estimate.suggested_max))
            .await?;
        self.keypair.sign_transaction(&final_raw)
    }

    // ─── Convenience shortcuts ───────────────────────────────────────────────

    /// Transfer native SUPRA coin to another account.
    ///
    /// `amount` is in raw octa units (1 SUPRA = 1_000_000_000 octas).
    /// Uses the same gas limits as the TS SDK: 10 gas units for existing accounts,
    /// 1020 for accounts that don't yet exist on-chain.
    pub async fn transfer(&self, to: &AccountAddress, amount: u64) -> Result<SignedTransaction> {
        // Check if the recipient already exists to choose the right gas cap.
        let max_gas = match self.client.get_account(to).await {
            Ok(_) => SUPRA_TRANSFER_MAX_GAS_EXISTING,
            Err(_) => SUPRA_TRANSFER_MAX_GAS_NEW,
        };
        let raw = self.build_raw("0x1::supra_account", "transfer", vec![], vec![MoveArg::Address(to.clone()), MoveArg::U64(amount)], Some(max_gas)).await?;
        self.keypair.sign_transaction(&raw)
    }

    /// Transfer native SUPRA coin, with gas auto-estimated from a dry-run.
    pub async fn transfer_with_gas_estimate(
        &self,
        to: &AccountAddress,
        amount: u64,
    ) -> Result<SignedTransaction> {
        self.build_with_gas_estimate(
            "0x1::supra_account",
            "transfer",
            vec![],
            vec![MoveArg::Address(to.clone()), MoveArg::U64(amount)],
        )
        .await
    }

    // ─── Gas estimation (public helper) ─────────────────────────────────────

    /// Simulates a signed transaction and returns a `GasEstimate`.
    ///
    /// Note: The tx passed here does NOT need to have a valid signature — the
    /// simulation endpoint zeroes it out internally. A draft signed with default
    /// gas is sufficient.
    pub async fn estimate_gas(&self, signed: &SignedTransaction) -> Result<GasEstimate> {
        let sim_result = self.client.dry_run_transaction(signed).await?;

        // The simulation endpoint may return a single object or an array with one entry.
        let entry = if let Some(arr) = sim_result.as_array() {
            arr.first().cloned().context("Simulation response array was empty")?  
        } else {
            sim_result.clone()
        };

        // gas_used may live at top level OR inside output.Move (Supra testnet v1)
        let gas_used: u64 = entry
            .pointer("/output/Move/gas_used")
            .and_then(|v| v.as_u64())
            .or_else(|| entry.get("gas_used").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()))
            .or_else(|| entry.get("gas_used").and_then(|v| v.as_u64()))
            .context("Could not parse 'gas_used' from simulation result")?;

        let gas_unit_price: u64 = entry
            .pointer("/header/gas_unit_price")
            .and_then(|v| v.as_u64())
            .or_else(|| entry.get("gas_unit_price").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()))
            .or_else(|| entry.get("gas_unit_price").and_then(|v| v.as_u64()))
            .unwrap_or(DEFAULT_GAS_PRICE);

        // Use at least 1 gas even if the simulation reports 0 ("Invalid" status means we still got data)
        let effective_gas = gas_used.max(1);
        let suggested_max = ((effective_gas as f64) * GAS_BUFFER).ceil() as u64;
        // Ensure we always meet the node's minimum gas requirement.
        let suggested_max = suggested_max.max(MIN_GAS_FLOOR);

        Ok(GasEstimate {
            gas_used,
            suggested_max,
            gas_unit_price,
        })
    }

    // ─── Internal helpers ────────────────────────────────────────────────────

    /// Parse a `"0xADDR::module"` string into a `ModuleId`.
    fn parse_module(module: &str) -> Result<ModuleId> {
        let parts: Vec<&str> = module.splitn(2, "::").collect();
        if parts.len() != 2 {
            anyhow::bail!(
                "Invalid module path '{}': expected format '0xADDR::module_name'",
                module
            );
        }
        Ok(ModuleId {
            address: parts[0].parse::<AccountAddress>()?,
            name: Identifier(parts[1].to_string()),
        })
    }

    /// Core internal builder — assembles a `RawTransaction` ready for signing.
    async fn build_raw(
        &self,
        module: &str,
        function: &str,
        type_args: Vec<TypeTag>,
        args: Vec<MoveArg>,
        max_gas_override: Option<u64>,
    ) -> Result<RawTransaction> {
        let sender = self.keypair.address();

        // Fetch live sequence number.
        let account = self
            .client
            .get_account(&sender)
            .await
            .context("Failed to fetch sender account info — is the account funded?")?;

        // Fetch live gas price from the node (mirrors TS SDK's `getMinGasUnitPrice`).
        let gas_unit_price = self
            .client
            .get_gas_price()
            .await
            .unwrap_or(DEFAULT_GAS_PRICE);

        // Serialise Move args to BCS.
        let bcs_args: Vec<Vec<u8>> = args
            .iter()
            .map(|a| a.to_bcs())
            .collect::<Result<_>>()?;

        let payload = TransactionPayload::EntryFunction(EntryFunction {
            module: Self::parse_module(module)?,
            function: Identifier(function.to_string()),
            ty_args: type_args,
            args: bcs_args,
        });

        // Use chain ledger timestamp as the base to avoid local clock skew issues.
        // Fallback to local system time if we can't reach the RPC.
        let now_secs = self.chain_timestamp_secs().await.unwrap_or_else(|_| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        });
        let expiry = now_secs + DEFAULT_TTL_SECS;

        Ok(RawTransaction {
            sender,
            sequence_number: account.sequence_number,
            payload,
            max_gas_amount: max_gas_override.unwrap_or(DEFAULT_MAX_GAS),
            gas_unit_price,
            expiration_timestamp_secs: expiry,
            chain_id: self.client.chain_id,
        })
    }

    /// Fetch the current chain timestamp in seconds.
    ///
    /// The ledger info returns `block_time` in microseconds; we convert to seconds.
    async fn chain_timestamp_secs(&self) -> Result<u64> {
        let ledger = self.client.get_ledger_info().await?;
        // Response includes "block_time" as microseconds from epoch
        let micros = ledger
            .get("block_time")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<u64>().ok())
            .or_else(|| ledger.get("block_time").and_then(|v| v.as_u64()))
            .context("Could not parse 'block_time' from ledger info")?;
        Ok(micros / 1_000_000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(arg: &MoveArg) -> Vec<u8> {
        arg.to_bcs().expect("BCS serialization should not fail")
    }

    #[test]
    fn test_move_arg_u64_bcs() {
        // BCS encodes u64 as 8 little-endian bytes.
        let bytes = decode(&MoveArg::U64(1));
        assert_eq!(bytes, vec![1, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn test_move_arg_bool_bcs() {
        assert_eq!(decode(&MoveArg::Bool(true)), vec![1]);
        assert_eq!(decode(&MoveArg::Bool(false)), vec![0]);
    }

    #[test]
    fn test_move_arg_u8_bcs() {
        assert_eq!(decode(&MoveArg::U8(42)), vec![42]);
    }

    #[test]
    fn test_move_arg_str_bcs() {
        // BCS string = BCS bytes of UTF-8 bytes: length-prefixed u8 sequence.
        let bytes = decode(&MoveArg::Str("hi".into()));
        // Inner "hi" as bytes is [104, 105]. BCS vector<u8> = [len=2, 104, 105].
        assert_eq!(bytes, vec![2, 104, 105]);
    }

    #[test]
    fn test_move_arg_bytes_bcs() {
        let bytes = decode(&MoveArg::Bytes(vec![0xDE, 0xAD]));
        // BCS vector<u8>: [len=2, 0xDE, 0xAD]
        assert_eq!(bytes, vec![2, 0xDE, 0xAD]);
    }

    #[test]
    fn test_parse_module_valid() {
        let mid = TxBuilder::parse_module("0x1::supra_account").unwrap();
        assert_eq!(mid.name.0, "supra_account");
    }

    #[test]
    fn test_parse_module_invalid() {
        assert!(TxBuilder::parse_module("no_double_colon").is_err());
    }
}

