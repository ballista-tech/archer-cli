use clap::Subcommand;
use solana_sdk::{pubkey::Pubkey, signature::Signer};

use archer_sdk::ix_builder::market as build;
use archer_sdk::onchain::{is_global_authority, MarketStatus, ARCHER_GLOBAL_AUTHORITY};

use crate::commands::market::parse_pubkey;
use crate::{display, tx, CliConfig, CliError};

/// `(maker_fee_ppm, taker_fee_ppm)` as the market currently holds them.
///
/// Both fee builders check `maker + taker >= 0`, which needs the side that is
/// not being changed.
fn current_fees(
    config: &CliConfig,
    market: &str,
    market_pk: Pubkey,
) -> Result<(i32, i32), CliError> {
    let account = config
        .rpc_client
        .get_account(&market_pk)
        .map_err(|_| CliError::AccountNotFound(market.to_string()))?;
    let header = crate::commands::fee::load_market_header(&account.data)?;
    Ok((header.maker_fee_ppm, header.taker_fee_ppm))
}

/// Refuse locally when the loaded keypair is not the authority.
///
/// The chain would reject it anyway; failing here costs no fee and names the
/// key that was actually loaded, which is the usual mistake.
fn require_authority(config: &CliConfig, action: &str) -> Result<(), CliError> {
    if is_global_authority(&config.keypair.pubkey()) {
        return Ok(());
    }
    eprintln!(
        "  ✗ {action} requires the global authority ({ARCHER_GLOBAL_AUTHORITY}); signer is {}.",
        config.keypair.pubkey()
    );
    Err(CliError::InvalidInput(
        "signer is not the global authority".into(),
    ))
}

#[derive(Subcommand)]
pub enum AuthorityCommands {
    /// Set a market's maker fee. Market must be paused.
    FeeMaker {
        #[arg(long)]
        market: String,
        /// Fee in parts per million (negative = rebate)
        #[arg(long, allow_hyphen_values = true)]
        fee_ppm: i32,
    },

    /// Set a market's taker fee. Market must be paused.
    FeeTaker {
        #[arg(long)]
        market: String,
        /// Fee in parts per million (negative = rebate)
        #[arg(long, allow_hyphen_values = true)]
        fee_ppm: i32,
    },

    /// Change a market's status: 0 = Active, 1 = Paused, 2 = Closed
    Status {
        #[arg(long)]
        market: String,
        #[arg(long)]
        set: u8,
    },

    /// Create the maker registry for a market (authority pays rent)
    RegistryInit {
        #[arg(long)]
        market: String,
    },

    /// Admit a maker book to a market's registry
    RegistryRegister {
        #[arg(long)]
        market: String,
        #[arg(long)]
        maker_book: String,
    },

    /// Remove a maker book from a market's registry
    RegistryDeregister {
        #[arg(long)]
        market: String,
        #[arg(long)]
        maker_book: String,
    },
}

pub fn handle(cmd: AuthorityCommands, config: &CliConfig) -> Result<(), CliError> {
    match cmd {
        AuthorityCommands::FeeMaker { market, fee_ppm } => {
            require_authority(config, "A fee update")?;
            let market_pk = parse_pubkey(&market)?;

            display::print_header("Set Maker Fee");
            display::print_kv("Market:", &market);
            display::print_kv("New Fee:", &display::format_fee_ppm(fee_ppm));

            let current = current_fees(config, &market, market_pk)?;
            let ix = build::build_update_maker_fee_ix(
                &market_pk,
                &config.keypair.pubkey(),
                fee_ppm,
                current.1,
            )
            .map_err(|e| CliError::InvalidInput(e.to_string()))?;
            send(config, vec![ix])
        }

        AuthorityCommands::FeeTaker { market, fee_ppm } => {
            require_authority(config, "A fee update")?;
            let market_pk = parse_pubkey(&market)?;

            display::print_header("Set Taker Fee");
            display::print_kv("Market:", &market);
            display::print_kv("New Fee:", &display::format_fee_ppm(fee_ppm));

            let current = current_fees(config, &market, market_pk)?;
            let ix = build::build_update_taker_fee_ix(
                &market_pk,
                &config.keypair.pubkey(),
                fee_ppm,
                current.0,
            )
            .map_err(|e| CliError::InvalidInput(e.to_string()))?;
            send(config, vec![ix])
        }

        AuthorityCommands::Status { market, set } => {
            require_authority(config, "A market status change")?;
            let market_pk = parse_pubkey(&market)?;

            display::print_header("Change Market Status");
            display::print_kv("Market:", &market);
            display::print_kv("New Status:", display::format_status(set));

            let status = MarketStatus::from_u8(set).map_err(|_| {
                CliError::InvalidInput(format!(
                    "status must be 0 (Active), 1 (Paused) or 2 (Closed), got {set}"
                ))
            })?;
            let ix =
                build::build_change_market_status_ix(&market_pk, &config.keypair.pubkey(), status);
            send(config, vec![ix])
        }

        AuthorityCommands::RegistryInit { market } => {
            require_authority(config, "Registry creation")?;
            let market_pk = parse_pubkey(&market)?;

            display::print_header("Initialize Maker Registry");
            display::print_kv("Market:", &market);

            let ix =
                build::build_initialize_maker_registry_ix(&market_pk, &config.keypair.pubkey());
            send(config, vec![ix])
        }

        AuthorityCommands::RegistryRegister { market, maker_book } => {
            require_authority(config, "Maker registration")?;
            let (market_pk, book_pk) = pair(&market, &maker_book)?;

            display::print_header("Register Maker");
            display::print_kv("Market:", &market);
            display::print_kv("Maker book:", &maker_book);

            let ix =
                build::build_register_maker_ix(&market_pk, &config.keypair.pubkey(), &book_pk);
            send(config, vec![ix])
        }

        AuthorityCommands::RegistryDeregister { market, maker_book } => {
            require_authority(config, "Maker deregistration")?;
            let (market_pk, book_pk) = pair(&market, &maker_book)?;

            display::print_header("Deregister Maker");
            display::print_kv("Market:", &market);
            display::print_kv("Maker book:", &maker_book);

            let ix =
                build::build_deregister_maker_ix(&market_pk, &config.keypair.pubkey(), &book_pk);
            send(config, vec![ix])
        }
    }
}

fn pair(market: &str, maker_book: &str) -> Result<(Pubkey, Pubkey), CliError> {
    Ok((parse_pubkey(market)?, parse_pubkey(maker_book)?))
}

fn send(
    config: &CliConfig,
    ixs: Vec<solana_sdk::instruction::Instruction>,
) -> Result<(), CliError> {
    tx::send_transaction(
        &config.rpc_client,
        ixs,
        &[&config.keypair],
        config.dry_run,
        config.priority_fee,
    )
}
