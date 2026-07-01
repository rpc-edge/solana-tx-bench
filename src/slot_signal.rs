use crate::collectors::connect_geyser_endpoint;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, time::Duration};
use tokio::{sync::mpsc, task::JoinHandle};
use yellowstone_grpc_proto::geyser::{
    subscribe_update::UpdateOneof, CommitmentLevel, SlotStatus, SubscribeRequest,
    SubscribeRequestFilterSlots,
};

#[derive(Debug, Clone)]
pub struct GrpcSlotSignalConfig {
    pub endpoint: String,
    pub x_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SlotSignal {
    pub slot: u64,
    pub status: String,
    pub observed_at: DateTime<Utc>,
}

pub fn spawn_grpc_slot_signal(config: GrpcSlotSignalConfig) -> mpsc::Receiver<SlotSignal> {
    let (tx, rx) = mpsc::channel(16_384);
    spawn_slot_signal_task(config, tx);
    rx
}

fn spawn_slot_signal_task(
    config: GrpcSlotSignalConfig,
    tx: mpsc::Sender<SlotSignal>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            if let Err(error) = collect_slots_once(&config, tx.clone()).await {
                eprintln!("rpcedge_slot stream error: {error:#}; reconnecting");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    })
}

async fn collect_slots_once(
    config: &GrpcSlotSignalConfig,
    tx: mpsc::Sender<SlotSignal>,
) -> Result<()> {
    let mut client = connect_geyser_endpoint(&config.endpoint, config.x_token.as_deref()).await?;
    let (_send, mut stream) = client
        .subscribe_with_request(Some(slot_request()))
        .await
        .context("subscribe slot updates")?;

    while let Some(update) = stream.next().await {
        let update = update.context("read slot update")?;
        let Some(UpdateOneof::Slot(slot)) = update.update_oneof else {
            continue;
        };
        let Ok(slot_status) = SlotStatus::try_from(slot.status) else {
            continue;
        };
        if !matches!(
            slot_status,
            SlotStatus::SlotFirstShredReceived | SlotStatus::SlotCreatedBank
        ) {
            continue;
        }
        let status = slot_status.as_str_name().to_string();
        tx.send(SlotSignal {
            slot: slot.slot,
            status,
            observed_at: Utc::now(),
        })
        .await
        .context("send slot signal")?;
    }
    Ok(())
}

fn slot_request() -> SubscribeRequest {
    let mut request = SubscribeRequest::default();
    request.slots = HashMap::from([(
        "slots".to_string(),
        SubscribeRequestFilterSlots {
            filter_by_commitment: Some(false),
            interslot_updates: Some(true),
        },
    )]);
    request.commitment = Some(CommitmentLevel::Processed as i32);
    request
}
