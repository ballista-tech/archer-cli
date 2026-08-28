use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction,
    instruction::Instruction,
    signature::{Keypair, Signer},
    transaction::Transaction,
};

use crate::{display, error::CliError};

pub fn send_transaction(
    client: &RpcClient,
    ixs: Vec<Instruction>,
    signers: &[&Keypair],
    dry_run: bool,
    priority_fee: u64,
) -> Result<(), CliError> {
    let mut all_ixs = Vec::new();

    if priority_fee > 0 {
        all_ixs.push(ComputeBudgetInstruction::set_compute_unit_price(
            priority_fee,
        ));
    }

    all_ixs.extend(ixs);

    let blockhash = client.get_latest_blockhash()?;
    let tx = Transaction::new_signed_with_payer(
        &all_ixs,
        Some(&signers[0].pubkey()),
        signers,
        blockhash,
    );

    if dry_run {
        let result = client.simulate_transaction(&tx)?;
        if let Some(err) = result.value.err {
            return Err(CliError::SimulationFailed {
                logs: result.value.logs.unwrap_or_else(|| vec![err.to_string()]),
            });
        }
        display::print_dry_run(
            result.value.units_consumed,
            &result.value.logs.unwrap_or_default(),
        );
        return Ok(());
    }

    let sig = client.send_and_confirm_transaction(&tx)?;
    display::print_success(&sig.to_string());
    Ok(())
}

pub fn confirm_destructive(prompt: &str, skip_confirm: bool) -> Result<(), CliError> {
    if skip_confirm {
        return Ok(());
    }
    eprint!("  ⚠ {prompt} [y/N]: ");
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .map_err(|e| CliError::InvalidInput(e.to_string()))?;
    if input.trim().eq_ignore_ascii_case("y") {
        Ok(())
    } else {
        Err(CliError::UserAborted)
    }
}
