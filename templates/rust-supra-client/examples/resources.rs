//! `resources` example — demonstrates generic resource fetching.
//!
//! Run:
//!   cargo run --example resources

use anyhow::Result;
use serde::Deserialize;
use supra_rust_client::SupraClient;

/// Mirror of `0x1::coin::CoinInfo<0x1::supra_coin::SupraCoin>` data fields.
#[derive(Deserialize, Debug)]
struct CoinInfo {
    name: String,
    symbol: String,
    decimals: u8,
}

#[tokio::main]
async fn main() -> Result<()> {
    let client = SupraClient::new(None, None);

    let framework = "0x1".parse().unwrap();

    println!("=== Resources on 0x1 (first 10) ===");
    let (resources, next_cursor) = client.list_resources(&framework, Some(10), None).await?;
    for (i, r) in resources.iter().enumerate() {
        println!("{:>2}. {}", i + 1, r["type"].as_str().unwrap_or("<unknown>"));
    }
    if let Some(cursor) = next_cursor {
        println!("   ... more available (cursor: {})", cursor);
    }

    // ── 2. Fetch a specific typed resource and deserialize into a Rust struct ─
    println!("\n=== CoinInfo for SupraCoin ===");
    let coin_info: CoinInfo = client
        .get_resource(
            &framework,
            "0x1::coin::CoinInfo<0x1::supra_coin::SupraCoin>",
        )
        .await?;
    println!("Name    : {}", coin_info.name);
    println!("Symbol  : {}", coin_info.symbol);
    println!("Decimals: {}", coin_info.decimals);

    Ok(())
}
