use clap::Subcommand;
use comfy_table::Cell;
use solana_account_decoder::UiAccountEncoding;
use solana_client::{
    rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig},
    rpc_filter::{Memcmp, RpcFilterType},
};
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey};

use archer_sdk::onchain::MAKER_BOOK_DISCRIMINATOR;

use crate::{commands::market::parse_pubkey, config::CliConfig, display, error::CliError};

#[derive(Subcommand)]
pub enum MakerCommands {
    /// List every MakerBook for a market that holds non-zero deposits
    /// (base_free + base_locked + quote_free + quote_locked > 0).
    ListFunded {
        /// Market pubkey
        #[arg(long)]
        market: String,
    },
}

pub fn handle(cmd: MakerCommands, config: &CliConfig) -> Result<(), CliError> {
    match cmd {
        MakerCommands::ListFunded { market } => {
            let market_pk = parse_pubkey(&market)?;
            let entries = fetch_funded_maker_books(config, &market_pk)?;

            if config.json_output {
                let rows: Vec<_> = entries
                    .iter()
                    .map(|e| {
                        serde_json::json!({
                            "maker_book": e.maker_book.to_string(),
                            "maker": e.maker.to_string(),
                            "base_free": e.base_free,
                            "base_locked": e.base_locked,
                            "quote_free": e.quote_free,
                            "quote_locked": e.quote_locked,
                        })
                    })
                    .collect();
                println!(
                    "{}",
                    serde_json::json!({
                        "market": market,
                        "count": entries.len(),
                        "books": rows,
                    })
                );
                return Ok(());
            }

            display::print_header(&format!("Funded MakerBooks — {} found", entries.len()));
            display::print_kv("Market:", &market);

            if entries.is_empty() {
                println!("  (no funded books)");
                return Ok(());
            }

            let mut table = display::make_table(&[
                "MakerBook",
                "Maker",
                "base_free",
                "base_locked",
                "quote_free",
                "quote_locked",
            ]);
            for e in &entries {
                table.add_row(vec![
                    Cell::new(e.maker_book.to_string()),
                    Cell::new(e.maker.to_string()),
                    Cell::new(e.base_free),
                    Cell::new(e.base_locked),
                    Cell::new(e.quote_free),
                    Cell::new(e.quote_locked),
                ]);
            }
            println!("{table}");

            Ok(())
        }
    }
}

struct FundedBook {
    maker_book: Pubkey,
    maker: Pubkey,
    base_free: u64,
    base_locked: u64,
    quote_free: u64,
    quote_locked: u64,
}

const OFFSET_MAKER: usize = 8;
const OFFSET_QUOTE_LOCKED: usize = 128;
const OFFSET_QUOTE_FREE: usize = 136;
const OFFSET_BASE_LOCKED: usize = 144;
const OFFSET_BASE_FREE: usize = 152;
const MIN_READABLE_LEN: usize = OFFSET_BASE_FREE + 8;

fn fetch_funded_maker_books(
    config: &CliConfig,
    market: &Pubkey,
) -> Result<Vec<FundedBook>, CliError> {
    let program_id = archer_sdk::ARCHER_V1_PROGRAM_ID;

    let filters = vec![
        RpcFilterType::Memcmp(Memcmp::new_raw_bytes(0, MAKER_BOOK_DISCRIMINATOR.to_vec())),
        RpcFilterType::Memcmp(Memcmp::new_raw_bytes(40, market.to_bytes().to_vec())),
    ];

    let accounts_config = RpcProgramAccountsConfig {
        filters: Some(filters),
        account_config: RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::Base64),
            commitment: Some(CommitmentConfig::confirmed()),
            ..Default::default()
        },
        ..Default::default()
    };

    let accounts = config
        .rpc_client
        .get_program_accounts_with_config(&program_id, accounts_config)?;

    let mut entries = Vec::with_capacity(accounts.len());
    for (pubkey, account) in accounts {
        if account.data.len() < MIN_READABLE_LEN {
            continue;
        }

        let maker = Pubkey::try_from(&account.data[OFFSET_MAKER..OFFSET_MAKER + 32])
            .map_err(|_| CliError::Deserialization("maker pubkey bytes".into()))?;
        let quote_locked = read_u64_le(&account.data, OFFSET_QUOTE_LOCKED);
        let quote_free = read_u64_le(&account.data, OFFSET_QUOTE_FREE);
        let base_locked = read_u64_le(&account.data, OFFSET_BASE_LOCKED);
        let base_free = read_u64_le(&account.data, OFFSET_BASE_FREE);

        if quote_locked + quote_free + base_locked + base_free == 0 {
            continue;
        }

        entries.push(FundedBook {
            maker_book: pubkey,
            maker,
            base_free,
            base_locked,
            quote_free,
            quote_locked,
        });
    }

    entries.sort_by(|a, b| b.quote_free.cmp(&a.quote_free));
    Ok(entries)
}

fn read_u64_le(data: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap())
}
