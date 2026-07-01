use crate::{
    artifacts::{BenchManifest, BenchSample},
    leader_paced::LeaderSendEvent,
    observations::ObservationEvent,
};
use anyhow::{bail, Context, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct CompareOptions {
    pub artifact_dirs: Vec<PathBuf>,
    pub labels: Vec<String>,
    pub output_dir: PathBuf,
    pub primary_source: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonOutput {
    pub schema_version: u32,
    pub title: String,
    pub generated_at: DateTime<Utc>,
    pub primary_source: String,
    pub run_count: usize,
    pub runs: Vec<RunComparisonSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunComparisonSummary {
    pub label: String,
    pub test_id: String,
    pub artifact_dir: String,
    pub route_policy: String,
    pub routes: String,
    pub total_runs: usize,
    pub provider_samples: usize,
    pub accepted: usize,
    pub errors: usize,
    pub avg_ack_ms: Option<f64>,
    pub p90_ack_ms: Option<f64>,
    pub avg_deshred_ms: Option<f64>,
    pub p90_deshred_ms: Option<f64>,
    pub avg_landed_ms: Option<f64>,
    pub p90_landed_ms: Option<f64>,
    pub avg_block_ms: Option<f64>,
    pub p90_block_ms: Option<f64>,
    pub avg_landed_slots: Option<f64>,
    pub p90_landed_slots: Option<f64>,
    pub avg_idx_in_block: Option<f64>,
    pub p90_idx_in_block: Option<f64>,
    pub avg_priority_fee_lamports: Option<f64>,
    pub max_slots: Option<i64>,
    pub min_slots: Option<i64>,
    pub same_slot_landed: usize,
    pub landed_runs: usize,
    pub block_seen: usize,
    pub success_ratio_pct: f64,
    pub performance_rate_pct: f64,
    pub score: ScoreBreakdown,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScoreBreakdown {
    pub landed_ms: f64,
    pub landed_slots: f64,
    pub landed_idx: f64,
    pub same_slot: f64,
    pub success_ratio: f64,
}

#[derive(Debug, Clone, Default)]
struct SourceStats {
    count: usize,
    latencies_ms: Vec<f64>,
    slot_deltas: Vec<i64>,
    slot_indexes: Vec<u64>,
}

pub fn generate_comparison(options: CompareOptions) -> Result<ComparisonOutput> {
    if options.artifact_dirs.is_empty() {
        bail!("compare requires at least one --artifact-dir value");
    }
    if !options.labels.is_empty() && options.labels.len() != options.artifact_dirs.len() {
        bail!("--label count must match --artifact-dir count");
    }

    fs::create_dir_all(&options.output_dir)
        .with_context(|| format!("create {}", options.output_dir.display()))?;

    let mut runs = Vec::with_capacity(options.artifact_dirs.len());
    for (index, artifact_dir) in options.artifact_dirs.iter().enumerate() {
        let label = options.labels.get(index).cloned();
        runs.extend(load_runs(artifact_dir, label, &options.primary_source)?);
    }
    if runs.len() < 2 {
        bail!("compare needs at least two runs or one paired artifact with multiple policy arms");
    }

    apply_scores(&mut runs);

    let output = ComparisonOutput {
        schema_version: 1,
        title: options.title,
        generated_at: Utc::now(),
        primary_source: options.primary_source,
        run_count: runs.len(),
        runs,
    };

    fs::write(
        options.output_dir.join("comparison.json"),
        serde_json::to_vec_pretty(&output)?,
    )
    .with_context(|| {
        format!(
            "write {}",
            options.output_dir.join("comparison.json").display()
        )
    })?;
    fs::write(
        options.output_dir.join("comparison.md"),
        comparison_markdown(&output),
    )
    .with_context(|| {
        format!(
            "write {}",
            options.output_dir.join("comparison.md").display()
        )
    })?;
    let html = comparison_html(&output);
    fs::write(options.output_dir.join("comparison.html"), &html).with_context(|| {
        format!(
            "write {}",
            options.output_dir.join("comparison.html").display()
        )
    })?;
    fs::write(options.output_dir.join("index.html"), html)
        .with_context(|| format!("write {}", options.output_dir.join("index.html").display()))?;

    Ok(output)
}

#[derive(Debug)]
struct RunInputs {
    samples: Vec<BenchSample>,
    sends: Vec<LeaderSendEvent>,
    observations: Vec<ObservationEvent>,
    manifest: Option<BenchManifest>,
}

fn load_runs(
    artifact_dir: &Path,
    label: Option<String>,
    requested_primary_source: &str,
) -> Result<Vec<RunComparisonSummary>> {
    let samples: Vec<BenchSample> = read_ndjson(&artifact_dir.join("samples.ndjson"))?;
    let sends: Vec<LeaderSendEvent> = read_ndjson(&artifact_dir.join("leader-sends.ndjson"))?;
    let observations: Vec<ObservationEvent> =
        read_ndjson(&artifact_dir.join("matched-observations.ndjson"))?;
    let manifest = read_manifest(artifact_dir)?;

    let paired_arms = sends
        .iter()
        .filter_map(|send| send.policy_arm.clone())
        .collect::<BTreeSet<_>>();
    if paired_arms.len() > 1 {
        return paired_arms
            .into_iter()
            .map(|arm| {
                let signatures = sends
                    .iter()
                    .filter(|send| send.policy_arm.as_deref() == Some(arm.as_str()))
                    .map(|send| send.signature.clone())
                    .collect::<BTreeSet<_>>();
                let inputs = RunInputs {
                    samples: samples
                        .iter()
                        .filter(|sample| signatures.contains(&sample.signature))
                        .cloned()
                        .collect(),
                    sends: sends
                        .iter()
                        .filter(|send| signatures.contains(&send.signature))
                        .cloned()
                        .collect(),
                    observations: observations
                        .iter()
                        .filter(|event| signatures.contains(&event.signature))
                        .cloned()
                        .collect(),
                    manifest: manifest.clone(),
                };
                let arm_label = label
                    .as_deref()
                    .map(|base| format!("{base} / {arm}"))
                    .unwrap_or_else(|| arm.clone());
                load_run_from_inputs(
                    artifact_dir,
                    Some(arm_label),
                    requested_primary_source,
                    inputs,
                )
            })
            .collect();
    }

    load_run_from_inputs(
        artifact_dir,
        label,
        requested_primary_source,
        RunInputs {
            samples,
            sends,
            observations,
            manifest,
        },
    )
    .map(|run| vec![run])
}

fn load_run_from_inputs(
    artifact_dir: &Path,
    label: Option<String>,
    requested_primary_source: &str,
    inputs: RunInputs,
) -> Result<RunComparisonSummary> {
    let RunInputs {
        samples,
        sends,
        observations,
        manifest,
    } = inputs;

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
    let label = label.unwrap_or_else(|| test_id.clone());

    let send_by_signature = sends
        .iter()
        .map(|send| (send.signature.clone(), send))
        .collect::<BTreeMap<_, _>>();

    let primary_source = select_source(&observations, requested_primary_source);
    let primary = source_stats(&observations, &send_by_signature, &primary_source);
    let deshred = source_stats(&observations, &send_by_signature, "rpcedge_deshred");
    let processed = source_stats(&observations, &send_by_signature, "rpcedge_processed");

    let mut ack_latencies_ms = samples
        .iter()
        .filter(|sample| sample.accepted)
        .map(|sample| sample.provider_ack_latency_us as f64 / 1000.0)
        .collect::<Vec<_>>();
    ack_latencies_ms.sort_by(f64::total_cmp);

    let total_runs = submitted_signature_count(&samples, &sends, &observations);
    let accepted = samples.iter().filter(|sample| sample.accepted).count();
    let errors = samples.len().saturating_sub(accepted);

    let priority_fees = sends
        .iter()
        .map(|send| {
            priority_fee_lamports(
                send.compute_unit_limit,
                send.compute_unit_price_microlamports,
            ) as f64
        })
        .collect::<Vec<_>>();
    let block_seen = processed.count;
    let landed_runs = primary.count;
    let same_slot_landed = primary
        .slot_deltas
        .iter()
        .filter(|delta| **delta == 0)
        .count();

    Ok(RunComparisonSummary {
        label,
        test_id,
        artifact_dir: artifact_dir.display().to_string(),
        route_policy: route_policy_summary(&samples, manifest.as_ref()),
        routes: routes_summary(&samples, manifest.as_ref()),
        total_runs,
        provider_samples: samples.len(),
        accepted,
        errors,
        avg_ack_ms: avg(&ack_latencies_ms),
        p90_ack_ms: percentile_f64(&ack_latencies_ms, 0.90),
        avg_deshred_ms: avg(&deshred.latencies_ms),
        p90_deshred_ms: percentile_f64(&deshred.latencies_ms, 0.90),
        avg_landed_ms: avg(&primary.latencies_ms),
        p90_landed_ms: percentile_f64(&primary.latencies_ms, 0.90),
        avg_block_ms: avg(&processed.latencies_ms),
        p90_block_ms: percentile_f64(&processed.latencies_ms, 0.90),
        avg_landed_slots: avg_i64(&primary.slot_deltas),
        p90_landed_slots: percentile_i64_as_f64(&primary.slot_deltas, 0.90),
        avg_idx_in_block: avg_u64(&processed.slot_indexes),
        p90_idx_in_block: percentile_u64_as_f64(&processed.slot_indexes, 0.90),
        avg_priority_fee_lamports: avg(&priority_fees),
        max_slots: primary.slot_deltas.iter().max().copied(),
        min_slots: primary.slot_deltas.iter().min().copied(),
        same_slot_landed,
        landed_runs,
        block_seen,
        success_ratio_pct: ratio(landed_runs, total_runs) * 100.0,
        performance_rate_pct: 0.0,
        score: ScoreBreakdown::default(),
    })
}

fn submitted_signature_count(
    samples: &[BenchSample],
    sends: &[LeaderSendEvent],
    observations: &[ObservationEvent],
) -> usize {
    let mut signatures = sends
        .iter()
        .map(|send| send.signature.as_str())
        .collect::<BTreeSet<_>>();
    if signatures.is_empty() {
        signatures.extend(
            samples
                .iter()
                .filter(|sample| sample.accepted)
                .map(|sample| sample.signature.as_str()),
        );
    }
    if signatures.is_empty() {
        signatures.extend(observations.iter().map(|event| event.signature.as_str()));
    }
    signatures.len()
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

fn read_manifest(artifact_dir: &Path) -> Result<Option<BenchManifest>> {
    let path = artifact_dir.join("manifest.json");
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
    Ok(Some(
        serde_json::from_slice::<BenchManifest>(&bytes)
            .with_context(|| format!("parse {}", path.display()))?,
    ))
}

fn select_source(observations: &[ObservationEvent], requested: &str) -> String {
    if observations
        .iter()
        .any(|event| event.source_name == requested)
    {
        return requested.to_string();
    }
    observations
        .iter()
        .find(|event| event.source_name.contains("processed"))
        .map(|event| event.source_name.clone())
        .or_else(|| observations.first().map(|event| event.source_name.clone()))
        .unwrap_or_else(|| requested.to_string())
}

fn source_stats(
    observations: &[ObservationEvent],
    send_by_signature: &BTreeMap<String, &LeaderSendEvent>,
    source: &str,
) -> SourceStats {
    let mut first_by_signature = BTreeMap::<String, &ObservationEvent>::new();
    for event in observations
        .iter()
        .filter(|event| event.source_name == source)
    {
        first_by_signature
            .entry(event.signature.clone())
            .and_modify(|existing| {
                if event.observed_at < existing.observed_at {
                    *existing = event;
                }
            })
            .or_insert(event);
    }

    let mut stats = SourceStats {
        count: first_by_signature.len(),
        ..SourceStats::default()
    };
    for event in first_by_signature.values() {
        let send = send_by_signature.get(&event.signature).copied();
        let submitted_at = event.submitted_at.or_else(|| send.map(|send| send.sent_at));
        if let Some(submitted_at) = submitted_at {
            if let Some(us) = event
                .observed_at
                .signed_duration_since(submitted_at)
                .num_microseconds()
            {
                stats.latencies_ms.push(us as f64 / 1000.0);
            }
        }
        if let (Some(slot), Some(send)) = (event.slot, send) {
            stats.slot_deltas.push(slot as i64 - send.send_slot as i64);
        }
        if let Some(slot_index) = event.slot_index {
            stats.slot_indexes.push(slot_index);
        }
    }
    stats.latencies_ms.sort_by(f64::total_cmp);
    stats.slot_deltas.sort_unstable();
    stats.slot_indexes.sort_unstable();
    stats
}

fn route_policy_summary(samples: &[BenchSample], manifest: Option<&BenchManifest>) -> String {
    let mut counts = BTreeMap::<String, usize>::new();
    for sample in samples {
        let policy = sample
            .route_policy
            .clone()
            .or_else(|| sample.route_mode.clone())
            .unwrap_or_else(|| "static".to_string());
        *counts.entry(policy).or_default() += 1;
    }
    if counts.is_empty() {
        if let Some(manifest) = manifest {
            let policies = manifest
                .providers
                .iter()
                .filter_map(|provider| provider.route_mode.clone())
                .collect::<BTreeSet<_>>();
            if !policies.is_empty() {
                return policies.into_iter().collect::<Vec<_>>().join(", ");
            }
        }
        return "static".to_string();
    }
    counts
        .into_iter()
        .map(|(policy, count)| format!("{policy} ({count})"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn routes_summary(samples: &[BenchSample], manifest: Option<&BenchManifest>) -> String {
    let mut counts = BTreeMap::<String, usize>::new();
    for sample in samples {
        let routes = if sample.selected_routes.is_empty() {
            String::new()
        } else {
            sample.selected_routes.join("+")
        };
        if !routes.is_empty() {
            *counts.entry(routes).or_default() += 1;
        }
    }
    if counts.is_empty() {
        if let Some(manifest) = manifest {
            let routes = manifest
                .providers
                .iter()
                .flat_map(|provider| provider.routes.iter().cloned())
                .collect::<BTreeSet<_>>();
            if !routes.is_empty() {
                return routes.into_iter().collect::<Vec<_>>().join("+");
            }
        }
        return "unknown".to_string();
    }
    counts
        .into_iter()
        .map(|(routes, count)| format!("{routes} ({count})"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn priority_fee_lamports(cu_limit: u32, cu_price_microlamports: u64) -> u64 {
    (cu_limit as u64)
        .saturating_mul(cu_price_microlamports)
        .saturating_add(999_999)
        / 1_000_000
}

fn apply_scores(runs: &mut [RunComparisonSummary]) {
    let best_avg_landed_ms = min_present(runs.iter().filter_map(|run| run.avg_landed_ms));
    let best_p90_landed_ms = min_present(runs.iter().filter_map(|run| run.p90_landed_ms));
    let best_avg_slots = min_present(runs.iter().filter_map(|run| run.avg_landed_slots));
    let best_p90_slots = min_present(runs.iter().filter_map(|run| run.p90_landed_slots));
    let best_avg_idx = min_present(runs.iter().filter_map(|run| run.avg_idx_in_block));
    let best_p90_idx = min_present(runs.iter().filter_map(|run| run.p90_idx_in_block));
    let best_same_slot = max_present(
        runs.iter()
            .map(|run| ratio(run.same_slot_landed, run.total_runs) * 100.0),
    );
    let best_success = max_present(runs.iter().map(|run| run.success_ratio_pct));

    for run in runs {
        let avg_landed_ms =
            lower_is_better_score(run.avg_landed_ms, best_avg_landed_ms).unwrap_or(0.0);
        let p90_landed_ms =
            lower_is_better_score(run.p90_landed_ms, best_p90_landed_ms).unwrap_or(0.0);
        let avg_slots = lower_is_better_score(run.avg_landed_slots, best_avg_slots).unwrap_or(0.0);
        let p90_slots = lower_is_better_score(run.p90_landed_slots, best_p90_slots).unwrap_or(0.0);
        let avg_idx = lower_is_better_score(run.avg_idx_in_block, best_avg_idx).unwrap_or(0.0);
        let p90_idx = lower_is_better_score(run.p90_idx_in_block, best_p90_idx).unwrap_or(0.0);
        let same_slot_rate = ratio(run.same_slot_landed, run.total_runs) * 100.0;

        run.score = ScoreBreakdown {
            landed_ms: 0.2 * avg_landed_ms + 0.8 * p90_landed_ms,
            landed_slots: 0.2 * avg_slots + 0.8 * p90_slots,
            landed_idx: 0.5 * avg_idx + 0.5 * p90_idx,
            same_slot: higher_is_better_score(same_slot_rate, best_same_slot).unwrap_or(0.0),
            success_ratio: higher_is_better_score(run.success_ratio_pct, best_success)
                .unwrap_or(0.0),
        };
        run.performance_rate_pct = (run.score.landed_ms
            + run.score.landed_slots
            + run.score.landed_idx
            + run.score.same_slot
            + run.score.success_ratio)
            / 5.0;
    }
}

fn min_present(values: impl Iterator<Item = f64>) -> Option<f64> {
    values
        .filter(|value| value.is_finite())
        .min_by(f64::total_cmp)
}

fn max_present(values: impl Iterator<Item = f64>) -> Option<f64> {
    values
        .filter(|value| value.is_finite())
        .max_by(f64::total_cmp)
}

fn lower_is_better_score(value: Option<f64>, best: Option<f64>) -> Option<f64> {
    let value = value?;
    let best = best?;
    if !value.is_finite() || !best.is_finite() {
        return None;
    }
    if value <= best {
        return Some(100.0);
    }
    let score = if best <= 0.0 {
        100.0 / (1.0 + value - best)
    } else {
        100.0 * best / value
    };
    Some(score.clamp(0.0, 100.0))
}

fn higher_is_better_score(value: f64, best: Option<f64>) -> Option<f64> {
    let best = best?;
    if !value.is_finite() || !best.is_finite() {
        return None;
    }
    if best <= 0.0 {
        return Some(if value <= 0.0 { 100.0 } else { 0.0 });
    }
    Some((100.0 * value / best).clamp(0.0, 100.0))
}

fn avg(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        None
    } else {
        Some(values.iter().sum::<f64>() / values.len() as f64)
    }
}

fn avg_i64(values: &[i64]) -> Option<f64> {
    if values.is_empty() {
        None
    } else {
        Some(values.iter().map(|value| *value as f64).sum::<f64>() / values.len() as f64)
    }
}

fn avg_u64(values: &[u64]) -> Option<f64> {
    if values.is_empty() {
        None
    } else {
        Some(values.iter().map(|value| *value as f64).sum::<f64>() / values.len() as f64)
    }
}

fn percentile_f64(sorted: &[f64], p: f64) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let rank = ((sorted.len() - 1) as f64 * p).ceil() as usize;
    sorted.get(rank).copied()
}

fn percentile_i64_as_f64(sorted: &[i64], p: f64) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let rank = ((sorted.len() - 1) as f64 * p).ceil() as usize;
    sorted.get(rank).map(|value| *value as f64)
}

fn percentile_u64_as_f64(sorted: &[u64], p: f64) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let rank = ((sorted.len() - 1) as f64 * p).ceil() as usize;
    sorted.get(rank).map(|value| *value as f64)
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn comparison_markdown(output: &ComparisonOutput) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", output.title));
    out.push_str(&format!(
        "- Generated: `{}`\n- Primary landing source: `{}`\n- Runs: `{}`\n\n",
        output
            .generated_at
            .to_rfc3339_opts(SecondsFormat::Millis, true),
        output.primary_source,
        output.run_count,
    ));
    out.push_str("## Methodology\n\n");
    out.push_str(methodology_text());
    out.push_str("\n\n## Comparison\n\n");
    out.push_str("| Run | Routes | Avg ACK ms | P90 ACK ms | Avg Deshred ms | P90 Deshred ms | Avg Landed ms | P90 Landed ms | Avg Landed slots | P90 Landed slots | Avg Idx-in-Block | P90 Idx-in-Block | Avg Priority Fee | Max Slots | Min Slots | Same-slot landed | Landed runs | Block seen | Total runs | Success ratio % | Performance rate % |\n");
    out.push_str("| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n");
    for run in &output.runs {
        out.push_str(&format!(
            "| `{}` | `{}` | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {:.2} | {:.2} |\n",
            run.label,
            run.routes,
            fmt_f64(run.avg_ack_ms),
            fmt_f64(run.p90_ack_ms),
            fmt_f64(run.avg_deshred_ms),
            fmt_f64(run.p90_deshred_ms),
            fmt_f64(run.avg_landed_ms),
            fmt_f64(run.p90_landed_ms),
            fmt_f64(run.avg_landed_slots),
            fmt_f64(run.p90_landed_slots),
            fmt_f64(run.avg_idx_in_block),
            fmt_f64(run.p90_idx_in_block),
            fmt_f64(run.avg_priority_fee_lamports),
            fmt_i64(run.max_slots),
            fmt_i64(run.min_slots),
            run.same_slot_landed,
            run.landed_runs,
            run.block_seen,
            run.total_runs,
            run.success_ratio_pct,
            run.performance_rate_pct,
        ));
    }
    out.push_str("\n## Score Components\n\n");
    out.push_str(
        "| Run | Landed ms | Landed slots | Block index | Same-slot | Success | Final |\n",
    );
    out.push_str("| --- | ---: | ---: | ---: | ---: | ---: | ---: |\n");
    for run in &output.runs {
        out.push_str(&format!(
            "| `{}` | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} |\n",
            run.label,
            run.score.landed_ms,
            run.score.landed_slots,
            run.score.landed_idx,
            run.score.same_slot,
            run.score.success_ratio,
            run.performance_rate_pct,
        ));
    }
    out
}

fn methodology_text() -> &'static str {
    "Scores follow a Beam/RPCFast-style shape but use RPCEdge artifacts: landing proof comes from matched processed/deshred observations by signature, not provider ACK. Lower-is-better metrics are normalized relative to the best run in the comparison set. Landing latency and landed slots weight p90 at 80% and average at 20%. Block position weights average and p90 equally. Same-slot and success use rates so runs with different counts remain comparable. The final performance rate is the unweighted average of the five bucket scores. Only compare runs with the same transaction shape, trigger mode, sender region, and fee/tip policy; otherwise treat the score as descriptive rather than a claim."
}

fn comparison_html(output: &ComparisonOutput) -> String {
    let mut out = String::new();
    out.push_str("<!doctype html><html><head><meta charset=\"utf-8\">");
    out.push_str(&format!("<title>{}</title>", html_escape(&output.title)));
    out.push_str(
        "<style>
body{font-family:Inter,Arial,sans-serif;margin:0;background:#f7f8fa;color:#17202a;line-height:1.45}
main{max-width:1380px;margin:0 auto;padding:32px}
h1{font-size:28px;margin:0 0 6px} h2{font-size:19px;margin:28px 0 10px}
.muted{color:#5d6975}.note{background:#fff;border-left:4px solid #4b7bec;padding:12px 14px;margin:16px 0;max-width:1100px}
.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(180px,1fr));gap:10px;margin:18px 0}
.card{background:#fff;border:1px solid #e3e7ec;border-radius:8px;padding:14px}.card b{display:block;font-size:24px}
table{width:100%;border-collapse:collapse;background:#fff;border:1px solid #e3e7ec;border-radius:8px;overflow:hidden}
th,td{padding:8px 10px;border-bottom:1px solid #edf0f3;text-align:right;font-size:13px;vertical-align:top}
th:first-child,td:first-child,td.left{text-align:left}th{background:#f0f3f6;color:#44505c;font-weight:600}
code{background:#eef1f4;padding:1px 4px;border-radius:4px}.nowrap{white-space:nowrap}
</style></head><body><main>",
    );
    out.push_str(&format!(
        "<h1>{}</h1><div class=\"muted\">Generated: <code>{}</code> · Primary source: <code>{}</code></div>",
        html_escape(&output.title),
        html_escape(&output.generated_at.to_rfc3339_opts(SecondsFormat::Millis, true)),
        html_escape(&output.primary_source),
    ));
    out.push_str("<div class=\"grid\">");
    if let Some(best) = output
        .runs
        .iter()
        .max_by(|a, b| a.performance_rate_pct.total_cmp(&b.performance_rate_pct))
    {
        out.push_str(&format!(
            "<div class=\"card\"><span class=\"muted\">Best score</span><b>{:.2}%</b><span>{}</span></div>",
            best.performance_rate_pct,
            html_escape(&best.label)
        ));
    }
    for (label, value) in [
        ("Runs", output.run_count.to_string()),
        (
            "Total txs",
            output
                .runs
                .iter()
                .map(|run| run.total_runs)
                .sum::<usize>()
                .to_string(),
        ),
        (
            "Landed txs",
            output
                .runs
                .iter()
                .map(|run| run.landed_runs)
                .sum::<usize>()
                .to_string(),
        ),
    ] {
        out.push_str(&format!(
            "<div class=\"card\"><span class=\"muted\">{}</span><b>{}</b></div>",
            html_escape(label),
            html_escape(&value)
        ));
    }
    out.push_str("</div>");
    out.push_str(&format!(
        "<div class=\"note\">{}</div>",
        html_escape(methodology_text())
    ));

    out.push_str("<h2>Comparison</h2><table><thead><tr><th>Run</th><th>Routes</th><th>Avg ACK ms</th><th>P90 ACK ms</th><th>Avg Deshred ms</th><th>P90 Deshred ms</th><th>Avg Landed ms</th><th>P90 Landed ms</th><th>Avg Landed slots</th><th>P90 Landed slots</th><th>Avg Idx</th><th>P90 Idx</th><th>Avg Priority Fee</th><th>Max Slots</th><th>Min Slots</th><th>Same-slot</th><th>Landed</th><th>Block seen</th><th>Total</th><th>Success %</th><th>Score %</th></tr></thead><tbody>");
    for run in &output.runs {
        out.push_str(&format!(
            "<tr><td class=\"left\"><code>{}</code></td><td class=\"left\"><code>{}</code></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{:.2}</td><td><b>{:.2}</b></td></tr>",
            html_escape(&run.label),
            html_escape(&run.routes),
            fmt_f64(run.avg_ack_ms),
            fmt_f64(run.p90_ack_ms),
            fmt_f64(run.avg_deshred_ms),
            fmt_f64(run.p90_deshred_ms),
            fmt_f64(run.avg_landed_ms),
            fmt_f64(run.p90_landed_ms),
            fmt_f64(run.avg_landed_slots),
            fmt_f64(run.p90_landed_slots),
            fmt_f64(run.avg_idx_in_block),
            fmt_f64(run.p90_idx_in_block),
            fmt_f64(run.avg_priority_fee_lamports),
            fmt_i64(run.max_slots),
            fmt_i64(run.min_slots),
            run.same_slot_landed,
            run.landed_runs,
            run.block_seen,
            run.total_runs,
            run.success_ratio_pct,
            run.performance_rate_pct,
        ));
    }
    out.push_str("</tbody></table>");

    out.push_str("<h2>Score Components</h2><table><thead><tr><th>Run</th><th>Landed ms</th><th>Landed slots</th><th>Block index</th><th>Same-slot</th><th>Success</th><th>Final</th></tr></thead><tbody>");
    for run in &output.runs {
        out.push_str(&format!(
            "<tr><td class=\"left\"><code>{}</code></td><td>{:.2}</td><td>{:.2}</td><td>{:.2}</td><td>{:.2}</td><td>{:.2}</td><td><b>{:.2}</b></td></tr>",
            html_escape(&run.label),
            run.score.landed_ms,
            run.score.landed_slots,
            run.score.landed_idx,
            run.score.same_slot,
            run.score.success_ratio,
            run.performance_rate_pct,
        ));
    }
    out.push_str("</tbody></table>");
    out.push_str("</main></body></html>");
    out
}

fn fmt_f64(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.3}"))
        .unwrap_or_else(|| "-".to_string())
}

fn fmt_i64(value: Option<i64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-".to_string())
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
    use tempfile::tempdir;

    #[test]
    fn comparison_scores_two_local_artifact_dirs() {
        let dir = tempdir().expect("tempdir");
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        fs::create_dir_all(&a).expect("a");
        fs::create_dir_all(&b).expect("b");
        write_run(&a, "a", 100, 200, 1);
        write_run(&b, "b", 100, 400, 2);
        let output_dir = dir.path().join("out");

        let comparison = generate_comparison(CompareOptions {
            artifact_dirs: vec![a, b],
            labels: vec!["fast".to_string(), "slow".to_string()],
            output_dir: output_dir.clone(),
            primary_source: "rpcedge_processed".to_string(),
            title: "Test Compare".to_string(),
        })
        .expect("comparison");

        assert_eq!(comparison.run_count, 2);
        assert!(comparison.runs[0].performance_rate_pct > comparison.runs[1].performance_rate_pct);
        assert!(output_dir.join("comparison.json").exists());
        assert!(output_dir.join("index.html").exists());
    }

    #[test]
    fn comparison_splits_single_paired_artifact_by_policy_arm() {
        let dir = tempdir().expect("tempdir");
        let run = dir.path().join("paired");
        fs::create_dir_all(&run).expect("run");
        write_paired_run(&run);
        let output_dir = dir.path().join("out");

        let comparison = generate_comparison(CompareOptions {
            artifact_dirs: vec![run],
            labels: vec![],
            output_dir,
            primary_source: "rpcedge_processed".to_string(),
            title: "Paired Compare".to_string(),
        })
        .expect("comparison");

        assert_eq!(comparison.run_count, 2);
        assert_eq!(comparison.runs[0].label, "always_race");
        assert_eq!(comparison.runs[1].label, "tpu_quic_only");
        assert_eq!(comparison.runs[0].total_runs, 1);
        assert_eq!(comparison.runs[1].total_runs, 1);
        assert!(comparison.runs[0].p90_landed_ms < comparison.runs[1].p90_landed_ms);
    }

    fn write_run(path: &Path, test_id: &str, send_slot: u64, observed_ms: i64, slot_delta: u64) {
        fs::write(
            path.join("samples.ndjson"),
            format!(
                "{{\"schema_version\":2,\"test_id\":\"{test_id}\",\"iteration\":0,\"signature\":\"sig-{test_id}\",\"provider_name\":\"p\",\"provider_kind\":\"rpcedge_quic_raw_tx\",\"accepted\":true,\"client_started_at\":\"2026-01-01T00:00:00Z\",\"client_finished_at\":\"2026-01-01T00:00:00.001Z\",\"client_ack_latency_us\":1000,\"provider_send_started_at\":\"2026-01-01T00:00:00Z\",\"provider_send_finished_at\":\"2026-01-01T00:00:00.001Z\",\"provider_ack_latency_us\":1000,\"provider_request_id\":null,\"returned_signature\":\"sig-{test_id}\",\"status_code\":200,\"error_class\":null,\"error\":null,\"route_policy\":\"static\",\"route_mode\":\"only\",\"selected_routes\":[\"tpu_quic\"],\"leader_client_family\":\"jito\",\"compute_unit_limit\":660,\"compute_unit_price_microlamports\":0,\"estimated_spend_lamports\":5000}}\n"
            ),
        )
        .expect("samples");
        fs::write(
            path.join("leader-sends.ndjson"),
            format!(
                "{{\"schema_version\":2,\"test_id\":\"{test_id}\",\"iteration\":0,\"signature\":\"sig-{test_id}\",\"leader_identity\":\"leader\",\"leader_run_start_slot\":{send_slot},\"leader_run_end_slot\":{},\"send_slot\":{send_slot},\"sent_at\":\"2026-01-01T00:00:00Z\",\"trigger_source\":\"grpc_slot\",\"slot_signal_status\":null,\"slot_signal_observed_at\":null,\"leader_client_family\":\"jito\",\"route_policy\":\"static\",\"selected_routes\":[\"tpu_quic\"],\"compute_unit_limit\":660,\"compute_unit_price_microlamports\":0,\"estimated_spend_lamports\":5000}}\n",
                send_slot + 3
            ),
        )
        .expect("sends");
        fs::write(
            path.join("matched-observations.ndjson"),
            format!(
                "{{\"schema_version\":1,\"test_id\":\"{test_id}\",\"signature\":\"sig-{test_id}\",\"source_name\":\"rpcedge_deshred\",\"source_kind\":\"yellowstone_deshred\",\"observed_at\":\"2026-01-01T00:00:00.{observed_ms:03}Z\",\"submitted_at\":\"2026-01-01T00:00:00Z\",\"slot\":{},\"slot_index\":null,\"source_sequence\":null}}\n{{\"schema_version\":1,\"test_id\":\"{test_id}\",\"signature\":\"sig-{test_id}\",\"source_name\":\"rpcedge_processed\",\"source_kind\":\"yellowstone_processed\",\"observed_at\":\"2026-01-01T00:00:00.{observed_ms:03}Z\",\"submitted_at\":\"2026-01-01T00:00:00Z\",\"slot\":{},\"slot_index\":10,\"source_sequence\":null}}\n",
                send_slot + slot_delta,
                send_slot + slot_delta,
            ),
        )
        .expect("observations");
    }

    fn write_paired_run(path: &Path) {
        fs::write(
            path.join("samples.ndjson"),
            concat!(
                "{\"schema_version\":3,\"test_id\":\"paired\",\"iteration\":0,\"signature\":\"sig-tpu\",\"comparison_group_id\":\"group-1\",\"policy_arm\":\"tpu_quic_only\",\"policy_arm_index\":0,\"provider_name\":\"p\",\"provider_kind\":\"rpcedge_quic_raw_tx\",\"accepted\":true,\"client_started_at\":\"2026-01-01T00:00:00Z\",\"client_finished_at\":\"2026-01-01T00:00:00.001Z\",\"client_ack_latency_us\":1000,\"provider_send_started_at\":\"2026-01-01T00:00:00Z\",\"provider_send_finished_at\":\"2026-01-01T00:00:00.001Z\",\"provider_ack_latency_us\":1000,\"provider_request_id\":null,\"returned_signature\":\"sig-tpu\",\"status_code\":200,\"error_class\":null,\"error\":null,\"route_policy\":\"tpu_quic_only\",\"route_mode\":\"only\",\"selected_routes\":[\"tpu_quic\"],\"leader_client_family\":\"jito\",\"compute_unit_limit\":660,\"compute_unit_price_microlamports\":0,\"estimated_spend_lamports\":5000}\n",
                "{\"schema_version\":3,\"test_id\":\"paired\",\"iteration\":1,\"signature\":\"sig-race\",\"comparison_group_id\":\"group-1\",\"policy_arm\":\"always_race\",\"policy_arm_index\":1,\"provider_name\":\"p\",\"provider_kind\":\"rpcedge_quic_raw_tx\",\"accepted\":true,\"client_started_at\":\"2026-01-01T00:00:00Z\",\"client_finished_at\":\"2026-01-01T00:00:00.001Z\",\"client_ack_latency_us\":1000,\"provider_send_started_at\":\"2026-01-01T00:00:00Z\",\"provider_send_finished_at\":\"2026-01-01T00:00:00.001Z\",\"provider_ack_latency_us\":1000,\"provider_request_id\":null,\"returned_signature\":\"sig-race\",\"status_code\":200,\"error_class\":null,\"error\":null,\"route_policy\":\"always_race\",\"route_mode\":\"only\",\"selected_routes\":[\"tpu_quic\",\"jito_bundle\",\"harmonic_bundle\"],\"leader_client_family\":\"jito\",\"compute_unit_limit\":660,\"compute_unit_price_microlamports\":300000,\"estimated_spend_lamports\":5001}\n"
            ),
        )
        .expect("samples");
        fs::write(
            path.join("leader-sends.ndjson"),
            concat!(
                "{\"schema_version\":3,\"test_id\":\"paired\",\"iteration\":0,\"signature\":\"sig-tpu\",\"comparison_group_id\":\"group-1\",\"policy_arm\":\"tpu_quic_only\",\"policy_arm_index\":0,\"leader_identity\":\"leader\",\"leader_run_start_slot\":100,\"leader_run_end_slot\":103,\"send_slot\":100,\"sent_at\":\"2026-01-01T00:00:00Z\",\"trigger_source\":\"grpc_slot\",\"slot_signal_status\":null,\"slot_signal_observed_at\":null,\"leader_client_family\":\"jito\",\"route_policy\":\"tpu_quic_only\",\"selected_routes\":[\"tpu_quic\"],\"compute_unit_limit\":660,\"compute_unit_price_microlamports\":0,\"estimated_spend_lamports\":5000}\n",
                "{\"schema_version\":3,\"test_id\":\"paired\",\"iteration\":1,\"signature\":\"sig-race\",\"comparison_group_id\":\"group-1\",\"policy_arm\":\"always_race\",\"policy_arm_index\":1,\"leader_identity\":\"leader\",\"leader_run_start_slot\":100,\"leader_run_end_slot\":103,\"send_slot\":100,\"sent_at\":\"2026-01-01T00:00:00Z\",\"trigger_source\":\"grpc_slot\",\"slot_signal_status\":null,\"slot_signal_observed_at\":null,\"leader_client_family\":\"jito\",\"route_policy\":\"always_race\",\"selected_routes\":[\"tpu_quic\",\"jito_bundle\",\"harmonic_bundle\"],\"compute_unit_limit\":660,\"compute_unit_price_microlamports\":300000,\"estimated_spend_lamports\":5001}\n"
            ),
        )
        .expect("sends");
        fs::write(
            path.join("matched-observations.ndjson"),
            concat!(
                "{\"schema_version\":1,\"test_id\":\"paired\",\"signature\":\"sig-tpu\",\"source_name\":\"rpcedge_processed\",\"source_kind\":\"yellowstone_processed\",\"observed_at\":\"2026-01-01T00:00:00.300Z\",\"submitted_at\":\"2026-01-01T00:00:00Z\",\"slot\":101,\"slot_index\":90,\"source_sequence\":null}\n",
                "{\"schema_version\":1,\"test_id\":\"paired\",\"signature\":\"sig-race\",\"source_name\":\"rpcedge_processed\",\"source_kind\":\"yellowstone_processed\",\"observed_at\":\"2026-01-01T00:00:00.100Z\",\"submitted_at\":\"2026-01-01T00:00:00Z\",\"slot\":100,\"slot_index\":10,\"source_sequence\":null}\n"
            ),
        )
        .expect("observations");
    }
}
