use crate::observations::{
    observation_summary_markdown, summarize_observations, ObservationEvent, ObservationSourceKind,
};
use anyhow::{Context, Result};
use chrono::Utc;
use futures::StreamExt;
use serde_json::to_writer;
use std::{
    collections::HashMap,
    fs::{self, File},
    io::{BufWriter, Write},
    path::PathBuf,
    time::{Duration, Instant},
};
use tokio::{sync::mpsc, task::JoinHandle};
use yellowstone_grpc_client::{ClientTlsConfig, GeyserGrpcClient};
use yellowstone_grpc_proto::geyser::{
    subscribe_update::UpdateOneof, subscribe_update_deshred::UpdateOneof as DeshredUpdateOneof,
    CommitmentLevel, SubscribeDeshredRequest, SubscribeRequest,
    SubscribeRequestFilterDeshredTransactions, SubscribeRequestFilterTransactions,
};

const PROCESSED_SOURCE: &str = "rpcedge_processed";
const DESHRED_SOURCE: &str = "rpcedge_deshred";

#[derive(Debug, Clone)]
pub struct RpcEdgeCollectConfig {
    pub test_id: String,
    pub endpoint: String,
    pub x_token: Option<String>,
    pub output_dir: PathBuf,
    pub duration: Duration,
    pub account_include: Vec<String>,
    pub min_sources: usize,
}

#[derive(Debug)]
pub struct CollectRunOutput {
    pub test_id: String,
    pub output_dir: PathBuf,
    pub observations_path: PathBuf,
    pub summary_path: PathBuf,
    pub total_observations: usize,
    pub matched_signatures: usize,
}

pub async fn collect_rpcedge_observations(
    config: RpcEdgeCollectConfig,
) -> Result<CollectRunOutput> {
    fs::create_dir_all(&config.output_dir)
        .with_context(|| format!("create {}", config.output_dir.display()))?;
    let observations_path = config.output_dir.join("observations.ndjson");
    let mut writer = BufWriter::new(
        File::create(&observations_path)
            .with_context(|| format!("create {}", observations_path.display()))?,
    );

    let (tx, mut rx) = mpsc::channel::<ObservationEvent>(16_384);
    let processed_handle = spawn_processed_collector(&config, tx.clone());
    let deshred_handle = spawn_deshred_collector(&config, tx);

    let deadline = Instant::now() + config.duration;
    let mut events = Vec::new();
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match tokio::time::timeout(remaining, rx.recv()).await {
            Ok(Some(event)) => {
                to_writer(&mut writer, &event).context("write observation event")?;
                writer
                    .write_all(b"\n")
                    .context("write observation newline")?;
                events.push(event);
            }
            Ok(None) | Err(_) => break,
        }
    }
    writer.flush().context("flush observations")?;
    processed_handle.abort();
    deshred_handle.abort();

    let summary = summarize_observations(&config.test_id, &events, config.min_sources);
    let summary_json_path = config.output_dir.join("observation-summary.json");
    fs::write(&summary_json_path, serde_json::to_vec_pretty(&summary)?)
        .with_context(|| format!("write {}", summary_json_path.display()))?;
    let summary_path = config.output_dir.join("observation-summary.md");
    fs::write(&summary_path, observation_summary_markdown(&summary))
        .with_context(|| format!("write {}", summary_path.display()))?;

    Ok(CollectRunOutput {
        test_id: config.test_id,
        output_dir: config.output_dir,
        observations_path,
        summary_path,
        total_observations: summary.total_observations,
        matched_signatures: summary.matched_signatures,
    })
}

fn spawn_processed_collector(
    config: &RpcEdgeCollectConfig,
    tx: mpsc::Sender<ObservationEvent>,
) -> JoinHandle<()> {
    let config = config.clone();
    tokio::spawn(async move {
        loop {
            if let Err(error) = collect_processed_once(&config, tx.clone()).await {
                eprintln!("{PROCESSED_SOURCE} stream error: {error:#}; reconnecting");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    })
}

fn spawn_deshred_collector(
    config: &RpcEdgeCollectConfig,
    tx: mpsc::Sender<ObservationEvent>,
) -> JoinHandle<()> {
    let config = config.clone();
    tokio::spawn(async move {
        loop {
            if let Err(error) = collect_deshred_once(&config, tx.clone()).await {
                eprintln!("{DESHRED_SOURCE} stream error: {error:#}; reconnecting");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    })
}

async fn collect_processed_once(
    config: &RpcEdgeCollectConfig,
    tx: mpsc::Sender<ObservationEvent>,
) -> Result<()> {
    let mut client = connect_geyser(config).await?;
    let (_send, mut stream) = client
        .subscribe_with_request(Some(processed_request(&config.account_include)))
        .await
        .context("subscribe processed transactions")?;

    while let Some(update) = stream.next().await {
        let update = update.context("read processed update")?;
        let Some(UpdateOneof::Transaction(transaction)) = update.update_oneof else {
            continue;
        };
        let Some(info) = transaction.transaction else {
            continue;
        };
        tx.send(ObservationEvent {
            schema_version: 1,
            test_id: config.test_id.clone(),
            signature: bs58::encode(info.signature).into_string(),
            source_name: PROCESSED_SOURCE.to_string(),
            source_kind: ObservationSourceKind::YellowstoneProcessed,
            observed_at: Utc::now(),
            submitted_at: None,
            slot: Some(transaction.slot),
            slot_index: Some(info.index),
            source_sequence: None,
        })
        .await
        .context("send processed observation")?;
    }
    Ok(())
}

async fn collect_deshred_once(
    config: &RpcEdgeCollectConfig,
    tx: mpsc::Sender<ObservationEvent>,
) -> Result<()> {
    let mut client = connect_geyser(config).await?;
    let mut stream = client
        .subscribe_deshred_once(deshred_request(&config.account_include))
        .await
        .context("subscribe deshred transactions")?;

    while let Some(update) = stream.next().await {
        let update = update.context("read deshred update")?;
        let Some(DeshredUpdateOneof::DeshredTransaction(transaction)) = update.update_oneof else {
            continue;
        };
        let Some(info) = transaction.transaction else {
            continue;
        };
        tx.send(ObservationEvent {
            schema_version: 1,
            test_id: config.test_id.clone(),
            signature: bs58::encode(info.signature).into_string(),
            source_name: DESHRED_SOURCE.to_string(),
            source_kind: ObservationSourceKind::YellowstoneDeshred,
            observed_at: Utc::now(),
            submitted_at: None,
            slot: Some(transaction.slot),
            slot_index: None,
            source_sequence: None,
        })
        .await
        .context("send deshred observation")?;
    }
    Ok(())
}

pub(crate) async fn connect_geyser_endpoint(
    endpoint: &str,
    x_token: Option<&str>,
) -> Result<GeyserGrpcClient> {
    let mut builder =
        GeyserGrpcClient::build_from_shared(endpoint.to_string()).context("build geyser client")?;
    if let Some(token) = x_token {
        builder = builder
            .x_token(Some(token.to_string()))
            .context("configure x-token")?;
    }
    let mut builder = builder
        .max_decoding_message_size(134_217_728)
        .max_encoding_message_size(16_777_216)
        .http2_adaptive_window(true)
        .initial_connection_window_size(33_554_432)
        .initial_stream_window_size(33_554_432)
        .tcp_nodelay(true)
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(5));

    if endpoint.starts_with("https://") {
        builder = builder
            .tls_config(ClientTlsConfig::new().with_enabled_roots())
            .context("configure TLS")?;
    }
    builder.connect().await.context("connect geyser")
}

async fn connect_geyser(config: &RpcEdgeCollectConfig) -> Result<GeyserGrpcClient> {
    connect_geyser_endpoint(&config.endpoint, config.x_token.as_deref()).await
}

fn processed_request(account_include: &[String]) -> SubscribeRequest {
    let mut request = SubscribeRequest::default();
    request.transactions = HashMap::from([(
        "transactions".to_owned(),
        SubscribeRequestFilterTransactions {
            vote: Some(false),
            failed: Some(false),
            account_include: account_include.to_vec(),
            ..SubscribeRequestFilterTransactions::default()
        },
    )]);
    request.commitment = Some(CommitmentLevel::Processed as i32);
    request
}

fn deshred_request(account_include: &[String]) -> SubscribeDeshredRequest {
    let mut request = SubscribeDeshredRequest::default();
    request.deshred_transactions = HashMap::from([(
        "transactions".to_owned(),
        SubscribeRequestFilterDeshredTransactions {
            vote: Some(false),
            account_include: account_include.to_vec(),
            ..SubscribeRequestFilterDeshredTransactions::default()
        },
    )]);
    request
}
