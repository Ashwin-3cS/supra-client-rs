//! Supra CLI — interact with Supra MoveVM testnet from your terminal.
//!
//! Usage:
//!   supra balance <ADDRESS>
//!   supra faucet  <ADDRESS>
//!   supra account <ADDRESS>
//!   supra view    <ADDRESS> <MODULE> <FUNCTION> [--args <JSON>...]

use anyhow::Result;
use clap::{Parser, Subcommand};
use supra_rust_client::{
    AccountAddress, SupraClient, ViewRequest, Keypair, TxBuilder,
};

/// Supra Rust CLI — query balances, call view functions, and request faucet tokens.
#[derive(Parser, Debug)]
#[command(
    name = "supra",
    about = "Supra MoveVM testnet CLI (Chain ID 6)",
    version,
    author
)]
struct Cli {
    /// RPC endpoint (defaults to https://rpc-testnet.supra.com or $SUPRA_RPC_URL)
    #[arg(long, global = true, env = "SUPRA_RPC_URL")]
    rpc_url: Option<String>,

    /// Faucet endpoint (defaults to https://faucet-testnet.supra.com or $SUPRA_FAUCET_URL)
    #[arg(long, global = true, env = "SUPRA_FAUCET_URL")]
    faucet_url: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Fetch SUPRA coin balance for an address.
    Balance {
        /// Account address (0x... hex)
        address: AccountAddress,
    },

    /// Request testnet SUPRA from the faucet.
    Faucet {
        /// Account address to fund
        address: AccountAddress,
    },

    /// Transfer SUPRA to another address. Requires SUPRA_PRIVATE_KEY env var.
    Transfer {
        /// Recipient address (0x... hex)
        to: AccountAddress,
        /// Amount in RAW units (e.g. 100000000 = 1 SUPRA)
        amount: u64,
    },

    /// Fetch account info (sequence number, auth key).
    Account {
        /// Account address (0x... hex)
        address: AccountAddress,
    },

    /// Call a Move view function.
    ///
    /// Example: supra view 0x1 coin supply --type-args "0x1::supra_coin::SupraCoin"
    View {
        /// Module address (e.g. 0x1)
        address: AccountAddress,
        /// Module name (e.g. coin)
        module: String,
        /// Function name (e.g. supply)
        function: String,
        /// Type arguments (repeat flag for multiple)
        #[arg(long = "type-args", num_args = 0..)]
        type_args: Vec<String>,
        /// JSON arguments (repeat flag for multiple)
        #[arg(long = "args", num_args = 0..)]
        args: Vec<String>,
    },

    /// Print chain / ledger info (connectivity check).
    Info,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env file if present (optional, non-fatal).
    let _ = dotenvy::dotenv();

    let cli = Cli::parse();
    let client = SupraClient::new(cli.rpc_url, cli.faucet_url);

    match cli.command {
        Commands::Balance { address } => {
            let balance = client.get_balance(address).await?;
            println!("{}", balance);
        }

        Commands::Faucet { address } => {
            println!("Requesting testnet SUPRA for {}...", address);
            let resp = client.faucet(&address).await?;
            println!("Faucet response: {}", serde_json::to_string_pretty(&resp.extra)?);
            if let Some(status) = resp.status {
                println!("Status: {}", status);
            }
        }

        Commands::Transfer { to, amount } => {
            let keypair = Keypair::from_env()?;
            let sender_addr = keypair.address();
            println!("\nSender : {}", sender_addr);
            println!("To     : {}", to);
            println!("Amount : {} octas ({} SUPRA)", amount, amount as f64 / 1_000_000_000.0);

            let builder = TxBuilder::new(&client, &keypair);

            // Show live gas price
            let gas_price = client.get_gas_price().await.unwrap_or(100_000);
            println!("\nGas unit price : {} octas", gas_price);
            println!("(max_gas: 10 units for existing accounts, 1020 for new ones)");

            // Use the TS-SDK-aligned transfer which checks recipient existence
            // and picks the correct max_gas cap (10 vs 1020 units).
            let signed_tx = builder.transfer(&to, amount).await?;

            println!("\nSubmitting transaction...");
            let tx_res = client.submit_transaction(&signed_tx).await?;
            let tx_hash = tx_res.hash.clone().unwrap_or_else(|| "unknown_hash".into());
            println!("Transaction hash: {}", tx_hash);

            println!("Waiting for finality...");
            let finality = client.wait_for_transaction(&tx_hash).await?;

            let status = finality.get("status").and_then(|s| s.as_str()).unwrap_or("Unknown");
            if status == "Success" {
                println!("\nTransfer completed successfully!");
                println!("Explorer: https://testnet.suprascan.io/tx/{}", tx_hash);
            } else {
                let vm_status = finality
                    .pointer("/output/Move/vm_status")
                    .and_then(|s| s.as_str())
                    .unwrap_or("Unknown");
                println!("\nTransaction failed. Status: {}, VM Status: {}", status, vm_status);
            }
        }

        Commands::Account { address } => {
            let info = client.get_account(&address).await?;
            println!("Address         : {}", address);
            println!("Sequence number : {}", info.sequence_number);
            println!("Auth key        : {}", info.authentication_key);
        }

        Commands::View {
            address,
            module,
            function,
            type_args,
            args,
        } => {
            // Build fully-qualified function string: <addr>::<module>::<function>
            let fn_str = format!("{}::{}::{}", address.normalise(), module, function);

            // Parse JSON args (strings are passed as Move string values).
            let parsed_args: Vec<serde_json::Value> = args
                .iter()
                .map(|a| {
                    serde_json::from_str(a).unwrap_or_else(|_| serde_json::Value::String(a.clone()))
                })
                .collect();

            let req = ViewRequest {
                function: fn_str.clone(),
                type_arguments: type_args,
                arguments: parsed_args,
            };

            println!("Calling view: {}", fn_str);
            let result = client.view(req).await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }

        Commands::Info => {
            let info = client.get_ledger_info().await?;
            println!("{}", serde_json::to_string_pretty(&info)?);
        }
    }

    Ok(())
}
