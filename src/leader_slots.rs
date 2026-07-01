use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use reqwest::header::{HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{fs, path::Path};

#[derive(Debug, Clone)]
pub struct LeaderSlotsCaptureConfig {
    pub rpc_url: String,
    pub api_key: Option<String>,
    pub lookahead_slots: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LeaderSlotsSnapshotArtifact {
    pub schema_version: u32,
    pub fetched_at: DateTime<Utc>,
    pub rpc_url_label: String,
    pub start_slot: u64,
    pub limit: u64,
    pub response: Value,
}

pub async fn capture_leader_slots_snapshot(
    config: &LeaderSlotsCaptureConfig,
    start_slot: u64,
) -> Result<LeaderSlotsSnapshotArtifact> {
    if config.lookahead_slots == 0 {
        bail!("leader-slots lookahead must be greater than zero");
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .context("build getLeaderSlots HTTP client")?;
    let mut headers = HeaderMap::new();
    if let Some(api_key) = config.api_key.as_deref().filter(|value| !value.is_empty()) {
        headers.insert(
            "x-api-key",
            HeaderValue::from_str(api_key).context("build x-api-key header")?,
        );
    }

    let payload = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getLeaderSlots",
        "params": [start_slot, config.lookahead_slots],
    });
    let response = client
        .post(&config.rpc_url)
        .headers(headers)
        .json(&payload)
        .send()
        .await
        .with_context(|| format!("post getLeaderSlots to {}", redact_url(&config.rpc_url)))?;
    let status = response.status();
    let body = response
        .json::<Value>()
        .await
        .with_context(|| format!("decode getLeaderSlots response status={status}"))?;
    if !status.is_success() {
        bail!(
            "getLeaderSlots failed status={} body={}",
            status.as_u16(),
            body
        );
    }

    Ok(LeaderSlotsSnapshotArtifact {
        schema_version: 1,
        fetched_at: Utc::now(),
        rpc_url_label: redact_url(&config.rpc_url),
        start_slot,
        limit: config.lookahead_slots,
        response: body,
    })
}

pub fn write_leader_slots_snapshot(
    run_dir: &Path,
    artifact: &LeaderSlotsSnapshotArtifact,
) -> Result<()> {
    fs::write(
        run_dir.join("leader-slots-snapshot.json"),
        serde_json::to_vec_pretty(artifact)?,
    )
    .context("write leader-slots-snapshot.json")
}

fn redact_url(url: &str) -> String {
    if let Ok(mut parsed) = reqwest::Url::parse(url) {
        parsed.set_query(None);
        parsed.to_string()
    } else {
        url.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        matchers::{body_json, header, method, path},
        Mock, MockServer, ResponseTemplate,
    };

    #[tokio::test]
    async fn capture_posts_get_leader_slots_with_api_key_header() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/"))
            .and(header("x-api-key", "test-key"))
            .and(body_json(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "getLeaderSlots",
                "params": [100_u64, 32_u64],
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": {"success": true, "total": 0, "data": []}
            })))
            .mount(&server)
            .await;

        let artifact = capture_leader_slots_snapshot(
            &LeaderSlotsCaptureConfig {
                rpc_url: server.uri(),
                api_key: Some("test-key".to_string()),
                lookahead_slots: 32,
            },
            100,
        )
        .await
        .expect("capture");

        assert_eq!(artifact.start_slot, 100);
        assert_eq!(artifact.limit, 32);
        assert_eq!(artifact.response["result"]["success"], true);
    }
}
