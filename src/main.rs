use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use solana_tx_bench::{run_benchmark, BenchConfig};
use std::{fs, path::PathBuf};

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
    }
    Ok(())
}

fn fmt(value: Option<u128>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-".to_string())
}
