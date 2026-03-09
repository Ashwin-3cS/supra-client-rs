//! `supra-rust-client` — Rust SDK for the Supra MoveVM chain.
//!
//! # Quick Start
//! ```no_run
//! use supra_rust_client::{SupraClient, AccountAddress};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let client = SupraClient::new(None, None);
//!     let addr: AccountAddress = "0x1".parse()?;
//!     let balance = client.get_balance(addr).await?;
//!     println!("{}", balance);
//!     Ok(())
//! }
//! ```

pub mod builder;
pub mod client;
pub mod signing;
pub mod types;

// Re-export the most commonly used items at the crate root.
pub use builder::{GasEstimate, MoveArg, TxBuilder};
pub use client::SupraClient;
pub use signing::Keypair;
pub use types::{
    AccountAddress, AccountInfo, Balance, EntryFunction, FaucetResponse, Identifier, ModuleId,
    RawTransaction, TransactionPayload, TxResult, TypeTag, ViewRequest, ViewResponse,
};
