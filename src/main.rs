use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use solana_tx_bench::{
    collect_rpcedge_observations, generate_comparison, generate_report,
    observation_summary_markdown, run_benchmark, run_leader_paced, summarize_observations,
    BenchConfig, CompareOptions, LeaderPacedOptions, LeaderPacedRouteStrategy, LeaderPacedTrigger,
    LeaderSlotsCaptureConfig, ObservationEvent, RpcEdgeCollectConfig, RpcEdgeLeaderCollector,
};
use std::{
    fs,
    io::{BufRead, BufReader},
    path::PathBuf,
};

#[derive(Debug, Parser)]
#[command(name = "solana-tx-bench")]
#[command(about = "Reusable Solana transaction sender benchmark")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Run {
        #[arg(long)]
        config: PathBuf,
    },
    RunLeaderPaced {
        #[arg(long)]
        config: PathBuf,
        #[arg(long, default_value_t = 300)]
        duration_seconds: u64,
        #[arg(long, default_value_t = 1)]
        txs_per_leader_run: usize,
        #[arg(long, default_value_t = 1)]
        leader_run_concurrency: usize,
        #[arg(long, default_value_t = 12)]
        lookbehind_slots: u64,
        #[arg(long, default_value_t = 64)]
        lookahead_slots: u64,
        #[arg(long, default_value_t = 150)]
        poll_ms: u64,
        #[arg(long, default_value = "grpc_slot")]
        slot_trigger: String,
        #[arg(long, env = "RPCEDGE_GRPC_URL")]
        slot_signal_endpoint: Option<String>,
        #[arg(long)]
        collect_rpcedge: bool,
        #[arg(long, env = "RPCEDGE_GRPC_URL")]
        rpcedge_endpoint: Option<String>,
        #[arg(long, env = "YELLOWSTONE_X_TOKEN")]
        x_token: Option<String>,
        #[arg(long, default_value_t = 30)]
        observe_extra_seconds: u64,
        #[arg(long, default_value_t = 2)]
        min_sources: usize,
        #[arg(long)]
        capture_leader_slots: bool,
        #[arg(long, env = "LEADER_SLOTS_RPC_URL")]
        leader_slots_rpc_url: Option<String>,
        #[arg(long, env = "RPCEDGE_API_KEY")]
        leader_slots_api_key: Option<String>,
        #[arg(long, default_value_t = 512)]
        leader_slots_lookahead: u64,
        #[arg(long, default_value = "static")]
        route_strategy: String,
        #[arg(long)]
        client_aware_harmonic_cu_price_microlamports: Option<u64>,
    },
    InitConfig {
        #[arg(long, default_value = "bench.example.yaml")]
        output: PathBuf,
    },
    CollectRpcedge {
        #[arg(long)]
        test_id: Option<String>,
        #[arg(long, env = "RPCEDGE_GRPC_URL")]
        endpoint: String,
        #[arg(long, env = "YELLOWSTONE_X_TOKEN")]
        x_token: Option<String>,
        #[arg(long)]
        output_dir: Option<PathBuf>,
        #[arg(long, default_value_t = 120)]
        duration_seconds: u64,
        #[arg(long, value_delimiter = ',')]
        account_include: Vec<String>,
        #[arg(long, default_value_t = 2)]
        min_sources: usize,
    },
    SummarizeObservations {
        #[arg(long)]
        test_id: String,
        #[arg(long, required = true)]
        input: Vec<PathBuf>,
        #[arg(long)]
        output_dir: PathBuf,
        #[arg(long, default_value_t = 2)]
        min_sources: usize,
    },
    Report {
        #[arg(long)]
        artifact_dir: PathBuf,
    },
    Compare {
        #[arg(long = "artifact-dir", required = true)]
        artifact_dirs: Vec<PathBuf>,
        #[arg(long = "label")]
        labels: Vec<String>,
        #[arg(long)]
        output_dir: PathBuf,
        #[arg(long, default_value = "rpcedge_processed")]
        primary_source: String,
        #[arg(long, default_value = "Transaction Landing Comparison")]
        title: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let _ = dotenvy::dotenv();
    let cli = Cli::parse();
    match cli.command {
        Command::Run { config } => {
            let text = fs::read_to_string(&config)
                .with_context(|| format!("read {}", config.display()))?;
            let config: BenchConfig = serde_yaml::from_str(&text)
                .with_context(|| format!("parse {}", config.display()))?;
            let output = run_benchmark(config).await?;
            println!("test_id={}", output.test_id);
            println!("run_dir={}", output.run_dir.display());
            println!("provider\tcount\taccepted\terrors\tp50_us\tp90_us\tp99_us\tmax_us");
            for item in &output.summary.provider_summaries {
                println!(
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    item.provider_name,
                    item.count,
                    item.accepted,
                    item.errors,
                    fmt(item.p50_us),
                    fmt(item.p90_us),
                    fmt(item.p99_us),
                    fmt(item.max_us),
                );
            }
        }
        Command::RunLeaderPaced {
            config,
            duration_seconds,
            txs_per_leader_run,
            leader_run_concurrency,
            lookbehind_slots,
            lookahead_slots,
            poll_ms,
            slot_trigger,
            slot_signal_endpoint,
            collect_rpcedge,
            rpcedge_endpoint,
            x_token,
            observe_extra_seconds,
            min_sources,
            capture_leader_slots,
            leader_slots_rpc_url,
            leader_slots_api_key,
            leader_slots_lookahead,
            route_strategy,
            client_aware_harmonic_cu_price_microlamports,
        } => {
            let text = fs::read_to_string(&config)
                .with_context(|| format!("read {}", config.display()))?;
            let config: BenchConfig = serde_yaml::from_str(&text)
                .with_context(|| format!("parse {}", config.display()))?;
            let trigger = parse_leader_paced_trigger(
                &slot_trigger,
                slot_signal_endpoint.or_else(|| rpcedge_endpoint.clone()),
                x_token.clone(),
            )?;
            let rpcedge = if collect_rpcedge {
                Some(RpcEdgeLeaderCollector {
                    endpoint: rpcedge_endpoint.context(
                        "--rpcedge-endpoint or RPCEDGE_GRPC_URL is required with --collect-rpcedge",
                    )?,
                    x_token,
                    min_sources,
                })
            } else {
                None
            };
            let leader_slots = capture_leader_slots.then(|| LeaderSlotsCaptureConfig {
                rpc_url: leader_slots_rpc_url.unwrap_or_else(|| config.rpc_url.clone()),
                api_key: leader_slots_api_key,
                lookahead_slots: leader_slots_lookahead,
            });
            let output = run_leader_paced(
                config,
                LeaderPacedOptions {
                    duration: std::time::Duration::from_secs(duration_seconds),
                    txs_per_leader_run,
                    leader_run_concurrency,
                    lookbehind_slots,
                    lookahead_slots,
                    poll_interval: std::time::Duration::from_millis(poll_ms),
                    observe_extra: std::time::Duration::from_secs(observe_extra_seconds),
                    rpcedge,
                    leader_slots,
                    route_strategy: parse_route_strategy(&route_strategy)?,
                    client_aware_harmonic_cu_price_microlamports,
                    trigger,
                },
            )
            .await?;
            println!("test_id={}", output.test_id);
            println!("run_dir={}", output.run_dir.display());
            println!("sent_transactions={}", output.sent_transactions);
            println!("provider_samples={}", output.provider_samples);
            if let Some(collector) = &output.collector {
                println!("rpcedge_observations={}", collector.total_observations);
                println!(
                    "rpcedge_matched_signatures={}",
                    collector.matched_signatures
                );
            }
            if let Some(summary) = &output.matched_observation_summary {
                println!(
                    "matched_observation_signatures={}",
                    summary.matched_signatures
                );
                println!(
                    "matched_observation_summary={}",
                    output
                        .run_dir
                        .join("matched-observation-summary.md")
                        .display()
                );
            }
            if let Some(path) = &output.leader_slots_snapshot_path {
                println!("leader_slots_snapshot={}", path.display());
            }
        }
        Command::InitConfig { output } => {
            fs::write(&output, include_str!("../examples/bench.example.yaml"))
                .with_context(|| format!("write {}", output.display()))?;
            println!("wrote {}", output.display());
        }
        Command::CollectRpcedge {
            test_id,
            endpoint,
            x_token,
            output_dir,
            duration_seconds,
            account_include,
            min_sources,
        } => {
            let test_id = test_id.unwrap_or_else(|| {
                format!(
                    "rpcedge-observe-{}",
                    chrono::Utc::now().format("%Y%m%dT%H%M%SZ")
                )
            });
            let output_dir =
                output_dir.unwrap_or_else(|| PathBuf::from("artifacts").join(&test_id));
            let output = collect_rpcedge_observations(RpcEdgeCollectConfig {
                test_id,
                endpoint,
                x_token,
                output_dir,
                duration: std::time::Duration::from_secs(duration_seconds),
                account_include,
                min_sources,
            })
            .await?;
            println!("test_id={}", output.test_id);
            println!("output_dir={}", output.output_dir.display());
            println!("observations={}", output.total_observations);
            println!("matched_signatures={}", output.matched_signatures);
            println!("observations_file={}", output.observations_path.display());
            println!("summary={}", output.summary_path.display());
        }
        Command::SummarizeObservations {
            test_id,
            input,
            output_dir,
            min_sources,
        } => {
            let events = read_observation_events(&input)?;
            let summary = summarize_observations(&test_id, &events, min_sources);
            fs::create_dir_all(&output_dir)
                .with_context(|| format!("create {}", output_dir.display()))?;
            fs::write(
                output_dir.join("observation-summary.json"),
                serde_json::to_vec_pretty(&summary)?,
            )
            .with_context(|| format!("write {}", output_dir.display()))?;
            fs::write(
                output_dir.join("observation-summary.md"),
                observation_summary_markdown(&summary),
            )
            .with_context(|| format!("write {}", output_dir.display()))?;
            println!("observations={}", summary.total_observations);
            println!("matched_signatures={}", summary.matched_signatures);
            println!(
                "summary={}",
                output_dir.join("observation-summary.md").display()
            );
        }
        Command::Report { artifact_dir } => {
            let report = generate_report(&artifact_dir)?;
            println!("test_id={}", report.test_id);
            println!("sent_transactions={}", report.totals.sent_transactions);
            println!("matched_signatures={}", report.totals.matched_signatures);
            println!("report_json={}", artifact_dir.join("report.json").display());
            println!("report_md={}", artifact_dir.join("report.md").display());
            println!("report_html={}", artifact_dir.join("report.html").display());
        }
        Command::Compare {
            artifact_dirs,
            labels,
            output_dir,
            primary_source,
            title,
        } => {
            let comparison = generate_comparison(CompareOptions {
                artifact_dirs,
                labels,
                output_dir: output_dir.clone(),
                primary_source,
                title,
            })?;
            println!("title={}", comparison.title);
            println!("runs={}", comparison.run_count);
            println!(
                "comparison_json={}",
                output_dir.join("comparison.json").display()
            );
            println!(
                "comparison_md={}",
                output_dir.join("comparison.md").display()
            );
            println!(
                "comparison_html={}",
                output_dir.join("index.html").display()
            );
        }
    }
    Ok(())
}

fn read_observation_events(paths: &[PathBuf]) -> Result<Vec<ObservationEvent>> {
    let mut events = Vec::new();
    for path in paths {
        let file = fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
        let reader = BufReader::new(file);
        for (line_no, line) in reader.lines().enumerate() {
            let line = line.with_context(|| format!("read {}:{}", path.display(), line_no + 1))?;
            if line.trim().is_empty() {
                continue;
            }
            let event = serde_json::from_str::<ObservationEvent>(&line)
                .with_context(|| format!("parse {}:{}", path.display(), line_no + 1))?;
            events.push(event);
        }
    }
    Ok(events)
}

fn fmt(value: Option<u128>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-".to_string())
}

fn parse_route_strategy(raw: &str) -> Result<LeaderPacedRouteStrategy> {
    match raw.trim() {
        "static" => Ok(LeaderPacedRouteStrategy::Static),
        "client_aware" | "client-aware" => Ok(LeaderPacedRouteStrategy::ClientAware),
        other => {
            anyhow::bail!("unknown route strategy `{other}`; expected `static` or `client_aware`")
        }
    }
}

fn parse_leader_paced_trigger(
    raw: &str,
    endpoint: Option<String>,
    x_token: Option<String>,
) -> Result<LeaderPacedTrigger> {
    match raw.trim() {
        "rpc_poll" | "rpc-poll" | "poll" => Ok(LeaderPacedTrigger::RpcPoll),
        "grpc_slot" | "grpc-slot" | "grpc" => {
            let endpoint = endpoint.context(
                "--slot-signal-endpoint or RPCEDGE_GRPC_URL is required with --slot-trigger grpc_slot",
            )?;
            Ok(LeaderPacedTrigger::GrpcSlot(
                solana_tx_bench::slot_signal::GrpcSlotSignalConfig { endpoint, x_token },
            ))
        }
        other => {
            anyhow::bail!("unknown slot trigger `{other}`; expected `grpc_slot` or `rpc_poll`")
        }
    }
}
