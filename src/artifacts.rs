use crate::adapters::{ProviderAck, ProviderKind, RouteSelection};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchManifest {
    pub schema_version: u32,
    pub test_id: String,
    pub generated_at: DateTime<Utc>,
    pub rpc_url_label: String,
    pub keypair_pubkey: String,
    pub count: usize,
    pub duration_seconds: Option<u64>,
    pub rate_per_second: Option<f64>,
    pub lamports: u64,
    pub compute_unit_limit: u32,
    pub compute_unit_price_microlamports: u64,
    pub memo_prefix: String,
    pub max_spend_lamports: Option<u64>,
    pub route_strategy: Option<String>,
    pub client_aware_harmonic_cu_price_microlamports: Option<u64>,
    pub providers: Vec<ManifestProvider>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestProvider {
    pub name: String,
    pub kind: ProviderKind,
    #[serde(default)]
    pub route_mode: Option<String>,
    #[serde(default)]
    pub routes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchSample {
    pub schema_version: u32,
    pub test_id: String,
    pub iteration: usize,
    pub signature: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comparison_group_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_arm: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_arm_index: Option<u16>,
    pub provider_name: String,
    pub provider_kind: ProviderKind,
    pub accepted: bool,
    pub client_started_at: DateTime<Utc>,
    pub client_finished_at: DateTime<Utc>,
    pub client_ack_latency_us: u128,
    pub provider_send_started_at: DateTime<Utc>,
    pub provider_send_finished_at: DateTime<Utc>,
    pub provider_ack_latency_us: u128,
    pub provider_request_id: Option<String>,
    pub returned_signature: Option<String>,
    pub status_code: Option<u16>,
    pub error_class: Option<String>,
    pub error: Option<String>,
    pub route_policy: Option<String>,
    pub route_mode: Option<String>,
    pub selected_routes: Vec<String>,
    pub leader_client_family: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leader_software_client: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leader_software_client_id: Option<u16>,
    pub compute_unit_limit: u32,
    pub compute_unit_price_microlamports: u64,
    pub estimated_spend_lamports: u64,
}

impl BenchSample {
    pub fn from_ack(
        test_id: &str,
        iteration: usize,
        signature: String,
        comparison_group_id: Option<String>,
        policy_arm: Option<String>,
        policy_arm_index: Option<u16>,
        client_started_at: DateTime<Utc>,
        client_finished_at: DateTime<Utc>,
        client_ack_latency_us: u128,
        route_selection: Option<&RouteSelection>,
        leader_client_family: Option<String>,
        leader_software_client: Option<String>,
        leader_software_client_id: Option<u16>,
        compute_unit_limit: u32,
        compute_unit_price_microlamports: u64,
        estimated_spend_lamports: u64,
        ack: ProviderAck,
    ) -> Self {
        Self {
            schema_version: 3,
            test_id: test_id.to_string(),
            iteration,
            signature,
            comparison_group_id,
            policy_arm,
            policy_arm_index,
            provider_name: ack.provider_name,
            provider_kind: ack.provider_kind,
            accepted: ack.accepted,
            client_started_at,
            client_finished_at,
            client_ack_latency_us,
            provider_send_started_at: ack.send_started_at,
            provider_send_finished_at: ack.send_finished_at,
            provider_ack_latency_us: ack.ack_latency_us,
            provider_request_id: ack.provider_request_id,
            returned_signature: ack.returned_signature,
            status_code: ack.status_code,
            error_class: ack.error_class,
            error: ack.error,
            route_policy: route_selection.map(|selection| selection.policy.clone()),
            route_mode: route_selection.map(|selection| selection.mode.as_wire().to_string()),
            selected_routes: route_selection
                .map(|selection| selection.routes.clone())
                .unwrap_or_default(),
            leader_client_family,
            leader_software_client,
            leader_software_client_id,
            compute_unit_limit,
            compute_unit_price_microlamports,
            estimated_spend_lamports,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchSummary {
    pub schema_version: u32,
    pub test_id: String,
    pub generated_at: DateTime<Utc>,
    pub total_samples: usize,
    pub provider_summaries: Vec<ProviderSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSummary {
    pub provider_name: String,
    pub provider_kind: ProviderKind,
    pub count: usize,
    pub accepted: usize,
    pub errors: usize,
    pub min_us: Option<u128>,
    pub p50_us: Option<u128>,
    pub p75_us: Option<u128>,
    pub p90_us: Option<u128>,
    pub p95_us: Option<u128>,
    pub p99_us: Option<u128>,
    pub max_us: Option<u128>,
}

pub struct ArtifactWriter {
    pub run_dir: PathBuf,
    samples: BufWriter<File>,
}

impl ArtifactWriter {
    pub fn create(root: &Path, test_id: &str) -> anyhow::Result<Self> {
        let run_dir = root.join(test_id);
        fs::create_dir_all(&run_dir)?;
        let samples = BufWriter::new(File::create(run_dir.join("samples.ndjson"))?);
        Ok(Self { run_dir, samples })
    }

    pub fn write_manifest(&self, manifest: &BenchManifest) -> anyhow::Result<()> {
        let bytes = serde_json::to_vec_pretty(manifest)?;
        fs::write(self.run_dir.join("manifest.json"), bytes)?;
        Ok(())
    }

    pub fn write_sample(&mut self, sample: &BenchSample) -> anyhow::Result<()> {
        serde_json::to_writer(&mut self.samples, sample)?;
        self.samples.write_all(b"\n")?;
        Ok(())
    }

    pub fn finish(mut self, summary: &BenchSummary) -> anyhow::Result<PathBuf> {
        self.samples.flush()?;
        fs::write(
            self.run_dir.join("summary.json"),
            serde_json::to_vec_pretty(summary)?,
        )?;
        fs::write(self.run_dir.join("summary.md"), summary_markdown(summary))?;
        Ok(self.run_dir)
    }
}

pub fn summarize(test_id: &str, samples: &[BenchSample]) -> BenchSummary {
    let mut keys = samples
        .iter()
        .map(|sample| (sample.provider_name.clone(), sample.provider_kind))
        .collect::<Vec<_>>();
    keys.sort();
    keys.dedup();

    let provider_summaries = keys
        .into_iter()
        .map(|(name, kind)| {
            let mut latencies = samples
                .iter()
                .filter(|sample| sample.provider_name == name && sample.accepted)
                .map(|sample| sample.provider_ack_latency_us)
                .collect::<Vec<_>>();
            latencies.sort_unstable();
            let count = samples
                .iter()
                .filter(|sample| sample.provider_name == name)
                .count();
            let accepted = latencies.len();
            ProviderSummary {
                provider_name: name,
                provider_kind: kind,
                count,
                accepted,
                errors: count.saturating_sub(accepted),
                min_us: latencies.first().copied(),
                p50_us: percentile(&latencies, 0.50),
                p75_us: percentile(&latencies, 0.75),
                p90_us: percentile(&latencies, 0.90),
                p95_us: percentile(&latencies, 0.95),
                p99_us: percentile(&latencies, 0.99),
                max_us: latencies.last().copied(),
            }
        })
        .collect::<Vec<_>>();

    BenchSummary {
        schema_version: 1,
        test_id: test_id.to_string(),
        generated_at: Utc::now(),
        total_samples: samples.len(),
        provider_summaries,
    }
}

pub fn summary_markdown(summary: &BenchSummary) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Solana Tx Bench Summary\n\n"));
    out.push_str(&format!(
        "- Test ID: `{}`\n- Generated: `{}`\n- Samples: `{}`\n\n",
        summary.test_id,
        summary
            .generated_at
            .to_rfc3339_opts(SecondsFormat::Millis, true),
        summary.total_samples
    ));
    out.push_str(
        "| Provider | Kind | Count | Accepted | Errors | p50 us | p90 us | p99 us | Max us |\n",
    );
    out.push_str("| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n");
    for item in &summary.provider_summaries {
        out.push_str(&format!(
            "| `{}` | `{:?}` | {} | {} | {} | {} | {} | {} | {} |\n",
            item.provider_name,
            item.provider_kind,
            item.count,
            item.accepted,
            item.errors,
            fmt(item.p50_us),
            fmt(item.p90_us),
            fmt(item.p99_us),
            fmt(item.max_us)
        ));
    }
    out.push_str("\nProvider ACK is not landing or first-shred latency.\n");
    out
}

fn percentile(sorted: &[u128], p: f64) -> Option<u128> {
    if sorted.is_empty() {
        return None;
    }
    let rank = ((sorted.len() - 1) as f64 * p).ceil() as usize;
    sorted.get(rank).copied()
}

fn fmt(value: Option<u128>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-".to_string())
}
