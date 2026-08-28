use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    signature::{read_keypair_file, Keypair},
};

use crate::error::CliError;

pub struct CliConfig {
    pub rpc_client: RpcClient,
    pub keypair: Keypair,
    pub dry_run: bool,
    pub json_output: bool,
    pub priority_fee: u64,
}

impl CliConfig {
    pub fn load(
        url: Option<String>,
        keypair_path: Option<String>,
        commitment: &str,
        dry_run: bool,
        json_output: bool,
        priority_fee: u64,
    ) -> Result<Self, CliError> {
        let solana_config = solana_cli_config::Config::default();
        let solana_config_file = solana_cli_config::CONFIG_FILE
            .as_ref()
            .and_then(|f| solana_cli_config::Config::load(f).ok())
            .unwrap_or(solana_config);

        let rpc_url = url.unwrap_or(solana_config_file.json_rpc_url);

        let commitment = match commitment {
            "finalized" => CommitmentConfig::finalized(),
            _ => CommitmentConfig::confirmed(),
        };

        let rpc_client = RpcClient::new_with_commitment(rpc_url, commitment);

        let kp_path = keypair_path
            .or_else(|| std::env::var("ARCHER_ADMIN_KEYPAIR").ok())
            .unwrap_or(solana_config_file.keypair_path);

        let keypair = read_keypair_file(&kp_path)
            .map_err(|e| CliError::Keypair(format!("{kp_path}: {e}")))?;

        Ok(Self {
            rpc_client,
            keypair,
            dry_run,
            json_output,
            priority_fee,
        })
    }

    pub fn _load_rpc_only(
        url: Option<String>,
        commitment: &str,
        json_output: bool,
    ) -> Result<Self, CliError> {
        let solana_config = solana_cli_config::Config::default();
        let solana_config_file = solana_cli_config::CONFIG_FILE
            .as_ref()
            .and_then(|f| solana_cli_config::Config::load(f).ok())
            .unwrap_or(solana_config);

        let rpc_url = url.unwrap_or(solana_config_file.json_rpc_url);

        let commitment = match commitment {
            "finalized" => CommitmentConfig::finalized(),
            _ => CommitmentConfig::confirmed(),
        };

        let rpc_client = RpcClient::new_with_commitment(rpc_url, commitment);

        Ok(Self {
            rpc_client,
            keypair: Keypair::new(), // dummy, unused for observe
            dry_run: false,
            json_output,
            priority_fee: 0,
        })
    }
}
