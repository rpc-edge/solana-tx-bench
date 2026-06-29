use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::{DateTime, Utc};
use reqwest::StatusCode;
use rpcedge_relay_client::{QuicRelayClient, QuicRelayClientConfig};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{collections::HashMap, net::SocketAddr, time::Duration};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    SolanaRpc,
    RpcedgeRawHttp,
    RpcedgeRouteAwareHttp,
    RpcedgeQuicRawTx,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProviderConfig {
    SolanaRpc {
        endpoint: String,
        #[serde(default)]
        headers: HashMap<String, String>,
    },
    RpcedgeRawHttp {
        endpoint: String,
        #[serde(default)]
        api_key_env: Option<String>,
        #[serde(default)]
        headers: HashMap<String, String>,
    },
    RpcedgeRouteAwareHttp {
        endpoint: String,
        #[serde(default)]
        api_key_env: Option<String>,
        #[serde(default)]
        route_mode: RouteMode,
        #[serde(default)]
        routes: Vec<String>,
        #[serde(default)]
        headers: HashMap<String, String>,
    },
    RpcedgeQuicRawTx {
        endpoint: SocketAddr,
        api_key_env: String,
        #[serde(default)]
        server_name: Option<String>,
    },
}

impl ProviderConfig {
    pub fn kind(&self) -> ProviderKind {
        match self {
            Self::SolanaRpc { .. } => ProviderKind::SolanaRpc,
            Self::RpcedgeRawHttp { .. } => ProviderKind::RpcedgeRawHttp,
            Self::RpcedgeRouteAwareHttp { .. } => ProviderKind::RpcedgeRouteAwareHttp,
            Self::RpcedgeQuicRawTx { .. } => ProviderKind::RpcedgeQuicRawTx,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RouteMode {
    #[default]
    ServerDefault,
    Only,
    DefaultPlus,
    DefaultMinus,
}

impl RouteMode {
    fn as_wire(self) -> &'static str {
        match self {
            Self::ServerDefault => "server_default",
            Self::Only => "only",
            Self::DefaultPlus => "default_plus",
            Self::DefaultMinus => "default_minus",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SendContext {
    pub test_id: String,
    pub iteration: usize,
    pub signature: String,
    pub tx_base64: String,
    pub timeout: Duration,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderAck {
    pub provider_name: String,
    pub provider_kind: ProviderKind,
    pub accepted: bool,
    pub provider_request_id: Option<String>,
    pub returned_signature: Option<String>,
    pub status_code: Option<u16>,
    pub error_class: Option<String>,
    pub error: Option<String>,
    pub send_started_at: DateTime<Utc>,
    pub send_finished_at: DateTime<Utc>,
    pub ack_latency_us: u128,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderError {
    pub class: String,
    pub message: String,
    pub status_code: Option<u16>,
}

#[async_trait]
pub trait ProviderAdapter: Send + Sync {
    fn name(&self) -> &str;
    fn kind(&self) -> ProviderKind;
    async fn warmup(&mut self) -> Result<()> {
        Ok(())
    }
    async fn send_transaction(&self, tx: &[u8], ctx: &SendContext) -> ProviderAck;
}

pub fn build_adapter(
    name: String,
    config: ProviderConfig,
    timeout: Duration,
) -> Result<Box<dyn ProviderAdapter>> {
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .pool_idle_timeout(Duration::from_secs(30))
        .build()
        .context("build HTTP client")?;
    match config {
        ProviderConfig::SolanaRpc { endpoint, headers } => Ok(Box::new(JsonRpcAdapter {
            name,
            endpoint,
            headers,
            client,
        })),
        ProviderConfig::RpcedgeRawHttp {
            endpoint,
            api_key_env,
            headers,
        } => Ok(Box::new(RpcedgeRawHttpAdapter {
            name,
            endpoint,
            api_key: read_optional_secret(api_key_env)?,
            headers,
            client,
        })),
        ProviderConfig::RpcedgeRouteAwareHttp {
            endpoint,
            api_key_env,
            route_mode,
            routes,
            headers,
        } => Ok(Box::new(RpcedgeRouteAwareAdapter {
            name,
            endpoint,
            api_key: read_optional_secret(api_key_env)?,
            route_mode,
            routes,
            headers,
            client,
        })),
        ProviderConfig::RpcedgeQuicRawTx {
            endpoint,
            api_key_env,
            server_name,
        } => Ok(Box::new(RpcedgeQuicRawTxAdapter {
            name,
            endpoint,
            api_key: std::env::var(&api_key_env)
                .with_context(|| format!("read API key env var {api_key_env}"))?,
            server_name,
            timeout,
            client: None,
        })),
    }
}

fn read_optional_secret(env_var: Option<String>) -> Result<Option<String>> {
    env_var
        .map(|name| std::env::var(&name).with_context(|| format!("read API key env var {name}")))
        .transpose()
}

struct JsonRpcAdapter {
    name: String,
    endpoint: String,
    headers: HashMap<String, String>,
    client: reqwest::Client,
}

struct RpcedgeRawHttpAdapter {
    name: String,
    endpoint: String,
    api_key: Option<String>,
    headers: HashMap<String, String>,
    client: reqwest::Client,
}

struct RpcedgeRouteAwareAdapter {
    name: String,
    endpoint: String,
    api_key: Option<String>,
    route_mode: RouteMode,
    routes: Vec<String>,
    headers: HashMap<String, String>,
    client: reqwest::Client,
}

struct RpcedgeQuicRawTxAdapter {
    name: String,
    endpoint: SocketAddr,
    api_key: String,
    server_name: Option<String>,
    timeout: Duration,
    client: Option<QuicRelayClient>,
}

#[async_trait]
impl ProviderAdapter for JsonRpcAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> ProviderKind {
        ProviderKind::SolanaRpc
    }

    async fn send_transaction(&self, _tx: &[u8], ctx: &SendContext) -> ProviderAck {
        let body = json!({
            "jsonrpc": "2.0",
            "id": ctx.iteration,
            "method": "sendTransaction",
            "params": [
                ctx.tx_base64,
                {
                    "encoding": "base64",
                    "skipPreflight": true,
                    "maxRetries": 0
                }
            ]
        });
        send_json(
            &self.client,
            &self.endpoint,
            &self.headers,
            None,
            self.name(),
            self.kind(),
            ctx,
            body,
            parse_json_rpc_response,
        )
        .await
    }
}

#[async_trait]
impl ProviderAdapter for RpcedgeRawHttpAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> ProviderKind {
        ProviderKind::RpcedgeRawHttp
    }

    async fn send_transaction(&self, tx: &[u8], ctx: &SendContext) -> ProviderAck {
        let started = Utc::now();
        let timer = std::time::Instant::now();
        let mut request = self
            .client
            .post(&self.endpoint)
            .header("content-type", "application/octet-stream")
            .body(tx.to_vec());
        request = apply_headers(request, &self.headers, self.api_key.as_deref());
        finish_response(
            self.name(),
            self.kind(),
            started,
            timer,
            ctx,
            request.send().await,
        )
        .await
    }
}

#[async_trait]
impl ProviderAdapter for RpcedgeRouteAwareAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> ProviderKind {
        ProviderKind::RpcedgeRouteAwareHttp
    }

    async fn send_transaction(&self, _tx: &[u8], ctx: &SendContext) -> ProviderAck {
        let route_set = json!({
            "mode": self.route_mode.as_wire(),
            "routes": self.routes,
        });
        let body = json!({
            "jsonrpc": "2.0",
            "id": ctx.iteration,
            "method": "sendTransaction",
            "request_id": format!("{}-{}-{}", ctx.test_id, self.name, ctx.iteration),
            "params": {
                "transaction": ctx.tx_base64,
                "encoding": "base64",
            },
            "route_set": route_set,
        });
        send_json(
            &self.client,
            &self.endpoint,
            &self.headers,
            self.api_key.as_deref(),
            self.name(),
            self.kind(),
            ctx,
            body,
            parse_rpcedge_submit_response,
        )
        .await
    }
}

#[async_trait]
impl ProviderAdapter for RpcedgeQuicRawTxAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> ProviderKind {
        ProviderKind::RpcedgeQuicRawTx
    }

    async fn warmup(&mut self) -> Result<()> {
        let mut config = QuicRelayClientConfig::new(self.endpoint, self.api_key.clone())
            .with_timeout(self.timeout);
        if let Some(server_name) = self.server_name.as_ref() {
            config = config.with_server_name(server_name.clone());
        }
        let client = QuicRelayClient::connect(config)
            .await
            .map_err(|err| anyhow!("connect QUIC relay client: {err}"))?;
        self.client = Some(client);
        Ok(())
    }

    async fn send_transaction(&self, tx: &[u8], _ctx: &SendContext) -> ProviderAck {
        let started = Utc::now();
        let timer = std::time::Instant::now();
        let Some(client) = self.client.as_ref() else {
            return ack_error(
                self.name(),
                self.kind(),
                started,
                timer,
                None,
                "invalid_config",
                "QUIC client was not warmed up",
            );
        };

        match client.send_transaction_raw_bytes(tx).await {
            Ok(response) => ProviderAck {
                provider_name: self.name().to_string(),
                provider_kind: self.kind(),
                accepted: response.accepted,
                provider_request_id: Some(response.request_id),
                returned_signature: Some(response.signature),
                status_code: None,
                error_class: None,
                error: None,
                send_started_at: started,
                send_finished_at: Utc::now(),
                ack_latency_us: timer.elapsed().as_micros(),
            },
            Err(err) => ack_error(
                self.name(),
                self.kind(),
                started,
                timer,
                None,
                "transport_error",
                err,
            ),
        }
    }
}

async fn send_json<F>(
    client: &reqwest::Client,
    endpoint: &str,
    headers: &HashMap<String, String>,
    api_key: Option<&str>,
    name: &str,
    kind: ProviderKind,
    ctx: &SendContext,
    body: serde_json::Value,
    parser: F,
) -> ProviderAck
where
    F: Fn(
        StatusCode,
        &serde_json::Value,
        &SendContext,
    ) -> std::result::Result<(bool, Option<String>, Option<String>), ProviderError>,
{
    let started = Utc::now();
    let timer = std::time::Instant::now();
    let mut request = client.post(endpoint).json(&body);
    request = apply_headers(request, headers, api_key);
    match request.send().await {
        Ok(response) => {
            let status = response.status();
            match response.json::<serde_json::Value>().await {
                Ok(value) => {
                    let finished = Utc::now();
                    let latency = timer.elapsed().as_micros();
                    match parser(status, &value, ctx) {
                        Ok((accepted, provider_request_id, returned_signature)) => ProviderAck {
                            provider_name: name.to_string(),
                            provider_kind: kind,
                            accepted,
                            provider_request_id,
                            returned_signature,
                            status_code: Some(status.as_u16()),
                            error_class: None,
                            error: None,
                            send_started_at: started,
                            send_finished_at: finished,
                            ack_latency_us: latency,
                        },
                        Err(err) => ProviderAck {
                            provider_name: name.to_string(),
                            provider_kind: kind,
                            accepted: false,
                            provider_request_id: None,
                            returned_signature: None,
                            status_code: err.status_code.or(Some(status.as_u16())),
                            error_class: Some(err.class),
                            error: Some(err.message),
                            send_started_at: started,
                            send_finished_at: finished,
                            ack_latency_us: latency,
                        },
                    }
                }
                Err(err) => ack_error(
                    name,
                    kind,
                    started,
                    timer,
                    Some(status.as_u16()),
                    "decode_error",
                    err,
                ),
            }
        }
        Err(err) => ack_error(name, kind, started, timer, None, "transport_error", err),
    }
}

async fn finish_response(
    name: &str,
    kind: ProviderKind,
    started: DateTime<Utc>,
    timer: std::time::Instant,
    ctx: &SendContext,
    response: Result<reqwest::Response, reqwest::Error>,
) -> ProviderAck {
    match response {
        Ok(response) => {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            let finished = Utc::now();
            let latency = timer.elapsed().as_micros();
            if status.is_success() {
                let parsed = serde_json::from_str::<serde_json::Value>(&body).ok();
                let returned_signature = parsed
                    .as_ref()
                    .and_then(|v| v.get("signature"))
                    .and_then(|v| v.as_str())
                    .or_else(|| {
                        parsed
                            .as_ref()
                            .and_then(|v| v.get("result"))
                            .and_then(|v| v.as_str())
                    })
                    .map(ToOwned::to_owned)
                    .or_else(|| Some(ctx.signature.clone()));
                let provider_request_id = parsed
                    .as_ref()
                    .and_then(|v| v.get("request_id"))
                    .and_then(|v| v.as_str())
                    .map(ToOwned::to_owned);
                ProviderAck {
                    provider_name: name.to_string(),
                    provider_kind: kind,
                    accepted: true,
                    provider_request_id,
                    returned_signature,
                    status_code: Some(status.as_u16()),
                    error_class: None,
                    error: None,
                    send_started_at: started,
                    send_finished_at: finished,
                    ack_latency_us: latency,
                }
            } else {
                ProviderAck {
                    provider_name: name.to_string(),
                    provider_kind: kind,
                    accepted: false,
                    provider_request_id: None,
                    returned_signature: None,
                    status_code: Some(status.as_u16()),
                    error_class: Some("provider_error".to_string()),
                    error: Some(body),
                    send_started_at: started,
                    send_finished_at: finished,
                    ack_latency_us: latency,
                }
            }
        }
        Err(err) => ack_error(name, kind, started, timer, None, "transport_error", err),
    }
}

fn ack_error<E: std::fmt::Display>(
    name: &str,
    kind: ProviderKind,
    started: DateTime<Utc>,
    timer: std::time::Instant,
    status_code: Option<u16>,
    class: &str,
    err: E,
) -> ProviderAck {
    ProviderAck {
        provider_name: name.to_string(),
        provider_kind: kind,
        accepted: false,
        provider_request_id: None,
        returned_signature: None,
        status_code,
        error_class: Some(class.to_string()),
        error: Some(err.to_string()),
        send_started_at: started,
        send_finished_at: Utc::now(),
        ack_latency_us: timer.elapsed().as_micros(),
    }
}

fn apply_headers(
    mut request: reqwest::RequestBuilder,
    headers: &HashMap<String, String>,
    api_key: Option<&str>,
) -> reqwest::RequestBuilder {
    for (key, value) in headers {
        request = request.header(key, value);
    }
    if let Some(api_key) = api_key {
        request = request.header("x-api-key", api_key);
    }
    request
}

fn parse_json_rpc_response(
    status: StatusCode,
    value: &serde_json::Value,
    _ctx: &SendContext,
) -> std::result::Result<(bool, Option<String>, Option<String>), ProviderError> {
    if !status.is_success() {
        return Err(provider_error(status, "provider_error", value));
    }
    if let Some(error) = value.get("error") {
        return Err(ProviderError {
            class: "provider_error".to_string(),
            message: error.to_string(),
            status_code: Some(status.as_u16()),
        });
    }
    let signature = value
        .get("result")
        .and_then(|v| v.as_str())
        .map(ToOwned::to_owned)
        .ok_or_else(|| ProviderError {
            class: "missing_signature".to_string(),
            message: format!("JSON-RPC response did not include result signature: {value}"),
            status_code: Some(status.as_u16()),
        })?;
    Ok((true, None, Some(signature)))
}

fn parse_rpcedge_submit_response(
    status: StatusCode,
    value: &serde_json::Value,
    ctx: &SendContext,
) -> std::result::Result<(bool, Option<String>, Option<String>), ProviderError> {
    if !status.is_success() {
        return Err(provider_error(status, "provider_error", value));
    }
    let accepted = value
        .get("accepted")
        .and_then(|v| v.as_bool())
        .unwrap_or(status.is_success());
    let provider_request_id = value
        .get("request_id")
        .and_then(|v| v.as_str())
        .map(ToOwned::to_owned);
    let signature = value
        .get("signature")
        .and_then(|v| v.as_str())
        .map(ToOwned::to_owned)
        .or_else(|| Some(ctx.signature.clone()));
    Ok((accepted, provider_request_id, signature))
}

fn provider_error(status: StatusCode, class: &str, value: &serde_json::Value) -> ProviderError {
    ProviderError {
        class: class.to_string(),
        message: value.to_string(),
        status_code: Some(status.as_u16()),
    }
}

pub fn base64_tx(tx: &[u8]) -> String {
    STANDARD.encode(tx)
}

pub fn decode_tx_base64(encoded: &str) -> Result<Vec<u8>> {
    STANDARD
        .decode(encoded)
        .map_err(|err| anyhow!("decode base64 transaction: {err}"))
}
