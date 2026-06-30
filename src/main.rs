use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use solana_tx_bench::{
    collect_rpcedge_observations, observation_summary_markdown, run_benchmark, run_leader_paced,
    summarize_observations, BenchConfig, LeaderPacedOptions, ObservationEvent,
    RpcEdgeCollectConfig, RpcEdgeLeaderCollector,
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
        #[arg(long, default_value_t = 12)]
        lookbehind_slots: u64,
        #[arg(long, default_value_t = 64)]
        lookahead_slots: u64,
        #[arg(long, default_value_t = 150)]
        poll_ms: u64,
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
            lookbehind_slots,
            lookahead_slots,
            poll_ms,
            collect_rpcedge,
            rpcedge_endpoint,
            x_token,
            observe_extra_seconds,
            min_sources,
        } => {
            let text = fs::read_to_string(&config)
                .with_context(|| format!("read {}", config.display()))?;
            let config: BenchConfig = serde_yaml::from_str(&text)
                .with_context(|| format!("parse {}", config.display()))?;
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
            let output = run_leader_paced(
                config,
                LeaderPacedOptions {
                    duration: std::time::Duration::from_secs(duration_seconds),
                    txs_per_leader_run,
                    lookbehind_slots,
                    lookahead_slots,
                    poll_interval: std::time::Duration::from_millis(poll_ms),
                    observe_extra: std::time::Duration::from_secs(observe_extra_seconds),
                    rpcedge,
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
