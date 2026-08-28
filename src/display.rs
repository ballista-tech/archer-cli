use comfy_table::{Cell, Table};

pub fn make_table(headers: &[&str]) -> Table {
    let mut table = Table::new();
    table.set_header(headers.iter().map(|h| Cell::new(h)));
    table
}

pub fn print_kv(label: &str, value: &str) {
    println!("  {:<28} {}", label, value);
}

pub fn print_header(title: &str) {
    println!();
    println!("═══ {} ═══", title);
    println!();
}

pub fn print_success(sig: &str) {
    println!();
    println!("  Transaction confirmed");
    println!("  Signature: {sig}");
    println!("  Explorer:  https://solscan.io/tx/{sig}");
}

pub fn print_dry_run(cu_consumed: Option<u64>, logs: &[String]) {
    println!();
    println!("  [DRY RUN] Transaction simulated successfully");
    if let Some(cu) = cu_consumed {
        println!("  Compute units: {cu}");
    }
    if !logs.is_empty() {
        println!("  Logs:");
        for log in logs {
            println!("    {log}");
        }
    }
}

pub fn format_fee_ppm(ppm: i32) -> String {
    let bps = ppm as f64 / 100.0;
    if ppm >= 0 {
        format!("{ppm} ppm ({bps:.2} bps)")
    } else {
        format!("{ppm} ppm ({bps:.2} bps rebate)")
    }
}

pub fn format_status(status: u8) -> &'static str {
    match status {
        0 => "Active",
        1 => "Paused",
        2 => "Closed",
        _ => "Unknown",
    }
}

pub fn format_side(side: u8) -> &'static str {
    match side {
        0 => "Buy (Bid)",
        1 => "Sell (Ask)",
        _ => "Unknown",
    }
}

pub fn format_swap_mode(mode: u8) -> &'static str {
    match mode {
        0 => "MaxAmountIn",
        1 => "MinAmountOut",
        _ => "Unknown",
    }
}
