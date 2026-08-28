//! Fee collection.
//!

use clap::Subcommand;
use solana_sdk::signature::Signer;

use archer_sdk::onchain::builders::create_collect_protocol_fee_instruction;
use archer_sdk::onchain::{CollectProtocolFeeParams, MarketStateHeader, ARCHER_EXCHANGE_TREASURY};
use spl_associated_token_account::get_associated_token_address_with_program_id;
use spl_associated_token_account::instruction::create_associated_token_account_idempotent;

use crate::{commands::market::parse_pubkey, config::CliConfig, display, error::CliError, tx};

#[derive(Subcommand)]
pub enum FeeCommands {
    Collect {
        #[arg(long)]
        market: String,
        #[arg(long)]
        amount: u64,
    },
}

pub fn handle(cmd: FeeCommands, config: &CliConfig) -> Result<(), CliError> {
    match cmd {
        FeeCommands::Collect { market, amount } => {
            let market_pk = parse_pubkey(&market)?;
            let admin_pk = config.keypair.pubkey();

            let account = config
                .rpc_client
                .get_account(&market_pk)
                .map_err(|_| CliError::AccountNotFound(market.clone()))?;

            let header = load_market_header(&account.data)?;
            let quote_mint = header.quote_mint;
            let quote_vault = header.quote_vault;

            let token_mint_account = config
                .rpc_client
                .get_account(&quote_mint)
                .map_err(|_| CliError::AccountNotFound(market.clone()))?;

            let quote_token_program = token_mint_account.owner;

            display::print_header("Collect Protocol Fees");
            display::print_kv("Market:", &market);
            display::print_kv("Amount (quote lots):", &amount.to_string());
            display::print_kv("Destination:", &admin_pk.to_string());

            let admin_token_account = get_associated_token_address_with_program_id(
                &admin_pk,
                &quote_mint,
                &quote_token_program,
            );

            let treasury_quote_token_account = get_associated_token_address_with_program_id(
                &ARCHER_EXCHANGE_TREASURY,
                &quote_mint,
                &quote_token_program,
            );

            let admin_fee_ata_ix = create_associated_token_account_idempotent(
                &admin_pk,
                &admin_pk,
                &quote_mint,
                &quote_token_program,
            );
            let archer_treasury_fee_ata_ix = create_associated_token_account_idempotent(
                &admin_pk,
                &ARCHER_EXCHANGE_TREASURY,
                &quote_mint,
                &quote_token_program,
            );

            let params = CollectProtocolFeeParams { amount };
            let ix = create_collect_protocol_fee_instruction(
                params,
                market_pk,
                admin_pk,
                quote_mint,
                quote_vault,
                admin_token_account,
                ARCHER_EXCHANGE_TREASURY,
                treasury_quote_token_account,
                quote_token_program,
            );

            tx::send_transaction(
                &config.rpc_client,
                vec![admin_fee_ata_ix, archer_treasury_fee_ata_ix, ix],
                &[&config.keypair],
                config.dry_run,
                config.priority_fee,
            )
        }
    }
}

pub fn load_market_header(data: &[u8]) -> Result<MarketStateHeader, CliError> {
    if data.len() < std::mem::size_of::<MarketStateHeader>() {
        return Err(CliError::Deserialization(
            "account data too small for MarketStateHeader".into(),
        ));
    }
    let header = unsafe { std::ptr::read_unaligned(data.as_ptr() as *const MarketStateHeader) };
    Ok(header)
}
