use clap::Subcommand;
use solana_sdk::signature::Signer;

use archer_sdk::onchain::builders::create_transfer_admin_instruction;

use crate::{commands::market::parse_pubkey, config::CliConfig, display, error::CliError, tx};

#[derive(Subcommand)]
pub enum AdminCommands {
    /// Transfer admin authority to a new address (irreversible)
    Transfer {
        #[arg(long)]
        market: String,
        #[arg(long)]
        new_admin: String,
        #[arg(long)]
        confirm: bool,
    },
}

pub fn handle(cmd: AdminCommands, config: &CliConfig) -> Result<(), CliError> {
    match cmd {
        AdminCommands::Transfer {
            market,
            new_admin,
            confirm,
        } => {
            let market_pk = parse_pubkey(&market)?;
            let new_admin_pk = parse_pubkey(&new_admin)?;

            display::print_header("Transfer Admin Authority");
            display::print_kv("Market:", &market);
            display::print_kv("Current Admin:", &config.keypair.pubkey().to_string());
            display::print_kv("New Admin:", &new_admin);

            tx::confirm_destructive(
                "This is IRREVERSIBLE. You will lose admin access. Continue?",
                confirm,
            )?;

            let ixs =
                create_transfer_admin_instruction(market_pk, config.keypair.pubkey(), new_admin_pk);

            tx::send_transaction(
                &config.rpc_client,
                ixs,
                &[&config.keypair],
                config.dry_run,
                config.priority_fee,
            )
        }
    }
}
