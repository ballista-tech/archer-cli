//! Market creation and inspection.
//!

use clap::Subcommand;
use solana_account_decoder::UiAccountEncoding;
use solana_client::{
    rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig},
    rpc_filter::{Memcmp, RpcFilterType},
};
use solana_sdk::{
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};
use std::str::FromStr;

use archer_sdk::accounts::parse_market_state;
use archer_sdk::ix_builder::market::build_initialize_market_ix;
use archer_sdk::onchain::{
    InitializeMarketParams, MarketStateHeader, MarketStatus, MARKET_STATE_DISCRIMINATOR,
    PERMISSIONLESS_MAKER_FEE_PPM, PERMISSIONLESS_TAKER_FEE_PPM,
};

use crate::{
    config::CliConfig,
    display,
    error::CliError,
    tx,
    utils::{analyze_market, print_analysis},
};

#[derive(Subcommand)]
pub enum MarketCommands {
    /// List markets on the program. Shows only Active markets by default.
    List {
        /// Include Paused and Closed markets as well
        #[arg(long)]
        all: bool,
    },

    Init {
        #[arg(long)]
        base_mint: String,
        #[arg(long)]
        quote_mint: String,
        #[arg(long, default_value = "")]
        base_token_program: String,
        #[arg(long, default_value = "")]
        quote_token_program: String,
        #[arg(long)]
        tick_size: u64,
        #[arg(long)]
        base_lot_size: u64,
        #[arg(long)]
        quote_lot_size: u64,
        #[arg(long)]
        raw_base_units_per_base_unit: u64,
        #[arg(long)]
        base_decimals: u8,
        #[arg(long)]
        quote_decimals: u8,
        /// Current price of base asset in quote token (e.g. 148.50 for SOL/USDC).
        /// Used for human-readable analysis only. Does NOT affect on-chain state.
        #[arg(long)]
        price: Option<f64>,
        /// Skip confirmation prompt
        #[arg(long)]
        confirm: bool,
    },

}

pub fn handle(cmd: MarketCommands, config: &CliConfig) -> Result<(), CliError> {
    match cmd {
        MarketCommands::List { all } => {
            let markets = fetch_markets(config)?;

            let shown: Vec<_> = markets
                .iter()
                .filter(|(_, h)| all || h.get_status() == Ok(MarketStatus::Active))
                .collect();

            display::print_header(if all { "All Markets" } else { "Active Markets" });

            if shown.is_empty() {
                println!("  No markets found.");
                return Ok(());
            }

            let mut table = display::make_table(&[
                "Market", "Base mint", "Quote mint", "Status", "Maker", "Taker",
            ]);
            for (pubkey, h) in &shown {
                table.add_row(vec![
                    pubkey.to_string(),
                    h.base_mint.to_string(),
                    h.quote_mint.to_string(),
                    display::format_status(h.status).to_string(),
                    format!("{} ppm", h.maker_fee_ppm),
                    format!("{} ppm", h.taker_fee_ppm),
                ]);
            }
            println!("{table}");

            let hidden = markets.len() - shown.len();
            if hidden > 0 {
                println!();
                println!("  {hidden} inactive market(s) hidden. Pass --all to include them.");
            }
            Ok(())
        }

        MarketCommands::Init {
            base_mint,
            quote_mint,
            base_token_program,
            quote_token_program,
            tick_size,
            base_lot_size,
            quote_lot_size,
            raw_base_units_per_base_unit,
            base_decimals,
            quote_decimals,
            price,
            confirm,
        } => {
            // Not a choice: the program requires exactly this of every creator
            // that is not the global authority.
            let maker_fee_ppm = PERMISSIONLESS_MAKER_FEE_PPM;
            let taker_fee_ppm = PERMISSIONLESS_TAKER_FEE_PPM;

            let market_keypair = Keypair::new();
            let market_id = market_keypair.pubkey();

            let analysis = analyze_market(
                tick_size,
                base_lot_size,
                quote_lot_size,
                raw_base_units_per_base_unit,
                base_decimals,
                quote_decimals,
                maker_fee_ppm,
                taker_fee_ppm,
                price,
            );

            display::print_header("Initialize Market");
            display::print_kv("Ephemeral Market ID:", &market_id.to_string());
            display::print_kv("Base Mint:", &base_mint);
            display::print_kv("Quote Mint:", &quote_mint);
            display::print_kv("Base Decimals:", &base_decimals.to_string());
            display::print_kv("Quote Decimals:", &quote_decimals.to_string());
            display::print_kv("Base Lot Size:", &format!("{base_lot_size} atoms"));
            display::print_kv("Quote Lot Size:", &format!("{quote_lot_size} atoms"));
            display::print_kv("Tick Size:", &format!("{tick_size} quote atoms/base unit"));
            display::print_kv(
                "Raw Base Units/Unit:",
                &raw_base_units_per_base_unit.to_string(),
            );
            display::print_kv("Admin:", &config.keypair.pubkey().to_string());
            display::print_kv(
                "Maker fee PPM",
                format!("{maker_fee_ppm} PPM (fixed by protocol)").as_str(),
            );
            display::print_kv(
                "Taker fee PPM",
                format!("{taker_fee_ppm} PPM (fixed by protocol)").as_str(),
            );
            print_analysis(&analysis, price);

            if !analysis.errors.is_empty() {
                println!();
                eprintln!(
                    "  ✗ {} error(s) found. Cannot initialize with these parameters.",
                    analysis.errors.len()
                );
                return Err(CliError::InvalidInput(
                    "market parameters have fatal errors — see above".into(),
                ));
            }

            if price.is_none() {
                println!();
                println!("  Tip: add --price <current_price> for full tick/lot/fee analysis.");
            }

            tx::confirm_destructive("Proceed with market initialization?", confirm)?;

            let base_mint_key = parse_pubkey(&base_mint)?;
            let quote_mint_key = parse_pubkey(&quote_mint)?;
            let base_token_program_key = parse_pubkey(&base_token_program)?;
            let quote_token_program_key = parse_pubkey(&quote_token_program)?;

            let params = InitializeMarketParams {
                market_id,
                base_mint: base_mint_key,
                base_token_program: base_token_program_key,
                quote_mint: quote_mint_key,
                quote_token_program: quote_token_program_key,
                base_atoms_per_base_lot: base_lot_size,
                quote_atoms_per_quote_lot: quote_lot_size,
                tick_size_in_quote_atoms_per_base_unit: tick_size,
                raw_base_units_per_base_unit,
                base_decimals,
                quote_decimals,
                maker_fee_ppm,
                taker_fee_ppm,
            };

            // The SDK builder applies every rule the program applies at
            // `InitializeMarket`, so bad parameters fail here rather than as a
            // paid-for transaction. `analyze_market` above is complementary —
            // economic advice, not the program's rules.
            let ix = build_initialize_market_ix(
                params,
                &config.keypair.pubkey(),
                &config.keypair.pubkey(),
            )
            .map_err(|e| CliError::InvalidInput(e.to_string()))?;

            tx::send_transaction(
                &config.rpc_client,
                vec![ix],
                &[&config.keypair],
                config.dry_run,
                config.priority_fee,
            )?;

            let market_pda = archer_sdk::pda::derive_market(&market_id);
            let base_vault = MarketStateHeader::get_vault_ata_address(
                &market_pda.0,
                &base_mint_key,
                &base_token_program_key,
            );
            let quote_vault = MarketStateHeader::get_vault_ata_address(
                &market_pda.0,
                &quote_mint_key,
                &quote_token_program_key,
            );

            if !config.dry_run {
                println!();
                println!("  Market: {:?}", market_pda.0.to_string());
                println!("  Base vault: {:?}", base_vault.to_string());
                println!("  Quote vault: {:?}", quote_vault.to_string());
                println!("  Save this — these are your permanent market addresses.");
            }

            Ok(())
        }

    }
}

pub fn parse_pubkey(s: &str) -> Result<Pubkey, CliError> {
    Pubkey::from_str(s).map_err(|e| CliError::InvalidInput(format!("invalid pubkey '{s}': {e}")))
}

/// Every market account the program owns, matched on the state discriminator.
///
/// `getProgramAccounts` with a memcmp filter, so the RPC does the filtering
/// rather than shipping the whole program's accounts over the wire. Accounts
/// that fail to decode are skipped rather than failing the listing — one
/// malformed account should not hide every healthy market.
fn fetch_markets(config: &CliConfig) -> Result<Vec<(Pubkey, MarketStateHeader)>, CliError> {
    let accounts_config = RpcProgramAccountsConfig {
        filters: Some(vec![RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
            0,
            MARKET_STATE_DISCRIMINATOR.to_vec(),
        ))]),
        account_config: RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::Base64),
            commitment: Some(CommitmentConfig::confirmed()),
            ..Default::default()
        },
        ..Default::default()
    };

    let accounts = config
        .rpc_client
        .get_program_accounts_with_config(&archer_sdk::ARCHER_V1_PROGRAM_ID, accounts_config)?;

    let mut markets = Vec::with_capacity(accounts.len());
    for (pubkey, account) in accounts {
        if let Ok(header) = parse_market_state(&account.data) {
            markets.push((pubkey, *header));
        }
    }
    markets.sort_by_key(|(pubkey, _)| *pubkey);
    Ok(markets)
}
