use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct BenchConfig {
    pub test_id: Option<String>,
    pub keypair_path: Option<PathBuf>,
    pub rpc_url: String,
    pub artifact_dir: PathBuf,
    pub providers: Vec<ProviderSpec>,
    pub count: Option<usize>,
    pub duration_seconds: Option<u64>,
    pub rate_per_second: Option<f64>,
    pub lamports: u64,
    pub compute_unit_limit: u32,
    pub compute_unit_price_microlamports: u64,
    pub memo_prefix: Option<String>,
    pub max_spend_lamports: Option<u64>,
    pub timeout_ms: u64,
}

impl Default for BenchConfig {
    fn default() -> Self {
        Self {
            test_id: None,
            keypair_path: None,
            rpc_url: "https://api.mainnet-beta.solana.com".to_string(),
            artifact_dir: PathBuf::from("artifacts"),
            providers: Vec::new(),
            count: Some(1),
            duration_seconds: None,
            rate_per_second: None,
            lamports: 500,
            compute_unit_limit: 5_000,
            compute_unit_price_microlamports: 0,
            memo_prefix: None,
            max_spend_lamports: None,
            timeout_ms: 2_000,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProviderSpec {
    pub name: String,
    #[serde(flatten)]
    pub config: crate::adapters::ProviderConfig,
}
