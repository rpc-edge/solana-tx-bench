use crate::{
    artifacts::BenchSample, leader_paced::LeaderSendEvent,
    leader_slots::LeaderSlotsSnapshotArtifact, observations::ObservationEvent,
};
use anyhow::{Context, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportOutput {
    pub schema_version: u32,
    pub test_id: String,
    pub generated_at: DateTime<Utc>,
    pub artifact_dir: String,
    pub totals: ReportTotals,
    pub provider_summaries: Vec<ProviderReportSummary>,
    pub source_summaries: Vec<SourceReportSummary>,
    pub cohort_summaries: Vec<CohortReportSummary>,
    pub tail_events: Vec<TailEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportTotals {
    pub samples: usize,
    pub sent_transactions: usize,
    pub observations: usize,
    pub matched_signatures: usize,
    pub leader_slot_snapshots: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderReportSummary {
    pub provider_name: String,
    pub route_policy: String,
    pub selected_routes: Vec<String>,
    pub count: usize,
    pub accepted: usize,
    pub errors: usize,
    pub ack_p50_ms: Option<f64>,
    pub ack_p90_ms: Option<f64>,
    pub ack_p99_ms: Option<f64>,
    pub ack_max_ms: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceReportSummary {
    pub source_name: String,
    pub count: usize,
    pub first_seen: usize,
    pub submit_to_observed_p50_ms: Option<f64>,
    pub submit_to_observed_p90_ms: Option<f64>,
    pub submit_to_observed_p99_ms: Option<f64>,
    pub submit_to_observed_max_ms: Option<f64>,
    pub same_slot_count: usize,
    pub same_slot_rate: f64,
    pub landed_slot_delta_p50: Option<i64>,
    pub landed_slot_delta_p90: Option<i64>,
    pub slot_index_p50: Option<u64>,
    pub slot_index_p90: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CohortReportSummary {
    pub cohort_kind: String,
    pub cohort_value: String,
    pub source_name: String,
    pub count: usize,
    pub submit_to_observed_p50_ms: Option<f64>,
    pub submit_to_observed_p90_ms: Option<f64>,
    pub submit_to_observed_p99_ms: Option<f64>,
    pub submit_to_observed_max_ms: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TailEvent {
    pub signature: String,
    pub source_name: String,
    pub latency_ms: f64,
    pub send_slot: Option<u64>,
    pub landed_slot: Option<u64>,
    pub slot_index: Option<u64>,
    pub leader_identity: String,
    pub leader_region: String,
    pub leader_city: String,
    pub leader_client: String,
}

#[derive(Debug, Clone, Default)]
struct LeaderMeta {
    identity: String,
    region: String,
    city: String,
    data_center_key: String,
    client_family: String,
    client_software: String,
    stake_bucket: String,
}

#[derive(Debug, Clone)]
struct ObservationRow {
    event: ObservationEvent,
    send: Option<LeaderSendEvent>,
    latency_us: Option<i128>,
    leader: LeaderMeta,
}

pub fn generate_report(artifact_dir: &Path) -> Result<ReportOutput> {
    let samples: Vec<BenchSample> = read_ndjson(&artifact_dir.join("samples.ndjson"))?;
    let sends: Vec<LeaderSendEvent> = read_ndjson(&artifact_dir.join("leader-sends.ndjson"))?;
    let observations: Vec<ObservationEvent> =
        read_ndjson(&artifact_dir.join("matched-observations.ndjson"))?;
    let manifest = read_manifest(artifact_dir)?;
    let snapshots = read_leader_slot_snapshots(artifact_dir)?;

    let test_id = samples
        .first()
        .map(|sample| sample.test_id.clone())
        .or_else(|| sends.first().map(|send| send.test_id.clone()))
        .or_else(|| observations.first().map(|event| event.test_id.clone()))
        .unwrap_or_else(|| {
            artifact_dir
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("unknown")
                .to_string()
        });

    let leader_by_slot = build_leader_lookup(&snapshots);
    let send_by_signature = sends
        .iter()
        .cloned()
        .map(|send| (send.signature.clone(), send))
        .collect::<BTreeMap<_, _>>();

    let rows = observations
        .iter()
        .cloned()
        .map(|event| {
            let send = send_by_signature.get(&event.signature).cloned();
            let leader = event
                .slot
                .and_then(|slot| leader_by_slot.get(&slot).cloned())
                .or_else(|| {
                    send.as_ref().and_then(|send| {
                        leader_by_slot
                            .get(&send.send_slot)
                            .cloned()
                            .or_else(|| leader_by_slot.get(&send.leader_run_start_slot).cloned())
                    })
                })
                .unwrap_or_else(|| {
                    let mut meta = LeaderMeta::default();
                    if let Some(send) = &send {
                        meta.identity = send.leader_identity.clone();
                        meta.client_family = send
                            .leader_client_family
                            .clone()
                            .unwrap_or_else(|| "unknown".to_string());
                    }
                    normalize_meta(meta)
                });
            let latency_us = event.submitted_at.and_then(|submitted_at| {
                event
                    .observed_at
                    .signed_duration_since(submitted_at)
                    .num_microseconds()
                    .map(|value| value as i128)
            });
            ObservationRow {
                event,
                send,
                latency_us,
                leader,
            }
        })
        .collect::<Vec<_>>();

    let report = ReportOutput {
        schema_version: 1,
        test_id,
        generated_at: Utc::now(),
        artifact_dir: artifact_dir.display().to_string(),
        totals: ReportTotals {
            samples: samples.len(),
            sent_transactions: sends.len(),
            observations: observations.len(),
            matched_signatures: observations
                .iter()
                .map(|event| event.signature.as_str())
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            leader_slot_snapshots: snapshots.len(),
        },
        provider_summaries: summarize_providers(&samples, manifest.as_ref()),
        source_summaries: summarize_sources(&rows),
        cohort_summaries: summarize_cohorts(&rows),
        tail_events: summarize_tail_events(&rows, 12),
    };

    fs::write(
        artifact_dir.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )
    .with_context(|| format!("write {}", artifact_dir.join("report.json").display()))?;
    fs::write(artifact_dir.join("report.md"), report_markdown(&report))
        .with_context(|| format!("write {}", artifact_dir.join("report.md").display()))?;
    fs::write(artifact_dir.join("report.html"), report_html(&report))
        .with_context(|| format!("write {}", artifact_dir.join("report.html").display()))?;

    Ok(report)
}

fn read_ndjson<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Vec<T>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut rows = Vec::new();
    for (line_no, line) in BufReader::new(file).lines().enumerate() {
        let line = line.with_context(|| format!("read {}:{}", path.display(), line_no + 1))?;
        if line.trim().is_empty() {
            continue;
        }
        rows.push(
            serde_json::from_str::<T>(&line)
                .with_context(|| format!("parse {}:{}", path.display(), line_no + 1))?,
        );
    }
    Ok(rows)
}

fn read_leader_slot_snapshots(artifact_dir: &Path) -> Result<Vec<LeaderSlotsSnapshotArtifact>> {
    let mut paths = fs::read_dir(artifact_dir)
        .with_context(|| format!("read {}", artifact_dir.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| {
                    name == "leader-slots-snapshot.json"
                        || name.starts_with("leader-slots-snapshot-")
                })
                .unwrap_or(false)
        })
        .collect::<Vec<PathBuf>>();
    paths.sort();

    let mut snapshots = Vec::new();
    for path in paths {
        let bytes = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
        snapshots.push(
            serde_json::from_slice::<LeaderSlotsSnapshotArtifact>(&bytes)
                .with_context(|| format!("parse {}", path.display()))?,
        );
    }
    Ok(snapshots)
}

fn read_manifest(artifact_dir: &Path) -> Result<Option<crate::artifacts::BenchManifest>> {
    let path = artifact_dir.join("manifest.json");
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
    Ok(Some(
        serde_json::from_slice::<crate::artifacts::BenchManifest>(&bytes)
            .with_context(|| format!("parse {}", path.display()))?,
    ))
}

fn build_leader_lookup(snapshots: &[LeaderSlotsSnapshotArtifact]) -> BTreeMap<u64, LeaderMeta> {
    let mut out = BTreeMap::new();
    for snapshot in snapshots {
        let rows = snapshot
            .response
            .get("result")
            .and_then(|result| result.get("data"))
            .and_then(Value::as_array)
            .or_else(|| snapshot.response.get("data").and_then(Value::as_array));
        let Some(rows) = rows else {
            continue;
        };
        for row in rows {
            let Some(slot) = row.get("slot").and_then(Value::as_u64) else {
                continue;
            };
            out.entry(slot).or_insert_with(|| meta_from_leader_row(row));
        }
    }
    out
}

fn meta_from_leader_row(row: &Value) -> LeaderMeta {
    let identity = row
        .get("identity")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let location = row.get("location").unwrap_or(&Value::Null);
    let client = row.get("client").unwrap_or(&Value::Null);
    normalize_meta(LeaderMeta {
        identity,
        region: string_field(location, "region"),
        city: string_field(location, "city"),
        data_center_key: string_field(location, "dataCenterKey"),
        client_family: string_field(client, "family"),
        client_software: string_field(client, "software"),
        stake_bucket: row
            .get("stake")
            .and_then(Value::as_u64)
            .map(stake_bucket)
            .unwrap_or_else(|| "unknown".to_string()),
    })
}

fn string_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string()
}

fn normalize_meta(mut meta: LeaderMeta) -> LeaderMeta {
    if meta.identity.is_empty() {
        meta.identity = "unknown".to_string();
    }
    if meta.region.is_empty() {
        meta.region = "unknown".to_string();
    }
    if meta.city.is_empty() {
        meta.city = "unknown".to_string();
    }
    if meta.data_center_key.is_empty() {
        meta.data_center_key = "unknown".to_string();
    }
    if meta.client_family.is_empty() {
        meta.client_family = "unknown".to_string();
    }
    if meta.client_software.is_empty() {
        meta.client_software = meta.client_family.clone();
    }
    if meta.stake_bucket.is_empty() {
        meta.stake_bucket = "unknown".to_string();
    }
    meta
}

fn stake_bucket(stake_lamports: u64) -> String {
    let sol = stake_lamports / 1_000_000_000;
    if sol >= 1_000_000 {
        ">=1m_sol".to_string()
    } else if sol >= 500_000 {
        "500k_1m_sol".to_string()
    } else if sol >= 100_000 {
        "100k_500k_sol".to_string()
    } else if sol > 0 {
        "<100k_sol".to_string()
    } else {
        "unknown".to_string()
    }
}

fn summarize_providers(
    samples: &[BenchSample],
    manifest: Option<&crate::artifacts::BenchManifest>,
) -> Vec<ProviderReportSummary> {
    let provider_manifest = manifest
        .map(|manifest| {
            manifest
                .providers
                .iter()
                .map(|provider| (provider.name.as_str(), provider))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let mut groups = BTreeMap::<(String, String, Vec<String>), Vec<&BenchSample>>::new();
    for sample in samples {
        let manifest_provider = provider_manifest.get(sample.provider_name.as_str());
        let route_policy = sample
            .route_policy
            .clone()
            .or_else(|| manifest_provider.and_then(|provider| provider.route_mode.clone()))
            .unwrap_or_else(|| "static".to_string());
        let selected_routes = if !sample.selected_routes.is_empty() {
            sample.selected_routes.clone()
        } else {
            manifest_provider
                .map(|provider| provider.routes.clone())
                .unwrap_or_default()
        };
        groups
            .entry((sample.provider_name.clone(), route_policy, selected_routes))
            .or_default()
            .push(sample);
    }
    groups
        .into_iter()
        .map(|((provider_name, route_policy, selected_routes), rows)| {
            let mut latencies = rows
                .iter()
                .filter(|row| row.accepted)
                .map(|row| row.provider_ack_latency_us as i128)
                .collect::<Vec<_>>();
            latencies.sort_unstable();
            ProviderReportSummary {
                provider_name,
                route_policy,
                selected_routes,
                count: rows.len(),
                accepted: latencies.len(),
                errors: rows.len().saturating_sub(latencies.len()),
                ack_p50_ms: percentile_ms(&latencies, 0.50),
                ack_p90_ms: percentile_ms(&latencies, 0.90),
                ack_p99_ms: percentile_ms(&latencies, 0.99),
                ack_max_ms: percentile_ms(&latencies, 1.0),
            }
        })
        .collect()
}

fn summarize_sources(rows: &[ObservationRow]) -> Vec<SourceReportSummary> {
    let mut first_seen_by_signature = BTreeMap::<String, DateTime<Utc>>::new();
    for row in rows {
        first_seen_by_signature
            .entry(row.event.signature.clone())
            .and_modify(|existing| {
                if row.event.observed_at < *existing {
                    *existing = row.event.observed_at;
                }
            })
            .or_insert(row.event.observed_at);
    }

    let mut groups = BTreeMap::<String, Vec<&ObservationRow>>::new();
    for row in rows {
        groups
            .entry(row.event.source_name.clone())
            .or_default()
            .push(row);
    }
    groups
        .into_iter()
        .map(|(source_name, rows)| {
            let mut latencies = rows
                .iter()
                .filter_map(|row| row.latency_us)
                .collect::<Vec<_>>();
            latencies.sort_unstable();
            let mut slot_deltas = rows
                .iter()
                .filter_map(|row| {
                    Some(row.event.slot? as i64 - row.send.as_ref()?.send_slot as i64)
                })
                .collect::<Vec<_>>();
            slot_deltas.sort_unstable();
            let same_slot_count = slot_deltas.iter().filter(|delta| **delta == 0).count();
            let mut slot_indexes = rows
                .iter()
                .filter_map(|row| row.event.slot_index)
                .collect::<Vec<_>>();
            slot_indexes.sort_unstable();
            let first_seen = rows
                .iter()
                .filter(|row| {
                    first_seen_by_signature.get(&row.event.signature)
                        == Some(&row.event.observed_at)
                })
                .count();
            SourceReportSummary {
                source_name,
                count: rows.len(),
                first_seen,
                submit_to_observed_p50_ms: percentile_ms(&latencies, 0.50),
                submit_to_observed_p90_ms: percentile_ms(&latencies, 0.90),
                submit_to_observed_p99_ms: percentile_ms(&latencies, 0.99),
                submit_to_observed_max_ms: percentile_ms(&latencies, 1.0),
                same_slot_count,
                same_slot_rate: ratio(same_slot_count, slot_deltas.len()),
                landed_slot_delta_p50: percentile_i64(&slot_deltas, 0.50),
                landed_slot_delta_p90: percentile_i64(&slot_deltas, 0.90),
                slot_index_p50: percentile_u64(&slot_indexes, 0.50),
                slot_index_p90: percentile_u64(&slot_indexes, 0.90),
            }
        })
        .collect()
}

fn summarize_cohorts(rows: &[ObservationRow]) -> Vec<CohortReportSummary> {
    let mut groups = BTreeMap::<(String, String, String), Vec<i128>>::new();
    for row in rows {
        let Some(latency) = row.latency_us else {
            continue;
        };
        for (kind, value) in [
            ("leader_region", row.leader.region.as_str()),
            ("leader_city", row.leader.city.as_str()),
            ("data_center_key", row.leader.data_center_key.as_str()),
            ("client_family", row.leader.client_family.as_str()),
            ("client_software", row.leader.client_software.as_str()),
            ("stake_bucket", row.leader.stake_bucket.as_str()),
        ] {
            groups
                .entry((
                    kind.to_string(),
                    value.to_string(),
                    row.event.source_name.clone(),
                ))
                .or_default()
                .push(latency);
        }
    }
    let mut summaries = groups
        .into_iter()
        .map(
            |((cohort_kind, cohort_value, source_name), mut latencies)| {
                latencies.sort_unstable();
                CohortReportSummary {
                    cohort_kind,
                    cohort_value,
                    source_name,
                    count: latencies.len(),
                    submit_to_observed_p50_ms: percentile_ms(&latencies, 0.50),
                    submit_to_observed_p90_ms: percentile_ms(&latencies, 0.90),
                    submit_to_observed_p99_ms: percentile_ms(&latencies, 0.99),
                    submit_to_observed_max_ms: percentile_ms(&latencies, 1.0),
                }
            },
        )
        .collect::<Vec<_>>();
    summaries.sort_by(|a, b| {
        a.cohort_kind
            .cmp(&b.cohort_kind)
            .then_with(|| b.count.cmp(&a.count))
            .then_with(|| a.cohort_value.cmp(&b.cohort_value))
            .then_with(|| a.source_name.cmp(&b.source_name))
    });
    summaries
}

fn summarize_tail_events(rows: &[ObservationRow], limit: usize) -> Vec<TailEvent> {
    let mut rows = rows
        .iter()
        .filter_map(|row| {
            let latency_us = row.latency_us?;
            Some((latency_us, row))
        })
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| b.0.cmp(&a.0));
    rows.into_iter()
        .take(limit)
        .map(|(latency_us, row)| TailEvent {
            signature: row.event.signature.clone(),
            source_name: row.event.source_name.clone(),
            latency_ms: latency_us as f64 / 1000.0,
            send_slot: row.send.as_ref().map(|send| send.send_slot),
            landed_slot: row.event.slot,
            slot_index: row.event.slot_index,
            leader_identity: row.leader.identity.clone(),
            leader_region: row.leader.region.clone(),
            leader_city: row.leader.city.clone(),
            leader_client: row.leader.client_software.clone(),
        })
        .collect()
}

fn percentile_ms(sorted_us: &[i128], p: f64) -> Option<f64> {
    percentile_i128(sorted_us, p).map(|value| value as f64 / 1000.0)
}

fn percentile_i128(sorted: &[i128], p: f64) -> Option<i128> {
    if sorted.is_empty() {
        return None;
    }
    let rank = ((sorted.len() - 1) as f64 * p).ceil() as usize;
    sorted.get(rank).copied()
}

fn percentile_i64(sorted: &[i64], p: f64) -> Option<i64> {
    if sorted.is_empty() {
        return None;
    }
    let rank = ((sorted.len() - 1) as f64 * p).ceil() as usize;
    sorted.get(rank).copied()
}

fn percentile_u64(sorted: &[u64], p: f64) -> Option<u64> {
    if sorted.is_empty() {
        return None;
    }
    let rank = ((sorted.len() - 1) as f64 * p).ceil() as usize;
    sorted.get(rank).copied()
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn report_markdown(report: &ReportOutput) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# Transaction Landing Report: `{}`\n\n",
        report.test_id
    ));
    out.push_str(&format!(
        "- Generated: `{}`\n- Artifact dir: `{}`\n- Sent transactions: `{}`\n- Provider samples: `{}`\n- Observations: `{}`\n- Matched signatures: `{}`\n- Leader-slot snapshots: `{}`\n\n",
        report.generated_at.to_rfc3339_opts(SecondsFormat::Millis, true),
        report.artifact_dir,
        report.totals.sent_transactions,
        report.totals.samples,
        report.totals.observations,
        report.totals.matched_signatures,
        report.totals.leader_slot_snapshots,
    ));
    out.push_str("This report is generated only from local benchmark artifacts. Leader metadata comes from saved `getLeaderSlots` snapshots captured during the run; no ClickHouse or private database join is required.\n\n");

    out.push_str("## Provider ACK\n\n");
    out.push_str("| Provider | Policy | Routes | Count | Accepted | Errors | p50 ms | p90 ms | p99 ms | max ms |\n");
    out.push_str("| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n");
    for row in &report.provider_summaries {
        out.push_str(&format!(
            "| `{}` | `{}` | `{}` | {} | {} | {} | {} | {} | {} | {} |\n",
            row.provider_name,
            row.route_policy,
            row.selected_routes.join(","),
            row.count,
            row.accepted,
            row.errors,
            fmt_ms(row.ack_p50_ms),
            fmt_ms(row.ack_p90_ms),
            fmt_ms(row.ack_p99_ms),
            fmt_ms(row.ack_max_ms),
        ));
    }
    out.push_str("\nProvider ACK is a sender diagnostic, not landing proof.\n\n");

    out.push_str("## Observation Sources\n\n");
    out.push_str("| Source | Count | First seen | p50 ms | p90 ms | p99 ms | max ms | same-slot | slot delta p50 | slot index p50 |\n");
    out.push_str("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n");
    for row in &report.source_summaries {
        out.push_str(&format!(
            "| `{}` | {} | {} | {} | {} | {} | {} | {} / {:.1}% | {} | {} |\n",
            row.source_name,
            row.count,
            row.first_seen,
            fmt_ms(row.submit_to_observed_p50_ms),
            fmt_ms(row.submit_to_observed_p90_ms),
            fmt_ms(row.submit_to_observed_p99_ms),
            fmt_ms(row.submit_to_observed_max_ms),
            row.same_slot_count,
            row.same_slot_rate * 100.0,
            fmt_i64(row.landed_slot_delta_p50),
            fmt_u64(row.slot_index_p50),
        ));
    }

    for kind in [
        "leader_region",
        "leader_city",
        "data_center_key",
        "client_family",
        "client_software",
        "stake_bucket",
    ] {
        out.push_str(&format!("\n## Cohort: `{kind}`\n\n"));
        out.push_str("| Value | Source | Count | p50 ms | p90 ms | p99 ms | max ms |\n");
        out.push_str("| --- | --- | ---: | ---: | ---: | ---: | ---: |\n");
        for row in report
            .cohort_summaries
            .iter()
            .filter(|row| row.cohort_kind == kind)
            .take(24)
        {
            out.push_str(&format!(
                "| `{}` | `{}` | {} | {} | {} | {} | {} |\n",
                row.cohort_value,
                row.source_name,
                row.count,
                fmt_ms(row.submit_to_observed_p50_ms),
                fmt_ms(row.submit_to_observed_p90_ms),
                fmt_ms(row.submit_to_observed_p99_ms),
                fmt_ms(row.submit_to_observed_max_ms),
            ));
        }
    }

    out.push_str("\n## Tail Events\n\n");
    out.push_str("| Signature | Source | Latency ms | Send slot | Landed slot | Slot index | Region | City | Client |\n");
    out.push_str("| --- | --- | ---: | ---: | ---: | ---: | --- | --- | --- |\n");
    for row in &report.tail_events {
        out.push_str(&format!(
            "| `{}` | `{}` | {:.3} | {} | {} | {} | `{}` | `{}` | `{}` |\n",
            short_sig(&row.signature),
            row.source_name,
            row.latency_ms,
            fmt_u64(row.send_slot),
            fmt_u64(row.landed_slot),
            fmt_u64(row.slot_index),
            row.leader_region,
            row.leader_city,
            row.leader_client,
        ));
    }
    out
}

fn report_html(report: &ReportOutput) -> String {
    let mut out = String::new();
    out.push_str("<!doctype html><html><head><meta charset=\"utf-8\">");
    out.push_str(&format!(
        "<title>Transaction Landing Report: {}</title>",
        html_escape(&report.test_id)
    ));
    out.push_str(
        "<style>
body{font-family:Inter,Arial,sans-serif;margin:0;background:#f7f8fa;color:#17202a;line-height:1.45}
main{max-width:1180px;margin:0 auto;padding:32px}
h1{font-size:26px;margin:0 0 6px} h2{font-size:18px;margin:28px 0 10px}
.muted{color:#5d6975}.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(155px,1fr));gap:10px;margin:18px 0}
.stat{background:#fff;border:1px solid #e3e7ec;border-radius:8px;padding:12px}.stat b{display:block;font-size:20px}
table{width:100%;border-collapse:collapse;background:#fff;border:1px solid #e3e7ec;border-radius:8px;overflow:hidden}
th,td{padding:8px 10px;border-bottom:1px solid #edf0f3;text-align:right;font-size:13px}
th:first-child,td:first-child,td.left{text-align:left}th{background:#f0f3f6;color:#44505c;font-weight:600}
code{background:#eef1f4;padding:1px 4px;border-radius:4px}.note{background:#fff;border-left:4px solid #4b7bec;padding:10px 12px;margin:14px 0}
</style></head><body><main>",
    );
    out.push_str(&format!(
        "<h1>Transaction Landing Report</h1><div class=\"muted\"><code>{}</code><br>Generated: <code>{}</code></div>",
        html_escape(&report.test_id),
        html_escape(&report.generated_at.to_rfc3339_opts(SecondsFormat::Millis, true)),
    ));
    out.push_str("<div class=\"grid\">");
    for (label, value) in [
        ("Sent", report.totals.sent_transactions.to_string()),
        ("Provider Samples", report.totals.samples.to_string()),
        ("Observations", report.totals.observations.to_string()),
        (
            "Matched Signatures",
            report.totals.matched_signatures.to_string(),
        ),
        (
            "Leader Snapshots",
            report.totals.leader_slot_snapshots.to_string(),
        ),
    ] {
        out.push_str(&format!(
            "<div class=\"stat\"><span class=\"muted\">{}</span><b>{}</b></div>",
            html_escape(label),
            html_escape(&value)
        ));
    }
    out.push_str("</div>");
    out.push_str("<div class=\"note\">Generated only from local benchmark artifacts. Leader metadata comes from saved <code>getLeaderSlots</code> snapshots captured during the run; no ClickHouse or private database join is required.</div>");

    out.push_str("<h2>Provider ACK</h2><table><thead><tr><th>Provider</th><th>Policy</th><th>Routes</th><th>Count</th><th>Accepted</th><th>Errors</th><th>p50 ms</th><th>p90 ms</th><th>p99 ms</th><th>max ms</th></tr></thead><tbody>");
    for row in &report.provider_summaries {
        out.push_str(&format!(
            "<tr><td class=\"left\"><code>{}</code></td><td><code>{}</code></td><td><code>{}</code></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            html_escape(&row.provider_name),
            html_escape(&row.route_policy),
            html_escape(&row.selected_routes.join(",")),
            row.count,
            row.accepted,
            row.errors,
            fmt_ms(row.ack_p50_ms),
            fmt_ms(row.ack_p90_ms),
            fmt_ms(row.ack_p99_ms),
            fmt_ms(row.ack_max_ms),
        ));
    }
    out.push_str("</tbody></table><p class=\"muted\">Provider ACK is a sender diagnostic, not landing proof.</p>");

    out.push_str("<h2>Observation Sources</h2><table><thead><tr><th>Source</th><th>Count</th><th>First seen</th><th>p50 ms</th><th>p90 ms</th><th>p99 ms</th><th>max ms</th><th>same-slot</th><th>slot delta p50</th><th>slot index p50</th></tr></thead><tbody>");
    for row in &report.source_summaries {
        out.push_str(&format!(
            "<tr><td class=\"left\"><code>{}</code></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{} / {:.1}%</td><td>{}</td><td>{}</td></tr>",
            html_escape(&row.source_name),
            row.count,
            row.first_seen,
            fmt_ms(row.submit_to_observed_p50_ms),
            fmt_ms(row.submit_to_observed_p90_ms),
            fmt_ms(row.submit_to_observed_p99_ms),
            fmt_ms(row.submit_to_observed_max_ms),
            row.same_slot_count,
            row.same_slot_rate * 100.0,
            fmt_i64(row.landed_slot_delta_p50),
            fmt_u64(row.slot_index_p50),
        ));
    }
    out.push_str("</tbody></table>");

    for kind in [
        "leader_region",
        "leader_city",
        "data_center_key",
        "client_family",
        "client_software",
        "stake_bucket",
    ] {
        out.push_str(&format!(
            "<h2>Cohort: <code>{}</code></h2><table><thead><tr><th>Value</th><th>Source</th><th>Count</th><th>p50 ms</th><th>p90 ms</th><th>p99 ms</th><th>max ms</th></tr></thead><tbody>",
            html_escape(kind)
        ));
        for row in report
            .cohort_summaries
            .iter()
            .filter(|row| row.cohort_kind == kind)
            .take(24)
        {
            out.push_str(&format!(
                "<tr><td class=\"left\"><code>{}</code></td><td><code>{}</code></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                html_escape(&row.cohort_value),
                html_escape(&row.source_name),
                row.count,
                fmt_ms(row.submit_to_observed_p50_ms),
                fmt_ms(row.submit_to_observed_p90_ms),
                fmt_ms(row.submit_to_observed_p99_ms),
                fmt_ms(row.submit_to_observed_max_ms),
            ));
        }
        out.push_str("</tbody></table>");
    }

    out.push_str("<h2>Tail Events</h2><table><thead><tr><th>Signature</th><th>Source</th><th>Latency ms</th><th>Send slot</th><th>Landed slot</th><th>Slot index</th><th>Region</th><th>City</th><th>Client</th></tr></thead><tbody>");
    for row in &report.tail_events {
        out.push_str(&format!(
            "<tr><td class=\"left\"><code>{}</code></td><td><code>{}</code></td><td>{:.3}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            html_escape(&short_sig(&row.signature)),
            html_escape(&row.source_name),
            row.latency_ms,
            fmt_u64(row.send_slot),
            fmt_u64(row.landed_slot),
            fmt_u64(row.slot_index),
            html_escape(&row.leader_region),
            html_escape(&row.leader_city),
            html_escape(&row.leader_client),
        ));
    }
    out.push_str("</tbody></table></main></body></html>");
    out
}

fn fmt_ms(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.3}"))
        .unwrap_or_else(|| "-".to_string())
}

fn fmt_i64(value: Option<i64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-".to_string())
}

fn fmt_u64(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-".to_string())
}

fn short_sig(signature: &str) -> String {
    if signature.len() <= 16 {
        signature.to_string()
    } else {
        format!(
            "{}...{}",
            &signature[..8],
            &signature[signature.len() - 8..]
        )
    }
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn report_uses_saved_leader_slot_snapshot() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        fs::write(
            root.join("samples.ndjson"),
            r#"{"schema_version":2,"test_id":"run-a","iteration":0,"signature":"sig-a","provider_name":"p","provider_kind":"rpcedge_quic_raw_tx","accepted":true,"client_started_at":"2026-01-01T00:00:00Z","client_finished_at":"2026-01-01T00:00:00.001Z","client_ack_latency_us":1000,"provider_send_started_at":"2026-01-01T00:00:00Z","provider_send_finished_at":"2026-01-01T00:00:00.001Z","provider_ack_latency_us":1000,"provider_request_id":"run-a-0","returned_signature":"sig-a","status_code":200,"error_class":null,"error":null,"route_policy":"static","route_mode":"only","selected_routes":["tpu_quic"],"leader_client_family":"jito","compute_unit_limit":660,"compute_unit_price_microlamports":0,"estimated_spend_lamports":5000}"#,
        )
        .expect("samples");
        fs::write(
            root.join("leader-sends.ndjson"),
            r#"{"schema_version":2,"test_id":"run-a","iteration":0,"signature":"sig-a","leader_identity":"leader-a","leader_run_start_slot":100,"leader_run_end_slot":103,"send_slot":100,"sent_at":"2026-01-01T00:00:00Z","trigger_source":"grpc_slot","slot_signal_status":"SLOT_FIRST_SHRED_RECEIVED","slot_signal_observed_at":"2026-01-01T00:00:00Z","leader_client_family":"jito","route_policy":"static","selected_routes":["tpu_quic"],"compute_unit_limit":660,"compute_unit_price_microlamports":0,"estimated_spend_lamports":5000}"#,
        )
        .expect("sends");
        fs::write(
            root.join("matched-observations.ndjson"),
            r#"{"schema_version":1,"test_id":"run-a","signature":"sig-a","source_name":"deshred","source_kind":"yellowstone_deshred","observed_at":"2026-01-01T00:00:00.100Z","submitted_at":"2026-01-01T00:00:00Z","slot":100,"slot_index":7,"source_sequence":null}"#,
        )
        .expect("observations");
        let snapshot = LeaderSlotsSnapshotArtifact {
            schema_version: 1,
            fetched_at: Utc::now(),
            rpc_url_label: "http://rpc.example".to_string(),
            start_slot: 100,
            limit: 4,
            response: json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": {
                    "success": true,
                    "data": [{
                        "slot": 100,
                        "identity": "leader-a",
                        "stake": 1_500_000_000_000_000_u64,
                        "location": {
                            "region": "Europe",
                            "city": "Frankfurt",
                            "dataCenterKey": "20326-DE-Frankfurt"
                        },
                        "client": {
                            "family": "jito",
                            "software": "JitoLabs"
                        }
                    }]
                }
            }),
        };
        fs::write(
            root.join("leader-slots-snapshot-100.json"),
            serde_json::to_vec(&snapshot).expect("snapshot"),
        )
        .expect("snapshot");

        let report = generate_report(root).expect("report");
        assert_eq!(report.totals.matched_signatures, 1);
        assert!(report
            .cohort_summaries
            .iter()
            .any(|row| row.cohort_kind == "leader_region" && row.cohort_value == "Europe"));
        assert!(root.join("report.md").exists());
        assert!(root.join("report.html").exists());
    }
}
