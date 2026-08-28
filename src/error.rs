use std::fmt;

#[derive(Debug)]
pub enum CliError {
    Rpc(String),
    AccountNotFound(String),
    Deserialization(String),
    SimulationFailed { logs: Vec<String> },
    InvalidInput(String),
    Keypair(String),
    UserAborted,
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rpc(msg) => write!(f, "RPC error: {msg}"),
            Self::AccountNotFound(addr) => write!(f, "Account not found: {addr}"),
            Self::Deserialization(msg) => write!(f, "Deserialization error: {msg}"),
            Self::SimulationFailed { logs } => {
                writeln!(f, "Simulation failed. Program logs:")?;
                for log in logs {
                    writeln!(f, "  {log}")?;
                }
                Ok(())
            }
            Self::InvalidInput(msg) => write!(f, "Invalid input: {msg}"),
            Self::Keypair(msg) => write!(f, "Keypair error: {msg}"),
            Self::UserAborted => write!(f, "Operation aborted by user"),
        }
    }
}

impl std::error::Error for CliError {}

impl From<solana_client::client_error::ClientError> for CliError {
    fn from(e: solana_client::client_error::ClientError) -> Self {
        Self::Rpc(e.to_string())
    }
}
