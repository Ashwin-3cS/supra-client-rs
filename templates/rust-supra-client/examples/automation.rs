//! Example: Keypair generation + faucet + automation registry query.
//!
//! Demonstrates:
//!   1. Generating a fresh Ed25519 keypair
//!   2. Deriving the on-chain address
//!   3. Funding via the testnet faucet
//!   4. Querying the automation registry view function
//!
//! Run: cargo run --example automation

use anyhow::Result;
use supra_rust_client::{Keypair, SupraClient, ViewRequest};

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv();
    let client = SupraClient::new(None, None);

    // ── Step 1: Generate a fresh keypair ─────────────────────────────────────
    let keypair = Keypair::generate();
    let address = keypair.address();

    println!("\n Generated new keypair");
    println!("  Public key : {}", keypair.public_hex());
    println!("  Address    : {}", address);

    // ── Step 2: Fund via faucet ───────────────────────────────────────────────
    println!("\n Requesting testnet SUPRA from faucet...");
    match client.faucet(&address).await {
        Ok(resp) => println!("  Faucet OK: {}", serde_json::to_string(&resp.extra)?),
        Err(e)   => println!("  Faucet error (non-fatal, rate-limits apply): {}", e),
    }

    // ── Step 3: Check balance ────────────────────────────────────────────────
    println!("\n Checking balance...");
    match client.get_balance(address.clone()).await {
        Ok(bal) => println!("  {}", bal),
        Err(e)  => println!("  Not yet on-chain (new account): {}", e),
    }

    // ── Step 4: Query automation registry ────────────────────────────────────
    println!("\n Querying automation registry...");
    let req = ViewRequest {
        function: "0x1::automation_registry::get_tasks".into(),
        type_arguments: vec![],
        arguments: vec![serde_json::json!(address.normalise())],
    };

    match client.view(req).await {
        Ok(result) => println!("  Tasks: {}", serde_json::to_string_pretty(&result)?),
        Err(e)     => println!("  No tasks yet (new account): {}", e),
    }

    println!();
    Ok(())
}
