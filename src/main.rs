//! Archer Protocol CLI — public.
//!
//! Covers everything a market creator, market maker or taker can sign for.


use clap::Parser;

use archer_cli::{dispatch, GlobalArgs, PublicCommands};

#[derive(Parser)]
#[command(name = "archer", about = "Archer Protocol CLI", version)]
struct Cli {
    #[command(flatten)]
    global: GlobalArgs,

    #[command(subcommand)]
    command: PublicCommands,
}

fn main() {
    let cli = Cli::parse();

    let config = match cli.global.load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    };

    if let Err(e) = dispatch(cli.command, &config) {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
