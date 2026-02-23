//! Example: Query Supra Oracle on-chain price feeds.
//!
//! Run: cargo run --example oracle_feed

use anyhow::Result;
use supra_rust_client::{SupraClient, ViewRequest};

/// Supra Oracle module address on testnet.
const ORACLE_ADDR: &str = "0xaa";

/// (pair_index, label) for common pairs.
const PAIRS: &[(u64, &str)] = &[
    (0, "BTC/USD"),
    (1, "ETH/USD"),
    (21, "SOL/USD"),
];

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv();
    let client = SupraClient::new(None, None);

    println!("\n Supra Oracle Price Feed (testnet)\n");
    println!("{:<12} {:>20}  {}", "Pair", "Price (raw)", "Decimals");
    println!("{}", "-".repeat(46));

    for &(pair_index, label) in PAIRS {
        // View function: supra_oracle::oracle_holder::get_price(pair_index: u64)
        let req = ViewRequest {
            function: format!("{}::oracle_holder::get_price", ORACLE_ADDR),
            type_arguments: vec![],
            arguments: vec![serde_json::json!(pair_index.to_string())],
        };

        match client.view(req).await {
            Ok(result) => {
                // Expected: [price_value, decimal_places, timestamp]
                let price    = result.get(0).and_then(|v| v.as_str()).unwrap_or("?");
                let decimals = result.get(1).and_then(|v| v.as_str()).unwrap_or("?");
                println!("{:<12} {:>20}  {}", label, price, decimals);
            }
            Err(e) => {
                println!("{:<12} {:>20}  ERROR: {:?}", label, "-", e);
            }
        }
    }

    println!();
    Ok(())
}
