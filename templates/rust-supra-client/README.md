# supra-client-rs

# supra-rust-client

A production-ready Rust SDK and CLI for interacting with the **Supra MoveVM testnet** (Chain ID 6).

This is the Rust counterpart to Supra's TypeScript `supra-l1-sdk` — same RPC endpoints, same JSON payloads, idiomatic async Rust. This SDK has reached **MVP (Minimum Viable Product) maturity** and is ready for integration in backend services or CLI tools!

---

## Prerequisites

- Rust 1.75+ (`rustup update stable`)
- Internet access to `rpc-testnet.supra.com`

---

## Quick Start

```bash
git clone <this-repo>
cd templates/rust-supra-client
cargo build
```

### Check a balance

```bash
cargo run -- balance 0x742d35Cc6634C0532925a3b8D4C9C4e5aCe2c1A2
```

```
Address : 0x000...a2
Balance : 10.5 SUPRA  (10500000000 raw units)
```

### Request faucet tokens

```bash
cargo run -- faucet 0x<your-address>
```

### Call a Move view function

```bash
# Query coin supply
cargo run -- view 0x1 coin supply --type-args "0x1::supra_coin::SupraCoin"

# Generic view call
cargo run -- view <MODULE_ADDR> <MODULE_NAME> <FUNCTION> \
  --type-args "T1" "T2" \
  --args "\"arg1\"" "42"
```

### Account info

```bash
cargo run -- account 0x1
```

### Transfer Tokens

```bash
# Provide private key via env var
SUPRA_PRIVATE_KEY=<hex> cargo run -- transfer <TO_ADDRESS> 10000000
```

### Chain info / connectivity check

```bash
cargo run -- info
```

### Explore On-Chain Resources

```bash
# List all resource types held by an account
cargo run -- resources 0x1 --count 10

# Fetch a specific resource as JSON
cargo run -- resource 0x1 "0x1::coin::CoinInfo<0x1::supra_coin::SupraCoin>"
```

---

## Library Usage

```rust
use supra_rust_client::{SupraClient, AccountAddress, Keypair, builder::TxBuilder};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = SupraClient::new(None, None);

    // 1. Fetch native SUPRA balance
    let addr: AccountAddress = "0x1".parse()?;
    let balance = client.get_balance(addr).await?;
    println!("Balance: {}", balance);

    // 2. Fetch any generic smart contract resource
    // let my_resource: MyStruct = client.get_resource(&addr, "0x1::my_module::MyStruct").await?;

    // 3. High-level Transaction Builder (like TS SDK)
    let keypair = Keypair::from_env()?;
    let builder = TxBuilder::new(&client, &keypair);
    
    // Automatically fetches live gas price, checks recipient existence for correct gas limits, 
    // and handles BCS serialization entirely under the hood.
    let signed_tx = builder.transfer(&addr, 10_000_000).await?;
    let tx_res = client.submit_transaction(&signed_tx).await?;
    
    println!("Submitted: {:?}", tx_res.hash);

    Ok(())
}
```

---

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `SUPRA_RPC_URL` | `https://rpc-testnet.supra.com` | RPC endpoint |
| `SUPRA_FAUCET_URL` | `https://faucet-testnet.supra.com` | Faucet endpoint |
| `SUPRA_PRIVATE_KEY` | — | 32-byte Ed25519 seed (hex, no 0x) |

Create a `.env` file in the project root (loaded automatically):

```env
SUPRA_RPC_URL=https://rpc-testnet.supra.com
SUPRA_PRIVATE_KEY=<your-64-hex-char-private-key>
```

---

## Examples

```bash
# Keypair generation + faucet + transaction submission
cargo run --example automation
```

---

## Architecture

```
src/
├── lib.rs       — public re-exports
├── main.rs      — CLI (clap subcommands)
├── client.rs    — SupraClient (async reqwest)
├── types.rs     — AccountAddress, Balance, ViewRequest, ...
└── signing.rs   — Ed25519 Keypair, address derivation
examples/
├── oracle_feed.rs
└── automation.rs
```

**TS → Rust mapping:**

| TypeScript | Rust |
|---|---|
| `new SupraClient(url)` | `SupraClient::new(None, None)` |
| `client.getAccountBalance(addr)` | `client.get_balance(addr).await?` |
| `client.invokeViewMethod(...)` | `client.view(req).await?` |
| `client.airdropTestSupraCoin(addr)` | `client.faucet(&addr).await?` |
| `client.getAccountResources(addr)` | `client.list_resources(&addr, limit, cursor).await?` |
| `client.getResourceData(addr, type)` | `client.get_resource::<T>(&addr, type).await?` |
| `client.transferSupraCoin(...)` | `builder.transfer(&to, amount).await?` |

---

## Running Tests

```bash
cargo test        # unit tests (offline, no network needed)
cargo clippy      # lint
cargo fmt --check # format check
```

---

## Testnet Resources

- Explorer: https://testnet.suprascan.io
- Faucet UI: https://faucet.supra.com
- RPC base: `https://rpc-testnet.supra.com`
- Chain ID: **6**

---

## License

MIT

---

## SDK Status & Integration Readiness (v0.2.0)

The Rust SDK is highly mature and in exact feature-parity with the core APIs of the TS SDK:

1. **Wallet & Key Management:** Secure generation, loading, and address derivation using Ed25519.
2. **High-Level Transaction Builder:** The `TxBuilder` abstracts away all BCS encoding, expiry calculations, and raw payload generation exactly like the official TS SDK.
3. **Smart Gas Management:** Automatically fetches live chain `min_configured_gas_price` and correctly calibrates transfer gas limits based on recipient account existence. 
4. **Read Operations:** Fetch account info, native balances, execute view functions, list paginated resources, and deserialize *any* generic on-chain resource (`get_resource<T>`).
5. **Tooling:** Interacting with the Faucet for automated testnet funding and awaiting transaction finality through the RPC.

**Integration:** A backend service or a Rust-based tool can import this crate and use `SupraClient` to manage wallets and submit transactions exactly as they would with the TS SDK. Check `examples/transfer.rs` and `examples/resources.rs` for full end-to-end integration flows.

---

## Future Roadmap

The following advanced cryptographic enhancements are planned for broader ecosystem compatibility:

1. **Extended Cryptography (Multi-sig):** Expanding beyond Single-signer Ed25519 to support Multi-Ed25519 threshold signers.
2. **EVM Compatibility:** Adding support for Secp256k1 signing keys.
