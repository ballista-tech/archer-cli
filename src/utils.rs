use crate::display;

pub struct MarketAnalysis {
    pub num: u128,
    pub den: u128,
    pub ratio: u128,
    pub ratio_clean: bool,

    pub base_lot_tokens: f64,
    pub base_lot_notional: Option<f64>,
    pub quote_lot_human: f64,
    pub lots_per_token: f64,

    pub price_ticks: Option<u64>,
    pub tick_human_price: Option<f64>,
    pub bps_per_tick: Option<f64>,

    pub fee_solvency_ok: bool,
    pub net_protocol_ppm: i32,
    pub maker_fee_bps: f64,
    pub taker_fee_bps: f64,
    pub min_trade_for_taker_fee: Option<f64>,
    pub min_trade_for_maker_fee: Option<f64>,

    pub one_token_quote_check: Option<f64>,

    pub max_safe_tokens: Option<f64>,
    pub max_safe_notional: Option<f64>,

    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

pub fn analyze_market(
    tick_size: u64,
    base_lot_size: u64,
    quote_lot_size: u64,
    raw_base_units: u64,
    base_decimals: u8,
    quote_decimals: u8,
    maker_fee_ppm: i32,
    taker_fee_ppm: i32,
    price: Option<f64>,
) -> MarketAnalysis {
    let mut warnings = Vec::new();
    let mut errors = Vec::new();

    let num = (tick_size as u128).saturating_mul(base_lot_size as u128);

    let base_unit_atoms = 10u128.pow(base_decimals as u32);
    let base_atoms_per_base_unit = base_unit_atoms.saturating_mul(raw_base_units as u128);
    let den = base_atoms_per_base_unit.saturating_mul(quote_lot_size as u128);

    let ratio_clean = den > 0 && num % den == 0;
    let ratio = if den > 0 { num / den } else { 0 };

    if !ratio_clean {
        errors.push(format!(
            "Tick conversion ratio is NOT an integer: num={num}, den={den}, \
             remainder={}. Every fill will have rounding errors. \
             Adjust base_lot_size to make num divisible by den.",
            if den > 0 { num % den } else { 0 }
        ));
    }

    if ratio == 0 && ratio_clean {
        errors.push(
            "Tick conversion ratio is 0. base_lot_size is too small \
             relative to tick_size and decimals."
                .into(),
        );
    }

    let quote_atom_value = 1.0 / 10f64.powi(quote_decimals as i32);
    let quote_lot_human = (quote_lot_size as f64) * quote_atom_value;

    let base_atom_value = 1.0 / 10f64.powi(base_decimals as i32);
    let base_lot_tokens = (base_lot_size as f64) * base_atom_value;
    let lots_per_token = 10f64.powi(base_decimals as i32) / (base_lot_size as f64);

    if quote_lot_size != 1 {
        warnings.push(format!(
            "quote_lot_size={quote_lot_size} (not 1). Fee precision \
             will be reduced. Use 1 unless you have a specific reason."
        ));
    }

    if raw_base_units > 1 {
        warnings.push(format!(
            "raw_base_units_per_base_unit={raw_base_units}. '1 base unit' \
             means {raw_base_units} tokens. tick_size is per this scaled unit."
        ));
    }

    let mut base_lot_notional = None;
    let mut price_ticks = None;
    let mut tick_human_price = None;
    let mut bps_per_tick = None;
    let mut one_token_quote_check = None;
    let mut max_safe_tokens = None;
    let mut max_safe_notional = None;

    if let Some(p) = price {
        let lot_notional = base_lot_tokens * p;
        base_lot_notional = Some(lot_notional);

        let price_in_quote_atoms = p * 10f64.powi(quote_decimals as i32);
        let price_per_base_unit = price_in_quote_atoms * (raw_base_units as f64);
        let p_ticks = (price_per_base_unit / (tick_size as f64)).round() as u64;
        let tick_dollar = (tick_size as f64) * quote_atom_value / (raw_base_units as f64);
        let bps = if p > 0.0 {
            (tick_dollar / p) * 10_000.0
        } else {
            0.0
        };

        price_ticks = Some(p_ticks);
        tick_human_price = Some(tick_dollar);
        bps_per_tick = Some(bps);

        if p_ticks < 10 {
            errors.push(format!(
                "Only {p_ticks} ticks of price resolution at ${p}. Makers \
                 cannot quote meaningful spreads. Decrease tick_size or \
                 increase raw_base_units_per_base_unit."
            ));
        } else if p_ticks < 100 {
            warnings.push(format!(
                "Only {p_ticks} ticks of price resolution. Spreads will \
                 be coarse. Consider decreasing tick_size."
            ));
        }

        if bps > 5.0 {
            warnings.push(format!(
                "1 tick = {bps:.2} bps — very coarse. Makers can't quote \
                 tight spreads. Consider a smaller tick_size."
            ));
        }

        if lot_notional > 1.0 {
            warnings.push(format!(
                "1 base lot = ${lot_notional:.4} — quite large. Small \
                 takers will get coarse fills. Consider smaller base_lot_size."
            ));
        } else if lot_notional < 0.0001 {
            warnings.push(format!(
                "1 base lot = ${lot_notional:.8} — very tiny. Trades may \
                 require many lots, increasing compute. Consider larger \
                 base_lot_size."
            ));
        }

        // Verify 1-token trade
        let quote_lots_1token = (lots_per_token as u128)
            .saturating_mul(p_ticks as u128)
            .saturating_mul(ratio);
        let one_tok_dollars =
            (quote_lots_1token as f64) * (quote_lot_size as f64) * quote_atom_value;
        one_token_quote_check = Some(one_tok_dollars);

        if p_ticks > 0 && ratio > 0 {
            let max_lots = (u64::MAX as u128) / ((p_ticks as u128) * ratio);
            let max_tok = (max_lots as f64) / lots_per_token;
            let max_not = max_tok * p;
            max_safe_tokens = Some(max_tok);
            max_safe_notional = Some(max_not);

            if max_not < 1_000_000.0 {
                warnings.push(format!(
                    "Max single trade before u64 overflow: ${max_not:.0}. \
                     This may limit large institutional trades."
                ));
            }
        }
    }

    let net_protocol = taker_fee_ppm + maker_fee_ppm;
    let fee_solvency_ok = net_protocol >= 0;

    if !fee_solvency_ok {
        errors.push(format!(
            "Fee solvency violated: taker({taker_fee_ppm}) + \
             maker({maker_fee_ppm}) = {net_protocol} ppm. Net must be \
             >= 0 or every trade fails with FeeSolvencyViolated."
        ));
    }

    let maker_bps = maker_fee_ppm as f64 / 100.0;
    let taker_bps = taker_fee_ppm as f64 / 100.0;

    let min_taker = if taker_fee_ppm.abs() > 0 {
        let min_ql = 1_000_000.0 / (taker_fee_ppm.abs() as f64);
        Some(min_ql * (quote_lot_size as f64) * quote_atom_value)
    } else {
        None
    };

    let min_maker = if maker_fee_ppm.abs() > 0 {
        let min_ql = 1_000_000.0 / (maker_fee_ppm.abs() as f64);
        Some(min_ql * (quote_lot_size as f64) * quote_atom_value)
    } else {
        None
    };

    MarketAnalysis {
        num,
        den,
        ratio,
        ratio_clean,
        base_lot_tokens,
        base_lot_notional,
        quote_lot_human,
        lots_per_token,
        price_ticks,
        tick_human_price,
        bps_per_tick,
        fee_solvency_ok,
        net_protocol_ppm: net_protocol,
        maker_fee_bps: maker_bps,
        taker_fee_bps: taker_bps,
        min_trade_for_taker_fee: min_taker,
        min_trade_for_maker_fee: min_maker,
        one_token_quote_check,
        max_safe_tokens,
        max_safe_notional,
        warnings,
        errors,
    }
}

pub fn print_analysis(a: &MarketAnalysis, price: Option<f64>) {
    display::print_header("Tick Conversion");
    display::print_kv("num:", &a.num.to_string());
    display::print_kv("den:", &a.den.to_string());
    if a.ratio_clean {
        display::print_kv("ratio:", &format!("{} ✓", a.ratio));
    } else {
        display::print_kv(
            "ratio:",
            &format!("{}.{} ✗ NOT INTEGER", a.num / a.den, a.num % a.den),
        );
    }

    display::print_header("Lot Sizing");
    display::print_kv("1 base lot:", &format!("{} tokens", a.base_lot_tokens));
    if let Some(notional) = a.base_lot_notional {
        display::print_kv("  notional:", &format!("${notional:.6}"));
    }
    display::print_kv("1 quote lot:", &format!("${:.6}", a.quote_lot_human));
    display::print_kv("Lots per token:", &format!("{:.0}", a.lots_per_token));

    if let Some(p) = price {
        display::print_header("Price & Tick Resolution");
        display::print_kv("Reference price:", &format!("${p}"));
        if let Some(ticks) = a.price_ticks {
            display::print_kv("Price in ticks:", &format!("{ticks}"));
        }
        if let Some(tick_usd) = a.tick_human_price {
            display::print_kv("1 tick:", &format!("${tick_usd:.6}"));
        }
        if let Some(bps) = a.bps_per_tick {
            display::print_kv("  in bps:", &format!("{bps:.4} bps"));
        }

        if let Some(quote) = a.one_token_quote_check {
            display::print_header("Trade Sanity Check");
            display::print_kv("Buy 1 token →", &format!("${quote:.6}"));
            display::print_kv("Expected →", &format!("${p}"));
            let drift_pct = ((quote - p) / p * 100.0).abs();
            if drift_pct > 0.01 {
                display::print_kv(
                    "Rounding drift:",
                    &format!("{drift_pct:.4}% (from lot/tick discretization)"),
                );
            } else {
                display::print_kv("Rounding drift:", "< 0.01% ✓");
            }
        }
    }

    display::print_header("Fee Analysis");
    display::print_kv(
        "Maker fee:",
        &format!(
            "{:.2} bps{}",
            a.maker_fee_bps,
            if a.maker_fee_bps < 0.0 {
                " (rebate)"
            } else if a.maker_fee_bps == 0.0 {
                " (free)"
            } else {
                ""
            }
        ),
    );
    display::print_kv("Taker fee:", &format!("{:.2} bps", a.taker_fee_bps));
    display::print_kv(
        "Net protocol:",
        &format!(
            "{:.2} bps {}",
            a.net_protocol_ppm as f64 / 100.0,
            if a.fee_solvency_ok {
                "✓"
            } else {
                "✗ INSOLVENT"
            }
        ),
    );

    if let Some(min_t) = a.min_trade_for_taker_fee {
        display::print_kv("Min trade for 1 atom taker fee:", &format!("${min_t:.6}"));
    }
    if let Some(min_m) = a.min_trade_for_maker_fee {
        display::print_kv("Min trade for 1 atom maker fee:", &format!("${min_m:.6}"));
    }

    if let Some(p) = price {
        println!();
        println!("  Fee cost at ${p}:");
        for &size in &[1.0, 100.0, 10_000.0, 1_000_000.0] {
            let taker_cost = size * (a.taker_fee_bps.abs() / 10_000.0);
            let maker_cost = size * (a.maker_fee_bps.abs() / 10_000.0);
            let taker_label = if a.taker_fee_bps >= 0.0 {
                format!("-${taker_cost:.4}")
            } else {
                format!("+${taker_cost:.4}")
            };
            let maker_label = if a.maker_fee_bps >= 0.0 {
                format!("-${maker_cost:.4}")
            } else {
                format!("+${maker_cost:.4}")
            };
            println!(
                "    ${:<12} trade → taker: {:<12} maker: {}",
                format!("{size:.0}"),
                taker_label,
                maker_label,
            );
        }
    }

    if let (Some(max_tokens), Some(max_notional)) = (a.max_safe_tokens, a.max_safe_notional) {
        display::print_header("Overflow Safety");
        display::print_kv(
            "Max single trade:",
            &format!("{max_tokens:.2} tokens (${max_notional:.0})"),
        );
        if max_notional > 1_000_000_000.0 {
            display::print_kv("", "Safe for any realistic trade ✓");
        } else if max_notional > 1_000_000.0 {
            display::print_kv("", "OK — watch very large institutional fills");
        } else {
            display::print_kv("", " WARNING -- Tight — large trades may overflow");
        }
    }

    if !a.warnings.is_empty() {
        display::print_header("Warnings");
        for (i, w) in a.warnings.iter().enumerate() {
            println!("  WARNING -- {}. {w}", i + 1);
        }
    }

    if !a.errors.is_empty() {
        display::print_header("ERRORS");
        for (i, e) in a.errors.iter().enumerate() {
            println!("  ERROR -- {}. {e}", i + 1);
        }
    }
}
