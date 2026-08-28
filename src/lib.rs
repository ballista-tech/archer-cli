//! Archer Protocol CLI, as a library.
//!
pub mod commands;
pub mod config;
pub mod display;
pub mod error;
pub mod tx;
pub mod utils;

pub use config::CliConfig;
pub use error::CliError;

use clap::{Args, Subcommand};

/// Connection, signer and output flags, shared by both binaries so their
/// invocation is identical.
#[derive(Args)]
pub struct GlobalArgs {
    /// Solana RPC URL (default: from Solana CLI config)
    #[arg(short = 'u', long, global = true)]
    pub url: Option<String>,

    /// Signing keypair path (default: from Solana CLI config)
    #[arg(short = 'k', long, global = true)]
    pub keypair: Option<String>,

    /// Commitment level: confirmed or finalized
    #[arg(short = 'c', long, global = true, default_value = "confirmed")]
    pub commitment: String,

    /// Output format: table or json
    #[arg(long, global = true, default_value = "table")]
    pub output: String,

    /// Simulate transaction without sending
    #[arg(long, global = true)]
    pub dry_run: bool,

    /// Priority fee in micro-lamports per CU
    #[arg(long, global = true, default_value = "0")]
    pub priority_fee: u64,
}

impl GlobalArgs {
    pub fn load(self) -> Result<CliConfig, CliError> {
        CliConfig::load(
            self.url,
            self.keypair,
            &self.commitment,
            self.dry_run,
            self.output == "json",
            self.priority_fee,
        )
    }
}

// Everything a market creator, market maker or taker can sign for.
//
#[derive(Subcommand)]
pub enum PublicCommands {
    /// Market lifecycle: create and inspect
    #[command(subcommand)]
    Market(commands::market::MarketCommands),

    /// Transfer a market's admin seat (its 80% fee share)
    #[command(subcommand)]
    Admin(commands::admin::AdminCommands),

    /// Fee collection for markets you administer
    #[command(subcommand)]
    Fees(commands::fee::FeeCommands),

    /// Maker registry: show
    #[command(subcommand)]
    Registry(commands::registry::RegistryCommands),

    /// Maker ops: list funded books
    #[command(subcommand)]
    Maker(commands::maker::MakerCommands),

    /// Read-only observability: market, books, liquidity, swaps, vaults
    #[command(subcommand)]
    Observe(commands::observe::ObserveCommands),
}

pub fn dispatch(command: PublicCommands, config: &CliConfig) -> Result<(), CliError> {
    match command {
        PublicCommands::Market(cmd) => commands::market::handle(cmd, config),
        PublicCommands::Admin(cmd) => commands::admin::handle(cmd, config),
        PublicCommands::Fees(cmd) => commands::fee::handle(cmd, config),
        PublicCommands::Registry(cmd) => commands::registry::handle(cmd, config),
        PublicCommands::Maker(cmd) => commands::maker::handle(cmd, config),
        PublicCommands::Observe(cmd) => commands::observe::handle(cmd, config),
    }
}
