use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use solana_tx_bench::{
    observation_summary_markdown, run_benchmark, summarize_observations, BenchConfig,
    ObservationEvent,
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
    InitConfig {
        #[arg(long, default_value = "bench.example.yaml")]
        output: PathBuf,
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
        Command::InitConfig { output } => {
            fs::write(&output, include_str!("../examples/bench.example.yaml"))
                .with_context(|| format!("write {}", output.display()))?;
            println!("wrote {}", output.display());
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
