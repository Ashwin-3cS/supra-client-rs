# supra-client-rs 

## supra-rust-client

Development of a Rust-based SDK for the Supra MoveVM testnet. The crate is located in `templates/rust-supra-client`.

### Status & Integration Readiness (v0.1.0 MVP)

The Rust SDK has reached **MVP maturity** and is ready for initial development and integration! 

It currently supports the most critical flows required by developers:
1. **Wallet & Key Management:** Secure generation, loading, and address derivation using Ed25519.
2. **Read Operations:** Fetching account info, native SUPRA coin balances, and executing view functions on smart contracts.
3. **Write Operations:** Formulating, correctly signing (`DOMAIN_SEPARATOR`), simulating (dry-run), and submitting transactions to the Supra testnet.
4. **Tooling:** Interacting with the Faucet for automated testnet funding and awaiting transaction finality through the RPC.

**Integration:** A backend service or a Rust-based tool can import this crate and use `SupraClient` to manage wallets and submit transactions exactly as they would with the TS SDK. Check `templates/rust-supra-client/examples/automation.rs` for a full end-to-end integration flow.

---

### Future Roadmap

To bring the Rust SDK to complete feature parity with the TS SDK, the following enhancements are planned:

1. **High-Level Transaction Builders:** Currently, developers need to manually BCS-encode arguments for complex contract calls (`EntryFunction`). Adding a high-level builder (similar to the TS SDK's payload generation) will make it much easier to interact with custom smart contracts.
2. **Extended Cryptography:** Expanding beyond Single-signer Ed25519 to support Multi-Ed25519, Secp256k1, and WebAuthn authenticators.
3. **Gas Estimation Automation:** Wrapping the `dry_run_transaction` endpoint into an automated gas estimator that dynamically sets the `max_gas_amount` for the user before submission.
4. **Enhanced Resource & Event Fetching:** Adding typed abstractions to fetch any on-chain resource or poll for specific on-chain events, beyond just the native `CoinStore`.

*Please see the [client library README](templates/rust-supra-client/README.md) for full setup and architecture details.*
