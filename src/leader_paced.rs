use crate::{
    adapters::{build_adapter, ProviderAdapter, RouteSelection, SendContext},
    artifacts::{summarize, ArtifactWriter, BenchManifest, BenchSample, ManifestProvider},
    collectors::{collect_rpcedge_observations, CollectRunOutput, RpcEdgeCollectConfig},
    config::BenchConfig,
    leader_slots::{
        capture_leader_slots_snapshot, write_leader_slots_snapshot, LeaderSlotsCaptureConfig,
    },
    observations::{
        observation_summary_markdown, summarize_observations, MatchedObservationSummary,
        ObservationEvent,
    },
    tx::{
        build_transaction_with_blockhash, estimated_transaction_spend, load_keypair, BenchTx,
        TxBuildConfig,
    },
};
use anyhow::{bail, Context, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use futures::future::join_all;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use solana_client::rpc_client::RpcClient;
use solana_commitment_config::CommitmentConfig;
use solana_sdk::signer::Signer;
use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, File},
    io::{BufRead, BufReader, BufWriter, Write},
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaderPacedRouteStrategy {
    Static,
    ClientAware,
}

impl LeaderPacedRouteStrategy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Static => "static",
            Self::ClientAware => "client_aware",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LeaderPacedOptions {
    pub duration: Duration,
    pub txs_per_leader_run: usize,
    pub leader_run_concurrency: usize,
    pub lookbehind_slots: u64,
    pub lookahead_slots: u64,
    pub poll_interval: Duration,
    pub observe_extra: Duration,
    pub rpcedge: Option<RpcEdgeLeaderCollector>,
    pub leader_slots: Option<LeaderSlotsCaptureConfig>,
    pub route_strategy: LeaderPacedRouteStrategy,
    pub client_aware_harmonic_cu_price_microlamports: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct RpcEdgeLeaderCollector {
    pub endpoint: String,
    pub x_token: Option<String>,
    pub min_sources: usize,
}

#[derive(Debug)]
pub struct LeaderPacedRunOutput {
    pub test_id: String,
    pub run_dir: PathBuf,
    pub sent_transactions: usize,
    pub provider_samples: usize,
    pub collector: Option<CollectRunOutput>,
    pub matched_observation_summary: Option<MatchedObservationSummary>,
    pub leader_slots_snapshot_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderSendEvent {
    pub schema_version: u32,
    pub test_id: String,
    pub iteration: usize,
    pub signature: String,
    pub leader_identity: String,
    pub leader_run_start_slot: u64,
    pub leader_run_end_slot: u64,
    pub send_slot: u64,
    pub sent_at: DateTime<Utc>,
    pub leader_client_family: Option<String>,
    pub route_policy: Option<String>,
    pub selected_routes: Vec<String>,
    pub compute_unit_limit: u32,
    pub compute_unit_price_microlamports: u64,
    pub estimated_spend_lamports: u64,
}

#[derive(Debug, Clone)]
struct LeaderRun {
    leader_identity: String,
    start_slot: u64,
    end_slot: u64,
}

pub async fn run_leader_paced(
    config: BenchConfig,
    options: LeaderPacedOptions,
) -> Result<LeaderPacedRunOutput> {
    if config.providers.is_empty() {
        bail!("at least one provider is required");
    }
    if options.txs_per_leader_run == 0 {
        bail!("txs_per_leader_run must be greater than zero");
    }
    if options.leader_run_concurrency == 0 {
        bail!("leader_run_concurrency must be greater than zero");
    }
    if options.route_strategy == LeaderPacedRouteStrategy::ClientAware
        && options.leader_slots.is_none()
    {
        bail!("client-aware route strategy requires --capture-leader-slots");
    }

    let test_id = config
        .test_id
        .clone()
        .unwrap_or_else(default_leader_paced_test_id);
    let keypair_path = config
        .keypair_path
        .as_ref()
        .context("keypair_path is required for transaction generation")?;
    let payer = load_keypair(keypair_path)?;
    let memo_prefix = config
        .memo_prefix
        .clone()
        .unwrap_or_else(|| test_id.clone());
    let rpc = RpcClient::new_with_commitment(config.rpc_url.clone(), CommitmentConfig::processed());
    let timeout = Duration::from_millis(config.timeout_ms);
    let mut providers = config
        .providers
        .iter()
        .map(|spec| build_adapter(spec.name.clone(), spec.config.clone(), timeout))
        .collect::<Result<Vec<_>>>()?;
    for provider in providers.iter_mut() {
        provider.warmup().await?;
    }

    let mut writer = ArtifactWriter::create(&config.artifact_dir, &test_id)?;
    let mut leader_lookup = LeaderMetadataLookup::default();
    let leader_slots_snapshot_path = if let Some(leader_slots) = options.leader_slots.as_ref() {
        let start_slot = rpc
            .get_slot()
            .context("get slot for getLeaderSlots snapshot")?;
        let artifact = capture_leader_slots_snapshot(leader_slots, start_slot).await?;
        leader_lookup = LeaderMetadataLookup::from_artifact(&artifact);
        write_leader_slots_snapshot(&writer.run_dir, &artifact)?;
        Some(writer.run_dir.join("leader-slots-snapshot.json"))
    } else {
        None
    };
    let leader_sends_path = writer.run_dir.join("leader-sends.ndjson");
    let mut leader_sends = BufWriter::new(
        File::create(&leader_sends_path)
            .with_context(|| format!("create {}", leader_sends_path.display()))?,
    );

    let collector_handle = options.rpcedge.clone().map(|rpcedge| {
        let output_dir = writer.run_dir.join("rpcedge-observations");
        let duration = options.duration + options.observe_extra;
        let account_include = vec![payer.pubkey().to_string()];
        let collector_config = RpcEdgeCollectConfig {
            test_id: test_id.clone(),
            endpoint: rpcedge.endpoint,
            x_token: rpcedge.x_token,
            output_dir,
            duration,
            account_include,
            min_sources: rpcedge.min_sources,
        };
        tokio::spawn(async move { collect_rpcedge_observations(collector_config).await })
    });

    let deadline = Instant::now() + options.duration;
    let mut sent_runs = HashSet::<(String, u64)>::new();
    let mut samples = Vec::<BenchSample>::new();
    let mut sends = Vec::<LeaderSendEvent>::new();
    let mut estimated_spent = 0_u64;
    let mut iteration = 0_usize;

    while Instant::now() < deadline {
        let send_slot = rpc.get_slot().context("get current slot")?;
        let leader_run = current_leader_run(
            &rpc,
            send_slot,
            options.lookbehind_slots,
            options.lookahead_slots,
        )?;
        let run_key = (leader_run.leader_identity.clone(), leader_run.start_slot);
        if sent_runs.insert(run_key) {
            let leader_metadata =
                leader_lookup.lookup(send_slot, leader_run.leader_identity.as_str());
            let route_selection = route_selection_for_strategy(
                options.route_strategy,
                leader_metadata.client_family.as_deref(),
            );
            let compute_unit_price_microlamports = compute_unit_price_for_strategy(
                &config,
                &options,
                leader_metadata.client_family.as_deref(),
            );
            let tx_config = TxBuildConfig {
                rpc_url: config.rpc_url.clone(),
                lamports: config.lamports,
                compute_unit_limit: config.compute_unit_limit,
                compute_unit_price_microlamports,
            };
            let blockhash = rpc.get_latest_blockhash().context("get latest blockhash")?;
            let mut pending = Vec::with_capacity(options.txs_per_leader_run);
            for _ in 0..options.txs_per_leader_run {
                let estimated_next =
                    estimated_spent.saturating_add(estimated_transaction_spend(&tx_config));
                if let Some(max) = config.max_spend_lamports {
                    if estimated_next > max {
                        bail!(
                            "spend cap exceeded before tx {iteration}: estimated_next_total={} max={max}",
                            estimated_next
                        );
                    }
                }

                let tx =
                    build_transaction_with_blockhash(&tx_config, &payer, iteration, blockhash)?;
                estimated_spent = estimated_spent.saturating_add(tx.estimated_spend_lamports);
                let sent_at = Utc::now();
                let event = LeaderSendEvent {
                    schema_version: 2,
                    test_id: test_id.clone(),
                    iteration,
                    signature: tx.signature.to_string(),
                    leader_identity: leader_run.leader_identity.clone(),
                    leader_run_start_slot: leader_run.start_slot,
                    leader_run_end_slot: leader_run.end_slot,
                    send_slot,
                    sent_at,
                    leader_client_family: leader_metadata.client_family.clone(),
                    route_policy: route_selection
                        .as_ref()
                        .map(|selection| selection.policy.clone()),
                    selected_routes: route_selection
                        .as_ref()
                        .map(|selection| selection.routes.clone())
                        .unwrap_or_default(),
                    compute_unit_limit: tx_config.compute_unit_limit,
                    compute_unit_price_microlamports: tx_config.compute_unit_price_microlamports,
                    estimated_spend_lamports: tx.estimated_spend_lamports,
                };
                serde_json::to_writer(&mut leader_sends, &event)
                    .context("write leader send event")?;
                leader_sends
                    .write_all(b"\n")
                    .context("write leader send newline")?;
                leader_sends.flush().context("flush leader send event")?;
                sends.push(event);
                pending.push((
                    tx,
                    sent_at,
                    iteration,
                    route_selection.clone(),
                    leader_metadata.clone(),
                    tx_config.compute_unit_limit,
                    tx_config.compute_unit_price_microlamports,
                ));
                iteration += 1;
            }
            for chunk in pending.chunks(options.leader_run_concurrency) {
                let futures = chunk.iter().map(
                    |(
                        tx,
                        sent_at,
                        iteration,
                        route_selection,
                        leader_metadata,
                        compute_unit_limit,
                        compute_unit_price_microlamports,
                    )| {
                        send_one_transaction_to_providers(
                            providers.as_slice(),
                            test_id.as_str(),
                            tx,
                            *sent_at,
                            *iteration,
                            timeout,
                            route_selection.clone(),
                            leader_metadata.client_family.clone(),
                            *compute_unit_limit,
                            *compute_unit_price_microlamports,
                        )
                    },
                );
                for tx_samples in join_all(futures).await {
                    for sample in tx_samples {
                        writer.write_sample(&sample)?;
                        samples.push(sample);
                    }
                }
            }
        }
        tokio::time::sleep(options.poll_interval).await;
    }

    leader_sends.flush().context("flush leader sends")?;

    let collector = if let Some(handle) = collector_handle {
        Some(handle.await.context("join RPCEdge collector")??)
    } else {
        None
    };

    let matched_observation_summary = if let Some(collector) = collector.as_ref() {
        let min_sources = options
            .rpcedge
            .as_ref()
            .map(|rpcedge| rpcedge.min_sources)
            .unwrap_or(2);
        Some(write_matched_observation_summary(
            &writer.run_dir,
            &test_id,
            &samples,
            &collector.observations_path,
            min_sources,
        )?)
    } else {
        None
    };

    let manifest = BenchManifest {
        schema_version: 1,
        test_id: test_id.clone(),
        generated_at: Utc::now(),
        rpc_url_label: redact_url(&config.rpc_url),
        keypair_pubkey: payer.pubkey().to_string(),
        count: sends.len(),
        duration_seconds: Some(options.duration.as_secs()),
        rate_per_second: None,
        lamports: config.lamports,
        compute_unit_limit: config.compute_unit_limit,
        compute_unit_price_microlamports: config.compute_unit_price_microlamports,
        memo_prefix,
        max_spend_lamports: config.max_spend_lamports,
        route_strategy: Some(options.route_strategy.as_str().to_string()),
        client_aware_harmonic_cu_price_microlamports: options
            .client_aware_harmonic_cu_price_microlamports,
        providers: config
            .providers
            .iter()
            .map(|spec| ManifestProvider {
                name: spec.name.clone(),
                kind: spec.config.kind(),
            })
            .collect(),
    };
    writer.write_manifest(&manifest)?;
    let summary = summarize(&test_id, &samples);
    let run_dir = writer.finish(&summary)?;

    Ok(LeaderPacedRunOutput {
        test_id,
        run_dir,
        sent_transactions: sends.len(),
        provider_samples: samples.len(),
        collector,
        matched_observation_summary,
        leader_slots_snapshot_path,
    })
}

async fn send_one_transaction_to_providers(
    providers: &[Box<dyn ProviderAdapter>],
    test_id: &str,
    tx: &BenchTx,
    sent_at: DateTime<Utc>,
    iteration: usize,
    timeout: Duration,
    route_selection: Option<RouteSelection>,
    leader_client_family: Option<String>,
    compute_unit_limit: u32,
    compute_unit_price_microlamports: u64,
) -> Vec<BenchSample> {
    let started = Instant::now();
    let ctx = SendContext {
        test_id: test_id.to_string(),
        iteration,
        signature: tx.signature.to_string(),
        tx_base64: tx.base64.clone(),
        timeout,
        route_selection: route_selection.clone(),
        leader_client_family: leader_client_family.clone(),
    };
    let futures = providers
        .iter()
        .map(|provider| provider.send_transaction(&tx.raw, &ctx));
    let acks = join_all(futures).await;
    let client_finished_at = Utc::now();
    let client_ack_latency_us = started.elapsed().as_micros();
    acks.into_iter()
        .map(|ack| {
            BenchSample::from_ack(
                test_id,
                iteration,
                tx.signature.to_string(),
                sent_at,
                client_finished_at,
                client_ack_latency_us,
                route_selection.as_ref(),
                leader_client_family.clone(),
                compute_unit_limit,
                compute_unit_price_microlamports,
                tx.estimated_spend_lamports,
                ack,
            )
        })
        .collect()
}

#[derive(Debug, Clone, Default)]
struct LeaderMetadata {
    client_family: Option<String>,
}

#[derive(Debug, Default)]
struct LeaderMetadataLookup {
    by_slot: BTreeMap<u64, LeaderMetadata>,
    by_identity: BTreeMap<String, LeaderMetadata>,
}

impl LeaderMetadataLookup {
    fn from_artifact(artifact: &crate::leader_slots::LeaderSlotsSnapshotArtifact) -> Self {
        let data = artifact
            .response
            .get("result")
            .and_then(|result| result.get("data"))
            .and_then(Value::as_array)
            .or_else(|| artifact.response.get("data").and_then(Value::as_array));
        let Some(rows) = data else {
            return Self::default();
        };

        let mut lookup = Self::default();
        for row in rows {
            let Some(slot) = row.get("slot").and_then(Value::as_u64) else {
                continue;
            };
            let identity = row
                .get("identity")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            let client_family = row
                .get("client")
                .and_then(|client| client.get("family"))
                .and_then(Value::as_str)
                .map(normalize_client_family);
            let metadata = LeaderMetadata { client_family };
            lookup.by_slot.insert(slot, metadata.clone());
            if let Some(identity) = identity {
                lookup.by_identity.entry(identity).or_insert(metadata);
            }
        }
        lookup
    }

    fn lookup(&self, slot: u64, identity: &str) -> LeaderMetadata {
        self.by_slot
            .get(&slot)
            .or_else(|| self.by_identity.get(identity))
            .cloned()
            .unwrap_or_default()
    }
}

fn normalize_client_family(value: &str) -> String {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.contains("jito") {
        "jito".to_string()
    } else if normalized.contains("harmonic") {
        "harmonic".to_string()
    } else if normalized.contains("bam") {
        "bam".to_string()
    } else if normalized.contains("firedancer") || normalized.contains("frankendancer") {
        "firedancer".to_string()
    } else if normalized.contains("agave") || normalized.contains("anza") {
        "agave".to_string()
    } else {
        normalized
    }
}

fn route_selection_for_strategy(
    strategy: LeaderPacedRouteStrategy,
    client_family: Option<&str>,
) -> Option<RouteSelection> {
    match strategy {
        LeaderPacedRouteStrategy::Static => None,
        LeaderPacedRouteStrategy::ClientAware => {
            let family = client_family.unwrap_or("unknown");
            if family == "jito" {
                Some(RouteSelection::only(
                    "client_aware_jito",
                    ["tpu_quic", "jito_bundle"],
                ))
            } else if family == "harmonic" {
                Some(RouteSelection::only(
                    "client_aware_harmonic",
                    ["tpu_quic", "harmonic_bundle"],
                ))
            } else {
                Some(RouteSelection::only("client_aware_tpu_only", ["tpu_quic"]))
            }
        }
    }
}

fn compute_unit_price_for_strategy(
    config: &BenchConfig,
    options: &LeaderPacedOptions,
    client_family: Option<&str>,
) -> u64 {
    if options.route_strategy == LeaderPacedRouteStrategy::ClientAware
        && client_family == Some("harmonic")
    {
        options
            .client_aware_harmonic_cu_price_microlamports
            .unwrap_or(config.compute_unit_price_microlamports)
    } else {
        config.compute_unit_price_microlamports
    }
}

fn current_leader_run(
    rpc: &RpcClient,
    current_slot: u64,
    lookbehind_slots: u64,
    lookahead_slots: u64,
) -> Result<LeaderRun> {
    let start_slot = current_slot.saturating_sub(lookbehind_slots);
    let limit = lookbehind_slots
        .saturating_add(lookahead_slots)
        .saturating_add(1)
        .max(1);
    let leaders = rpc
        .get_slot_leaders(start_slot, limit)
        .with_context(|| format!("get slot leaders start={start_slot} limit={limit}"))?;
    let current_index = current_slot.saturating_sub(start_slot) as usize;
    let leader = leaders
        .get(current_index)
        .with_context(|| format!("current slot {current_slot} missing from leader response"))?;

    let mut run_start_index = current_index;
    while run_start_index > 0 && leaders[run_start_index - 1] == *leader {
        run_start_index -= 1;
    }
    let mut run_end_index = current_index;
    while run_end_index + 1 < leaders.len() && leaders[run_end_index + 1] == *leader {
        run_end_index += 1;
    }

    Ok(LeaderRun {
        leader_identity: leader.to_string(),
        start_slot: start_slot + run_start_index as u64,
        end_slot: start_slot + run_end_index as u64,
    })
}

fn write_matched_observation_summary(
    run_dir: &std::path::Path,
    test_id: &str,
    samples: &[BenchSample],
    observations_path: &std::path::Path,
    min_sources: usize,
) -> Result<MatchedObservationSummary> {
    let submitted_at = samples.iter().fold(
        BTreeMap::<String, DateTime<Utc>>::new(),
        |mut map, sample| {
            map.entry(sample.signature.clone())
                .and_modify(|existing| {
                    if sample.client_started_at < *existing {
                        *existing = sample.client_started_at;
                    }
                })
                .or_insert(sample.client_started_at);
            map
        },
    );

    let file = File::open(observations_path)
        .with_context(|| format!("open {}", observations_path.display()))?;
    let mut matched_events = Vec::new();
    for (line_no, line) in BufReader::new(file).lines().enumerate() {
        let line = line.with_context(|| {
            format!(
                "read observation {}:{}",
                observations_path.display(),
                line_no + 1
            )
        })?;
        if line.trim().is_empty() {
            continue;
        }
        let mut event = serde_json::from_str::<ObservationEvent>(&line).with_context(|| {
            format!(
                "parse observation {}:{}",
                observations_path.display(),
                line_no + 1
            )
        })?;
        if let Some(started_at) = submitted_at.get(&event.signature) {
            event.submitted_at = Some(*started_at);
            matched_events.push(event);
        }
    }

    let matched_path = run_dir.join("matched-observations.ndjson");
    let mut matched_writer = BufWriter::new(
        File::create(&matched_path)
            .with_context(|| format!("create {}", matched_path.display()))?,
    );
    for event in &matched_events {
        serde_json::to_writer(&mut matched_writer, event).context("write matched observation")?;
        matched_writer
            .write_all(b"\n")
            .context("write matched observation newline")?;
    }
    matched_writer
        .flush()
        .context("flush matched observations")?;

    let summary = summarize_observations(test_id, &matched_events, min_sources);
    fs::write(
        run_dir.join("matched-observation-summary.json"),
        serde_json::to_vec_pretty(&summary)?,
    )
    .context("write matched observation summary json")?;
    fs::write(
        run_dir.join("matched-observation-summary.md"),
        observation_summary_markdown(&summary),
    )
    .context("write matched observation summary markdown")?;
    Ok(summary)
}

fn default_leader_paced_test_id() -> String {
    format!(
        "leader-paced-{}",
        Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
    )
    .replace(':', "")
}

fn redact_url(url: &str) -> String {
    if let Ok(mut parsed) = reqwest::Url::parse(url) {
        let _ = parsed.set_password(None);
        if !parsed.username().is_empty() {
            let _ = parsed.set_username("redacted");
        }
        let query = parsed.query().map(str::to_string);
        if query
            .as_deref()
            .map(|q| q.contains("api") || q.contains("key") || q.contains("token"))
            .unwrap_or(false)
        {
            parsed.set_query(Some("redacted=true"));
        }
        parsed.to_string()
    } else {
        url.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn client_aware_routes_jito_leaders_to_tpu_and_jito_bundle() {
        let selection =
            route_selection_for_strategy(LeaderPacedRouteStrategy::ClientAware, Some("jito"))
                .expect("selection");
        assert_eq!(selection.policy, "client_aware_jito");
        assert_eq!(selection.routes, vec!["tpu_quic", "jito_bundle"]);
    }

    #[test]
    fn client_aware_routes_harmonic_leaders_to_tpu_and_harmonic() {
        let selection =
            route_selection_for_strategy(LeaderPacedRouteStrategy::ClientAware, Some("harmonic"))
                .expect("selection");
        assert_eq!(selection.policy, "client_aware_harmonic");
        assert_eq!(selection.routes, vec!["tpu_quic", "harmonic_bundle"]);
    }

    #[test]
    fn client_aware_defaults_unknown_leaders_to_tpu_only() {
        let selection =
            route_selection_for_strategy(LeaderPacedRouteStrategy::ClientAware, Some("agave"))
                .expect("selection");
        assert_eq!(selection.policy, "client_aware_tpu_only");
        assert_eq!(selection.routes, vec!["tpu_quic"]);
    }

    #[test]
    fn leader_slots_artifact_lookup_normalizes_client_family() {
        let artifact = crate::leader_slots::LeaderSlotsSnapshotArtifact {
            schema_version: 1,
            fetched_at: Utc::now(),
            rpc_url_label: "https://rpc.rpcedge.com/".to_string(),
            start_slot: 10,
            limit: 2,
            response: json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": {
                    "success": true,
                    "data": [
                        {
                            "slot": 10,
                            "identity": "leader-a",
                            "client": {"family": "JitoLabs"}
                        }
                    ]
                }
            }),
        };
        let lookup = LeaderMetadataLookup::from_artifact(&artifact);
        assert_eq!(
            lookup.lookup(10, "leader-a").client_family.as_deref(),
            Some("jito")
        );
        assert_eq!(
            lookup.lookup(11, "leader-a").client_family.as_deref(),
            Some("jito")
        );
    }
}
