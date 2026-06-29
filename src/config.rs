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
    pub observation_sources: Vec<ObservationSourceSpec>,
    pub observation_timeout_ms: u64,
    pub min_observation_sources: usize,
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
            observation_sources: Vec::new(),
            observation_timeout_ms: 5_000,
            min_observation_sources: 2,
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ObservationSourceSpec {
    pub name: String,
    #[serde(flatten)]
    pub config: ObservationSourceConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ObservationSourceConfig {
    YellowstoneProcessed {
        endpoint: String,
        #[serde(default)]
        x_token_env: Option<String>,
    },
    YellowstoneDeshred {
        endpoint: String,
        #[serde(default)]
        x_token_env: Option<String>,
    },
    RawShredstreamUdp {
        bind: String,
    },
    NdjsonImport {
        path: PathBuf,
    },
}
