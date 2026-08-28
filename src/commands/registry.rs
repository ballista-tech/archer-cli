//! Maker registry inspection.
//!

use clap::Subcommand;

use archer_sdk::onchain::MakerRegistry;

use crate::{commands::market::parse_pubkey, config::CliConfig, display, error::CliError};

#[derive(Subcommand)]
pub enum RegistryCommands {
    /// Show the current registry state for a market
    Show {
        /// Market pubkey
        #[arg(long)]
        market: String,
    },
}

pub fn handle(cmd: RegistryCommands, config: &CliConfig) -> Result<(), CliError> {
    match cmd {
        RegistryCommands::Show { market } => {
            let market_pk = parse_pubkey(&market)?;
            let (registry_pk, _) = archer_sdk::pda::derive_maker_registry(&market_pk);

            display::print_header("Maker Registry");
            display::print_kv("Market:", &market);
            display::print_kv("Registry PDA:", &registry_pk.to_string());

            let account = config
                .rpc_client
                .get_account(&registry_pk)
                .map_err(|e| CliError::Rpc(format!("Failed to fetch registry: {e}")))?;

            if account.data.len() < MakerRegistry::LEN {
                return Err(CliError::Deserialization(
                    "Registry account data too small".into(),
                ));
            }

            let registry = MakerRegistry::load(&account.data)
                .map_err(|e| CliError::Deserialization(format!("Failed to parse registry: {e}")))?;

            let num = registry.num_makers as usize;
            display::print_kv("Admin:", &registry.admin.to_string());
            display::print_kv("Registered Makers:", &num.to_string());

            if num > 0 {
                println!();
                for i in 0..num {
                    println!("  {:>3}. {}", i + 1, registry.makers[i]);
                }
            }

            Ok(())
        }
    }
}
