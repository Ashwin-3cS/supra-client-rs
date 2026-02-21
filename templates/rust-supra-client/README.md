# supra-rust-client

A production-ready Rust SDK and CLI for interacting with the **Supra MoveVM testnet** (Chain ID 6).

This is the Rust counterpart to Supra's TypeScript `supra-l1-sdk` — same RPC endpoints, same JSON payloads, idiomatic async Rust.

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

### Chain info / connectivity check

```bash
cargo run -- info
```

---

## Library Usage

```rust
use supra_rust_client::{SupraClient, AccountAddress, ViewRequest};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = SupraClient::new(None, None);

    // Balance
    let addr: AccountAddress = "0x1".parse()?;
    let balance = client.get_balance(addr).await?;
    println!("{}", balance);     // "Balance : 0 SUPRA  (0 raw units)"

    // View function
    let result = client.view(ViewRequest {
        function: "0x1::supra_coin::supply".into(),
        type_arguments: vec![],
        arguments: vec![],
    }).await?;
    println!("{:?}", result);

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
# Oracle price feeds (BTC/USD, ETH/USD, SOL/USD)
cargo run --example oracle_feed

# Keypair generation + faucet + automation registry
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
- RPC base: `https://rpc-testnet.supra.com/rpc/v1/`
- Chain ID: **6**

---

## License

MIT
