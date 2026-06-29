use crate::{
    adapters::{build_adapter, SendContext},
    artifacts::{summarize, ArtifactWriter, BenchManifest, BenchSample, ManifestProvider},
    collectors::{collect_rpcedge_observations, CollectRunOutput, RpcEdgeCollectConfig},
    config::BenchConfig,
    observations::{
        observation_summary_markdown, summarize_observations, MatchedObservationSummary,
        ObservationEvent,
    },
    tx::{
        build_transaction_with_blockhash, estimated_transaction_spend, load_keypair, TxBuildConfig,
    },
};
use anyhow::{bail, Context, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use futures::future::join_all;
use serde::{Deserialize, Serialize};
use solana_client::rpc_client::RpcClient;
use solana_sdk::signer::Signer;
use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, File},
    io::{BufRead, BufReader, BufWriter, Write},
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Debug, Clone)]
pub struct LeaderPacedOptions {
    pub duration: Duration,
    pub txs_per_leader_run: usize,
    pub lookbehind_slots: u64,
    pub lookahead_slots: u64,
    pub poll_interval: Duration,
    pub observe_extra: Duration,
    pub rpcedge: Option<RpcEdgeLeaderCollector>,
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
    let tx_config = TxBuildConfig {
        rpc_url: config.rpc_url.clone(),
        lamports: config.lamports,
        compute_unit_limit: config.compute_unit_limit,
        compute_unit_price_microlamports: config.compute_unit_price_microlamports,
        memo_prefix: memo_prefix.clone(),
    };

    let rpc = RpcClient::new(config.rpc_url.clone());
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
            let blockhash = rpc.get_latest_blockhash().context("get latest blockhash")?;
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
                    schema_version: 1,
                    test_id: test_id.clone(),
                    iteration,
                    signature: tx.signature.to_string(),
                    leader_identity: leader_run.leader_identity.clone(),
                    leader_run_start_slot: leader_run.start_slot,
                    leader_run_end_slot: leader_run.end_slot,
                    send_slot,
                    sent_at,
                };
                serde_json::to_writer(&mut leader_sends, &event)
                    .context("write leader send event")?;
                leader_sends
                    .write_all(b"\n")
                    .context("write leader send newline")?;
                leader_sends.flush().context("flush leader send event")?;
                sends.push(event);

                let client_started_at = sent_at;
                let started = Instant::now();
                let ctx = SendContext {
                    test_id: test_id.clone(),
                    iteration,
                    signature: tx.signature.to_string(),
                    tx_base64: tx.base64.clone(),
                    timeout,
                };
                let futures = providers
                    .iter()
                    .map(|provider| provider.send_transaction(&tx.raw, &ctx));
                let acks = join_all(futures).await;
                let client_finished_at = Utc::now();
                let client_ack_latency_us = started.elapsed().as_micros();
                for ack in acks {
                    let sample = BenchSample::from_ack(
                        &test_id,
                        iteration,
                        tx.signature.to_string(),
                        client_started_at,
                        client_finished_at,
                        client_ack_latency_us,
                        ack,
                    );
                    writer.write_sample(&sample)?;
                    samples.push(sample);
                }
                iteration += 1;
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
    })
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
