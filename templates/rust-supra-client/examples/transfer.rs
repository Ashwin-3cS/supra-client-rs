//! Demonstrates a full native SUPRA coin transfer using `TxBuilder`.
//!
//! This example shows how to:
//!   1. Load a keypair from the SUPRA_PRIVATE_KEY env var
//!   2. Request faucet funds if needed
//!   3. Use `TxBuilder` to build and sign a transfer with automated gas estimation
//!   4. Submit the transaction and wait for finality
//!
//! Run:
//!   SUPRA_PRIVATE_KEY=<your-hex-key> cargo run --example transfer

use anyhow::Result;
use supra_rust_client::{Keypair, SupraClient, TxBuilder};

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv();
    let client = SupraClient::new(None, None);

    // ── Step 1: Load sender keypair ───────────────────────────────────────────
    let keypair = Keypair::from_env()?;
    let sender = keypair.address();
    println!("Sender : {}", sender);

    // ── Step 2: Generate a fresh recipient address ────────────────────────────
    let recipient = Keypair::generate().address();
    println!("To     : {}", recipient);

    // ── Step 3: Ensure sender is funded ──────────────────────────────────────
    let balance = client.get_balance(sender.clone()).await?;
    println!("Balance: {} SUPRA", balance.supra());
    if balance.raw == 0 {
        println!("Balance is zero — requesting faucet funds...");
        client.faucet(&sender).await?;
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }

    // ── Step 4: Build and submit ────────────────────────────────────────────────
    // Transfer 0.01 SUPRA (10_000_000 octas)
    let amount = 10_000_000u64;
    println!("\nTransferring {} octas ({} SUPRA)...", amount, amount as f64 / 1_000_000_000.0);

    let builder = TxBuilder::new(&client, &keypair);

    // Fetch live gas price so we can display it.
    let gas_price = client.get_gas_price().await.unwrap_or(100_000);
    println!("  Gas unit price : {} octas", gas_price);

    // builder.transfer() uses the TS-SDK gas limits:
    //   - 10 units if recipient account already exists
    //   - 1020 units if recipient is a brand-new account
    let signed_tx = builder.transfer(&recipient, amount).await?;

    // ── Step 5: Submit ────────────────────────────────────────────────────────
    println!("\nSubmitting to mempool...");
    let result = client.submit_transaction(&signed_tx).await?;
    let hash = result.hash.unwrap_or_else(|| "N/A".into());
    println!("Transaction hash : {}", hash);

    // ── Step 6: Wait for finality ─────────────────────────────────────────────
    println!("Waiting for finality...");
    let final_status = client.wait_for_transaction(&hash).await?;
    let status = final_status.get("status").and_then(|s| s.as_str()).unwrap_or("Unknown");
    println!("Final status     : {}", status);

    if status == "Success" {
        println!("\nTransfer successful!");
        println!("Explorer: https://testnet.suprascan.io/tx/{}", hash);
    } else {
        println!("\nTransaction failed. Full response:\n{}", serde_json::to_string_pretty(&final_status)?);
    }

    Ok(())
}
