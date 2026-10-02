use std::collections::{BTreeMap, HashSet};

use clap::Subcommand;
use comfy_table::{Cell, Color};
use solana_account_decoder::UiAccountEncoding;
use solana_client::{
    rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig},
    rpc_filter::{Memcmp, RpcFilterType},
};
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey};

use archer_sdk::onchain::{
    state::{ArcherAccount, DelegatedPlatform},
    ArcherUnit, MakerBook, MakerRegistry, MAKER_BOOK_DISCRIMINATOR,
};

use crate::{
    commands::{fee::load_market_header, market::parse_pubkey},
    config::CliConfig,
    display,
    error::CliError,
};

#[derive(Subcommand)]
pub enum ObserveCommands {
    Market {
        #[arg(long)]
        market: String,
    },

    MakerBook {
        #[arg(long)]
        maker_book: String,
    },

    ArcherAccount {
        #[arg(long, conflicts_with = "owner")]
        account: Option<String>,

        #[arg(long, requires = "platform")]
        owner: Option<String>,

        #[arg(long)]
        platform: Option<String>,
    },

    Liquidity {
        #[arg(long)]
        market: String,
        /// Bid --> 0, Ask --> 1
        #[arg(long)]
        side: Option<String>,
        #[arg(long, default_value = "10")]
        depth: usize,
    },

    Vaults {
        #[arg(long)]
        market: String,
    },

    Tvl {
        #[arg(long)]
        market: String,
    },
}

pub fn handle(cmd: ObserveCommands, config: &CliConfig) -> Result<(), CliError> {
    match cmd {
        ObserveCommands::Market { market } => show_market(config, &market),
        ObserveCommands::MakerBook { maker_book } => show_maker_book(config, &maker_book),
        ObserveCommands::ArcherAccount {
            account,
            owner,
            platform,
        } => show_archer_account(config, account, owner, platform),
        ObserveCommands::Liquidity {
            market,
            side,
            depth,
        } => show_liquidity(config, &market, side.as_deref(), depth),
        ObserveCommands::Vaults { market } => show_vaults(config, &market),
        ObserveCommands::Tvl { market } => show_tvl(config, &market),
    }
}

fn show_market(config: &CliConfig, market: &str) -> Result<(), CliError> {
    let market_pk = parse_pubkey(market)?;
    let account = config
        .rpc_client
        .get_account(&market_pk)
        .map_err(|_| CliError::AccountNotFound(market.into()))?;

    let h = load_market_header(&account.data)?;

    if config.json_output {
        println!(
            "{}",
            serde_json::json!({
                "market": market,
                "admin": h.admin.to_string(),
                "base_mint": h.base_mint.to_string(),
                "quote_mint": h.quote_mint.to_string(),
                "base_vault": h.base_vault.to_string(),
                "quote_vault": h.quote_vault.to_string(),
                "status": display::format_status(h.status),
                "maker_fee_ppm": h.maker_fee_ppm,
                "taker_fee_ppm": h.taker_fee_ppm,
                "tick_size": h.tick_size_in_quote_atoms_per_base_unit.as_u64(),
                "base_atoms_per_lot": h.base_atoms_per_base_lot.as_u64(),
                "quote_atoms_per_lot": h.quote_atoms_per_quote_lot.as_u64(),
                "base_decimals": h.base_decimals,
                "quote_decimals": h.quote_decimals,
                "uncollected_fees": h.uncollected_fees_quote_lots,
                "collected_fees": h.collected_fees_quote_lots,
            })
        );
        return Ok(());
    }

    display::print_header("Market State");
    display::print_kv("Address:", market);
    display::print_kv("Admin:", &h.admin.to_string());
    display::print_kv("Status:", display::format_status(h.status));
    println!();
    display::print_kv("Base Mint:", &h.base_mint.to_string());
    display::print_kv("Quote Mint:", &h.quote_mint.to_string());
    display::print_kv("Base Decimals:", &h.base_decimals.to_string());
    display::print_kv("Quote Decimals:", &h.quote_decimals.to_string());
    display::print_kv(
        "Base Atoms/Lot:",
        &h.base_atoms_per_base_lot.as_u64().to_string(),
    );
    display::print_kv(
        "Quote Atoms/Lot:",
        &h.quote_atoms_per_quote_lot.as_u64().to_string(),
    );
    display::print_kv(
        "Tick Size:",
        &h.tick_size_in_quote_atoms_per_base_unit
            .as_u64()
            .to_string(),
    );
    display::print_kv(
        "Raw Base Units/Unit:",
        &h.raw_base_units_per_base_unit.to_string(),
    );
    println!();
    display::print_kv("Base Vault:", &h.base_vault.to_string());
    display::print_kv("Quote Vault:", &h.quote_vault.to_string());
    display::print_kv("Maker Fee:", &display::format_fee_ppm(h.maker_fee_ppm));
    display::print_kv("Taker Fee:", &display::format_fee_ppm(h.taker_fee_ppm));
    display::print_kv(
        "Uncollected Fees:",
        &format!("{} quote lots", h.uncollected_fees_quote_lots),
    );
    display::print_kv(
        "Collected Fees:",
        &format!("{} quote lots", h.collected_fees_quote_lots),
    );

    Ok(())
}

fn show_maker_book(config: &CliConfig, addr: &str) -> Result<(), CliError> {
    let pk = parse_pubkey(addr)?;
    let account = config
        .rpc_client
        .get_account(&pk)
        .map_err(|_| CliError::AccountNotFound(addr.into()))?;

    let book = load_maker_book(&account.data)?;

    // The projected quote split reserves quote inclusive of the market's maker
    // fee, so the projection needs the fee from the book's market.
    let market_account = config
        .rpc_client
        .get_account(&book.market)
        .map_err(|_| CliError::AccountNotFound(book.market.to_string()))?;
    let maker_fee_ppm = load_market_header(&market_account.data)?.maker_fee_ppm;

    if config.json_output {
        let bids: Vec<_> = book
            .bid_levels
            .iter()
            .filter(|l| l.is_active())
            .map(|l| {
                serde_json::json!({
                    "size": l.size_in_base_lots.as_u64(),
                    "offset": l.price_offset_ticks,
                    "price": l.absolute_price(book.mid_price_ticks),
                })
            })
            .collect();
        let asks: Vec<_> = book
            .ask_levels
            .iter()
            .filter(|l| l.is_active())
            .map(|l| {
                serde_json::json!({
                    "size": l.size_in_base_lots.as_u64(),
                    "offset": l.price_offset_ticks,
                    "price": l.absolute_price(book.mid_price_ticks),
                })
            })
            .collect();

        // `quote_free` / `quote_locked` are the projected split — the raw account
        // fields lag when a reprice is pending. Both are reported so a stuck book
        // is diagnosable.
        let (projected_quote_locked, projected_quote_free) = book
            .projected_quote_balances(maker_fee_ppm)
            .unwrap_or((book.quote_locked.as_u64(), book.quote_free.as_u64()));

        println!(
            "{}",
            serde_json::json!({
                "address": addr,
                "maker": book.maker.to_string(),
                "market": book.market.to_string(),
                "delegate": book.delegate.to_string(),
                "status": format_book_status(book.status),
                "mid_price_ticks": book.mid_price_ticks,
                "sequence": book.last_updated_sequence_number,
                "last_updated_slot": book.last_updated_slot,
                "expiry_in_slots": book.expiry_in_slots,
                "base_free": book.base_free.as_u64(),
                "base_locked": book.base_locked.as_u64(),
                "quote_free": projected_quote_free,
                "quote_locked": projected_quote_locked,
                "quote_free_raw": book.quote_free.as_u64(),
                "quote_locked_raw": book.quote_locked.as_u64(),
                "mid_at_last_sync": book.mid_at_last_sync,
                "quote_sync_pending": book.mid_at_last_sync != 0
                    && book.mid_at_last_sync != book.mid_price_ticks,
                "quote_sync_unfundable": !book.is_quote_sync_fundable(maker_fee_ppm),
                "bids": bids,
                "asks": asks,
            })
        );
        return Ok(());
    }

    display::print_header("Maker Book");
    display::print_kv("Address:", addr);
    display::print_kv("Maker:", &book.maker.to_string());
    display::print_kv("Market:", &book.market.to_string());
    display::print_kv("Delegate:", &book.delegate.to_string());
    display::print_kv("Status:", format_book_status(book.status));
    display::print_kv("Mid Price (ticks):", &book.mid_price_ticks.to_string());
    display::print_kv(
        "Sequence #:",
        &book.last_updated_sequence_number.to_string(),
    );
    display::print_kv("Last Updated Slot:", &book.last_updated_slot.to_string());
    if book.expiry_in_slots == 0 {
        display::print_kv("Expiry (slots):", "0 (disabled)");
    } else {
        display::print_kv("Expiry (slots):", &book.expiry_in_slots.to_string());
    }
    println!();
    display::print_kv(
        "Base (free/locked):",
        &format!(
            "{} / {}",
            book.base_free.as_u64(),
            book.base_locked.as_u64()
        ),
    );

    match book.projected_quote_balances(maker_fee_ppm) {
        Ok((locked, free)) => {
            display::print_kv("Quote (free/locked):", &format!("{} / {}", free, locked));
            if book.mid_at_last_sync != 0 && book.mid_at_last_sync != book.mid_price_ticks {
                display::print_kv(
                    "  pending reprice:",
                    &format!(
                        "anchored at {} (raw {} / {})",
                        book.mid_at_last_sync,
                        book.quote_free.as_u64(),
                        book.quote_locked.as_u64()
                    ),
                );
            }
        }
        Err(_) => {
            display::print_kv(
                "Quote (free/locked):",
                &format!(
                    "{} / {} (raw)",
                    book.quote_free.as_u64(),
                    book.quote_locked.as_u64()
                ),
            );
            display::print_kv(
                "  pending reprice:",
                &format!(
                    "UNFUNDABLE — anchored at {}, book is skipped in auctions",
                    book.mid_at_last_sync
                ),
            );
        }
    }

    println!();
    println!("  Bid Levels:");
    let mut table = display::make_table(&["Level", "Price (ticks)", "Offset", "Size (base lots)"]);
    for (i, level) in book.bid_levels.iter().enumerate() {
        if !level.is_active() {
            continue;
        }
        let price = level
            .absolute_price(book.mid_price_ticks)
            .map(|p| p.to_string())
            .unwrap_or("N/A".into());
        table.add_row(vec![
            Cell::new(i).fg(Color::Green),
            Cell::new(&price).fg(Color::Green),
            Cell::new(level.price_offset_ticks).fg(Color::Green),
            Cell::new(level.size_in_base_lots.as_u64()).fg(Color::Green),
        ]);
    }
    println!("{table}");

    println!();
    println!("  Ask Levels:");
    let mut table = display::make_table(&["Level", "Price (ticks)", "Offset", "Size (base lots)"]);
    for (i, level) in book.ask_levels.iter().enumerate() {
        if !level.is_active() {
            continue;
        }
        let price = level
            .absolute_price(book.mid_price_ticks)
            .map(|p| p.to_string())
            .unwrap_or("N/A".into());
        table.add_row(vec![
            Cell::new(i).fg(Color::Red),
            Cell::new(&price).fg(Color::Red),
            Cell::new(level.price_offset_ticks).fg(Color::Red),
            Cell::new(level.size_in_base_lots.as_u64()).fg(Color::Red),
        ]);
    }
    println!("{table}");

    Ok(())
}

fn show_liquidity(
    config: &CliConfig,
    market: &str,
    side_filter: Option<&str>,
    depth: usize,
) -> Result<(), CliError> {
    let market_pk = parse_pubkey(market)?;
    let books = fetch_all_maker_books(config, &market_pk)?;

    let show_bids = side_filter.map_or(true, |s| s == "bid");
    let show_asks = side_filter.map_or(true, |s| s == "ask");

    let mut bid_agg: BTreeMap<u64, (u64, u32)> = BTreeMap::new();
    let mut ask_agg: BTreeMap<u64, (u64, u32)> = BTreeMap::new();

    let active_count = books.iter().filter(|b| b.status == 1).count();

    for book in &books {
        if book.status != 1 {
            continue;
        }

        if show_bids {
            for level in &book.bid_levels {
                if !level.is_active() {
                    continue;
                }
                if let Some(price) = level.absolute_price(book.mid_price_ticks) {
                    let entry = bid_agg.entry(price).or_insert((0, 0));
                    entry.0 = entry.0.saturating_add(level.size_in_base_lots.as_u64());
                    entry.1 += 1;
                }
            }
        }

        if show_asks {
            for level in &book.ask_levels {
                if !level.is_active() {
                    continue;
                }
                if let Some(price) = level.absolute_price(book.mid_price_ticks) {
                    let entry = ask_agg.entry(price).or_insert((0, 0));
                    entry.0 = entry.0.saturating_add(level.size_in_base_lots.as_u64());
                    entry.1 += 1;
                }
            }
        }
    }

    if config.json_output {
        let bids: Vec<_> = bid_agg
            .iter()
            .rev()
            .take(depth)
            .map(|(p, (s, m))| serde_json::json!({"price": p, "size": s, "makers": m}))
            .collect();
        let asks: Vec<_> = ask_agg
            .iter()
            .take(depth)
            .map(|(p, (s, m))| serde_json::json!({"price": p, "size": s, "makers": m}))
            .collect();

        println!(
            "{}",
            serde_json::json!({
                "market": market,
                "active_makers": active_count,
                "total_makers": books.len(),
                "bids": bids,
                "asks": asks,
            })
        );
        return Ok(());
    }

    display::print_header(&format!(
        "Aggregated Book ({} active makers, {} total)",
        active_count,
        books.len()
    ));

    if show_asks {
        let ask_levels: Vec<_> = ask_agg.iter().take(depth).collect();
        let mut table =
            display::make_table(&["Side", "Price (ticks)", "Size (base lots)", "Makers"]);
        for (price, (size, makers)) in ask_levels.iter().rev() {
            table.add_row(vec![
                Cell::new("ASK").fg(Color::Red),
                Cell::new(price).fg(Color::Red),
                Cell::new(size).fg(Color::Red),
                Cell::new(makers).fg(Color::Red),
            ]);
        }
        println!("{table}");
    }

    let best_bid = bid_agg.keys().next_back();
    let best_ask = ask_agg.keys().next();
    if let (Some(&bb), Some(&ba)) = (best_bid, best_ask) {
        let spread = ba.saturating_sub(bb);
        let spread_pct = if ba > 0 {
            (spread as f64 / ba as f64) * 100.0
        } else {
            0.0
        };
        println!("  ═══ Spread: {} ticks ({:.4}%) ═══", spread, spread_pct);
    } else {
        println!("  ═══ No spread (one side empty) ═══");
    }

    if show_bids {
        let mut table =
            display::make_table(&["Side", "Price (ticks)", "Size (base lots)", "Makers"]);
        for (price, (size, makers)) in bid_agg.iter().rev().take(depth) {
            table.add_row(vec![
                Cell::new("BID").fg(Color::Green),
                Cell::new(price).fg(Color::Green),
                Cell::new(size).fg(Color::Green),
                Cell::new(makers).fg(Color::Green),
            ]);
        }
        println!("{table}");
    }

    let total_bid_size: u64 = bid_agg.values().map(|(s, _)| s).sum();
    let total_ask_size: u64 = ask_agg.values().map(|(s, _)| s).sum();
    println!();
    display::print_kv("Total Bid Size:", &format!("{} base lots", total_bid_size));
    display::print_kv("Total Ask Size:", &format!("{} base lots", total_ask_size));

    Ok(())
}

fn show_vaults(config: &CliConfig, market: &str) -> Result<(), CliError> {
    let market_pk = parse_pubkey(market)?;
    let account = config
        .rpc_client
        .get_account(&market_pk)
        .map_err(|_| CliError::AccountNotFound(market.into()))?;

    let h = load_market_header(&account.data)?;

    let base_vault_ata =
        spl_associated_token_account::get_associated_token_address(&h.base_vault, &h.base_mint);
    let quote_vault_ata =
        spl_associated_token_account::get_associated_token_address(&h.quote_vault, &h.quote_mint);

    let base_balance = get_token_balance(config, &base_vault_ata);
    let quote_balance = get_token_balance(config, &quote_vault_ata);

    let books = fetch_all_maker_books(config, &market_pk)?;
    let mut total_base_lots: u64 = 0;
    let mut total_quote_lots: u64 = 0;

    for book in &books {
        total_base_lots = total_base_lots
            .saturating_add(book.base_free.as_u64())
            .saturating_add(book.base_locked.as_u64());
        total_quote_lots = total_quote_lots
            .saturating_add(book.quote_free.as_u64())
            .saturating_add(book.quote_locked.as_u64());
    }

    if config.json_output {
        println!(
            "{}",
            serde_json::json!({
                "market": market,
                "base_vault": h.base_vault.to_string(),
                "quote_vault": h.quote_vault.to_string(),
                "base_vault_ata": base_vault_ata.to_string(),
                "quote_vault_ata": quote_vault_ata.to_string(),
                "base_vault_balance": base_balance,
                "quote_vault_balance": quote_balance,
                "maker_books_count": books.len(),
                "total_base_lots_in_books": total_base_lots,
                "total_quote_lots_in_books": total_quote_lots,
                "uncollected_protocol_fees": h.uncollected_fees_quote_lots,
            })
        );
        return Ok(());
    }

    display::print_header("Vault Balances");
    display::print_kv("Base Vault:", &h.base_vault.to_string());
    display::print_kv("  ATA:", &base_vault_ata.to_string());
    display::print_kv(
        "  On-chain Balance:",
        &format!("{} atoms", base_balance.unwrap_or(0)),
    );
    display::print_kv("  Sum(maker base lots):", &total_base_lots.to_string());
    println!();
    display::print_kv("Quote Vault:", &h.quote_vault.to_string());
    display::print_kv("  ATA:", &quote_vault_ata.to_string());
    display::print_kv(
        "  On-chain Balance:",
        &format!("{} atoms", quote_balance.unwrap_or(0)),
    );
    display::print_kv("  Sum(maker quote lots):", &total_quote_lots.to_string());
    display::print_kv(
        "  Uncollected Protocol Fees:",
        &format!("{} quote lots", h.uncollected_fees_quote_lots),
    );
    println!();
    display::print_kv("Maker Books:", &books.len().to_string());

    Ok(())
}

fn load_maker_book(data: &[u8]) -> Result<MakerBook, CliError> {
    if data.len() < MakerBook::LEN {
        return Err(CliError::Deserialization(
            "account data too small for MakerBook".into(),
        ));
    }
    let book: &MakerBook = bytemuck::try_from_bytes(&data[..MakerBook::LEN])
        .map_err(|e| CliError::Deserialization(format!("MakerBook: {e}")))?;

    if &book.discriminator != MAKER_BOOK_DISCRIMINATOR {
        return Err(CliError::Deserialization(
            "invalid MakerBook discriminator".into(),
        ));
    }

    Ok(*book)
}

fn fetch_all_maker_books(config: &CliConfig, market: &Pubkey) -> Result<Vec<MakerBook>, CliError> {
    Ok(fetch_all_maker_books_with_addr(config, market)?
        .into_iter()
        .map(|(_, book)| book)
        .collect())
}

fn fetch_all_maker_books_with_addr(
    config: &CliConfig,
    market: &Pubkey,
) -> Result<Vec<(Pubkey, MakerBook)>, CliError> {
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

    let mut books = Vec::with_capacity(accounts.len());
    for (pubkey, account) in accounts {
        if let Ok(book) = load_maker_book(&account.data) {
            books.push((pubkey, book));
        }
    }

    Ok(books)
}

fn get_token_balance(config: &CliConfig, token_account: &Pubkey) -> Option<u64> {
    config
        .rpc_client
        .get_token_account_balance(token_account)
        .ok()
        .and_then(|b| b.amount.parse::<u64>().ok())
}

fn format_book_status(status: u8) -> &'static str {
    match status {
        1 => "Active",
        2 => "Suspended",
        _ => "Unknown",
    }
}

fn load_registered_set(
    config: &CliConfig,
    market: &Pubkey,
) -> Result<Option<HashSet<Pubkey>>, CliError> {
    let (registry_pk, _) = archer_sdk::pda::derive_maker_registry(market);
    let account = match config.rpc_client.get_account(&registry_pk) {
        Ok(acc) => acc,
        Err(_) => return Ok(None),
    };

    let registry = MakerRegistry::load(&account.data)
        .map_err(|e| CliError::Deserialization(format!("MakerRegistry: {e:?}")))?;

    let num = registry.num_makers as usize;
    let set: HashSet<Pubkey> = registry.makers[..num].iter().copied().collect();
    Ok(Some(set))
}

fn show_tvl(config: &CliConfig, market: &str) -> Result<(), CliError> {
    let market_pk = parse_pubkey(market)?;

    let market_account = config
        .rpc_client
        .get_account(&market_pk)
        .map_err(|_| CliError::AccountNotFound(market.into()))?;
    let header = load_market_header(&market_account.data)?;

    let books = fetch_all_maker_books_with_addr(config, &market_pk)?;
    let registered = load_registered_set(config, &market_pk)?;

    let base_atoms_per_lot = header.base_atoms_per_base_lot.as_u64();
    let quote_atoms_per_lot = header.quote_atoms_per_quote_lot.as_u64();
    let base_decimals = header.base_decimals;
    let quote_decimals = header.quote_decimals;

    let mut rows: Vec<TvlRow> = books
        .iter()
        .map(|(addr, book)| {
            let base_lots = book
                .base_free
                .as_u64()
                .saturating_add(book.base_locked.as_u64());
            let quote_lots = book
                .quote_free
                .as_u64()
                .saturating_add(book.quote_locked.as_u64());
            let base_atoms = base_lots.saturating_mul(base_atoms_per_lot);
            let quote_atoms = quote_lots.saturating_mul(quote_atoms_per_lot);
            let is_registered = registered.as_ref().map(|s| s.contains(addr));
            TvlRow {
                maker_book: *addr,
                maker: book.maker,
                status: book.status,
                base_lots,
                quote_lots,
                base_atoms,
                quote_atoms,
                is_registered,
            }
        })
        .collect();

    rows.sort_by(|a, b| b.quote_atoms.cmp(&a.quote_atoms));

    let total_base_atoms: u128 = rows.iter().map(|r| r.base_atoms as u128).sum();
    let total_quote_atoms: u128 = rows.iter().map(|r| r.quote_atoms as u128).sum();
    let registered_count = rows
        .iter()
        .filter(|r| r.is_registered == Some(true))
        .count();
    let unregistered_count = rows
        .iter()
        .filter(|r| r.is_registered == Some(false))
        .count();

    if config.json_output {
        let entries: Vec<_> = rows
            .iter()
            .map(|r| {
                serde_json::json!({
                    "maker_book": r.maker_book.to_string(),
                    "maker": r.maker.to_string(),
                    "status": format_book_status(r.status),
                    "base_lots": r.base_lots,
                    "quote_lots": r.quote_lots,
                    "base_atoms": r.base_atoms,
                    "quote_atoms": r.quote_atoms,
                    "registered": r.is_registered,
                })
            })
            .collect();

        println!(
            "{}",
            serde_json::json!({
                "market": market,
                "registry_exists": registered.is_some(),
                "maker_books": rows.len(),
                "registered_count": registered_count,
                "unregistered_count": unregistered_count,
                "total_base_atoms": total_base_atoms.to_string(),
                "total_quote_atoms": total_quote_atoms.to_string(),
                "base_decimals": base_decimals,
                "quote_decimals": quote_decimals,
                "books": entries,
            })
        );
        return Ok(());
    }

    display::print_header(&format!("Maker Book TVL ({} books)", rows.len()));
    display::print_kv("Market:", market);
    match &registered {
        Some(_) => display::print_kv(
            "Registry:",
            &format!(
                "present ({} registered, {} unregistered)",
                registered_count, unregistered_count
            ),
        ),
        None => display::print_kv("Registry:", "NOT INITIALIZED"),
    }
    println!();

    let mut table = display::make_table(&[
        "Maker Book",
        "Maker",
        "Status",
        "Base (ui)",
        "Quote (ui)",
        "Registered",
    ]);

    for r in &rows {
        let base_ui = format_ui_amount(r.base_atoms, base_decimals);
        let quote_ui = format_ui_amount(r.quote_atoms, quote_decimals);
        let (reg_text, reg_color) = match r.is_registered {
            Some(true) => ("YES", Color::Green),
            Some(false) => ("NO", Color::Red),
            None => ("—", Color::DarkGrey),
        };
        let status_color = match r.status {
            1 => Color::Green,
            2 => Color::Yellow,
            _ => Color::DarkGrey,
        };
        table.add_row(vec![
            Cell::new(r.maker_book.to_string()),
            Cell::new(r.maker.to_string()),
            Cell::new(format_book_status(r.status)).fg(status_color),
            Cell::new(base_ui),
            Cell::new(quote_ui),
            Cell::new(reg_text).fg(reg_color),
        ]);
    }
    println!("{table}");

    println!();
    display::print_kv(
        "Total Base:",
        &format!(
            "{} atoms ({})",
            total_base_atoms,
            format_ui_amount_u128(total_base_atoms, base_decimals)
        ),
    );
    display::print_kv(
        "Total Quote:",
        &format!(
            "{} atoms ({})",
            total_quote_atoms,
            format_ui_amount_u128(total_quote_atoms, quote_decimals)
        ),
    );

    if registered.is_none() {
        println!();
        println!(
            "  Note: registry PDA not found for this market — run `registry init --market {}`",
            market
        );
    }

    Ok(())
}

struct TvlRow {
    maker_book: Pubkey,
    maker: Pubkey,
    status: u8,
    base_lots: u64,
    quote_lots: u64,
    base_atoms: u64,
    quote_atoms: u64,
    is_registered: Option<bool>,
}

fn format_ui_amount(atoms: u64, decimals: u8) -> String {
    format_ui_amount_u128(atoms as u128, decimals)
}

fn format_ui_amount_u128(atoms: u128, decimals: u8) -> String {
    if decimals == 0 {
        return atoms.to_string();
    }
    let divisor = 10u128.pow(decimals as u32);
    let whole = atoms / divisor;
    let frac = atoms % divisor;
    let frac_str = format!("{:0>width$}", frac, width = decimals as usize);
    let trimmed = frac_str.trim_end_matches('0');
    if trimmed.is_empty() {
        whole.to_string()
    } else {
        format!("{}.{}", whole, trimmed)
    }
}

fn parse_platform(s: &str) -> Result<DelegatedPlatform, CliError> {
    match s.to_ascii_lowercase().as_str() {
        "self" | "selfmanaged" | "self_managed" => Ok(DelegatedPlatform::SelfManaged),
        "treadfi" | "tread" => Ok(DelegatedPlatform::TreadFi),
        other => Err(CliError::InvalidInput(format!(
            "unknown platform {other:?} — expected one of: self, treadfi"
        ))),
    }
}

fn show_archer_account(
    config: &CliConfig,
    account: Option<String>,
    owner: Option<String>,
    platform: Option<String>,
) -> Result<(), CliError> {
    let (pk, derived_from) = match (account, owner, platform) {
        (Some(a), _, _) => (parse_pubkey(&a)?, None),
        (None, Some(o), Some(p)) => {
            let owner_pk = parse_pubkey(&o)?;
            let plat = parse_platform(&p)?;
            let (pk, _) = archer_sdk::pda::derive_archer_account(&owner_pk, plat);
            (pk, Some((owner_pk, plat)))
        }
        _ => {
            return Err(CliError::InvalidInput(
                "pass --account, or --owner together with --platform".into(),
            ))
        }
    };

    let raw = config
        .rpc_client
        .get_account(&pk)
        .map_err(|_| CliError::AccountNotFound(pk.to_string()))?;

    let acct = ArcherAccount::load(&raw.data)
        .map_err(|_| CliError::InvalidInput(format!("{pk} is not an ArcherAccount")))?;

    let plat = acct
        .get_platform()
        .map_err(|_| CliError::InvalidInput("unknown platform discriminant".into()))?;

    let rent_floor = config
        .rpc_client
        .get_minimum_balance_for_rent_exemption(ArcherAccount::LEN)
        .unwrap_or(0);
    let spendable_lamports = raw.lamports.saturating_sub(rent_floor);

    let delegate = if acct.is_delegate_set() {
        acct.delegate.to_string()
    } else {
        "none (revoked or never set)".to_string()
    };

    if config.json_output {
        println!(
            "{}",
            serde_json::json!({
                "address": pk.to_string(),
                "owner": acct.owner.to_string(),
                "delegate": acct.delegate.to_string(),
                "delegate_set": acct.is_delegate_set(),
                "platform": format!("{plat:?}"),
                "max_builder_fee_ppm": acct.max_builder_fee_ppm,
                "bump": acct.bump,
                "lamports": raw.lamports,
                "rent_exempt_minimum": rent_floor,
                "lamports_available_for_book_rent": spendable_lamports,
            })
        );
    } else {
        println!("ArcherAccount {pk}");
        if let Some((o, p)) = derived_from {
            println!("  derived from  {o} / {p:?}");
        }
        println!("  owner         {}", acct.owner);
        println!("  delegate      {delegate}");
        println!("  platform      {plat:?}");
        println!(
            "  builder fee   max {} ppm{}",
            acct.max_builder_fee_ppm,
            if acct.max_builder_fee_ppm == 0 {
                "  (no builder fee permitted)"
            } else {
                ""
            }
        );
        println!(
            "  lamports      {} ({} above rent-exempt, available for book rent)",
            raw.lamports, spendable_lamports
        );
        if spendable_lamports == 0 {
            println!("                \u{26a0} cannot fund a new MakerBook at this balance");
        }
    }

    Ok(())
}
