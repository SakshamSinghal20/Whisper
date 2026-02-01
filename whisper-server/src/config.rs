use serde::Deserialize;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Environment variable error: {0}")]
    Env(#[from] std::env::VarError),
    #[error("Parse error: {0}")]
    Parse(String),
}

#[derive(Clone, Debug, Deserialize)]
pub struct ServerConfig {
    pub database_url: String,
    pub bitcoin_rpc_url: String,
    pub bitcoin_rpc_user: String,
    pub bitcoin_rpc_pass: String,
    pub zmq_socket: String,
    pub network: String,
    pub host: String,
    pub port: u16,
    pub max_block_range: i32,
    pub max_prefixes: usize,
}

impl ServerConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            database_url: std::env::var("DATABASE_URL")?,
            bitcoin_rpc_url: std::env::var("BITCOIN_RPC_URL")
                .unwrap_or_else(|_| "http://localhost:8332".into()),
            bitcoin_rpc_user: std::env::var("BITCOIN_RPC_USER")
                .unwrap_or_else(|_| "bitcoin".into()),
            bitcoin_rpc_pass: std::env::var("BITCOIN_RPC_PASS")
                .unwrap_or_else(|_| "password".into()),
            zmq_socket: std::env::var("ZMQ_BLOCK_SOCKET")
                .unwrap_or_else(|_| "tcp://127.0.0.1:28332".into()),
            network: std::env::var("NETWORK")
                .unwrap_or_else(|_| "regtest".into()),
            host: std::env::var("HOST")
                .unwrap_or_else(|_| "0.0.0.0".into()),
            port: std::env::var("PORT")
                .unwrap_or_else(|_| "3000".into())
                .parse()
                .map_err(|e| ConfigError::Parse(format!("Invalid port: {}", e)))?,
            max_block_range: std::env::var("MAX_BLOCK_RANGE")
                .unwrap_or_else(|_| "1000".into())
                .parse()
                .map_err(|e| ConfigError::Parse(format!("Invalid max_block_range: {}", e)))?,
            max_prefixes: std::env::var("MAX_PREFIXES")
                .unwrap_or_else(|_| "1000".into())
                .parse()
                .map_err(|e| ConfigError::Parse(format!("Invalid max_prefixes: {}", e)))?,
        })
    }
}
