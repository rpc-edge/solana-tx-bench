use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ObservationSourceKind {
    YellowstoneProcessed,
    YellowstoneDeshred,
    NdjsonImport,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ObservationEvent {
    pub schema_version: u32,
    pub test_id: String,
    pub signature: String,
    pub source_name: String,
    pub source_kind: ObservationSourceKind,
    pub observed_at: DateTime<Utc>,
    pub submitted_at: Option<DateTime<Utc>>,
    pub slot: Option<u64>,
    pub slot_index: Option<u64>,
    pub source_sequence: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MatchedObservationSummary {
    pub schema_version: u32,
    pub test_id: String,
    pub generated_at: DateTime<Utc>,
    pub total_observations: usize,
    pub matched_signatures: usize,
    pub min_sources: usize,
    pub source_summaries: Vec<SourceObservationSummary>,
    pub pair_delta_summaries: Vec<PairDeltaSummary>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SourceObservationSummary {
    pub source_name: String,
    pub source_kind: ObservationSourceKind,
    pub observed: usize,
    pub missing_from_matched: usize,
    pub first_seen: usize,
    pub first_seen_share: f64,
    pub submit_to_observed_min_us: Option<i128>,
    pub submit_to_observed_p50_us: Option<i128>,
    pub submit_to_observed_p75_us: Option<i128>,
    pub submit_to_observed_p90_us: Option<i128>,
    pub submit_to_observed_p95_us: Option<i128>,
    pub submit_to_observed_p99_us: Option<i128>,
    pub submit_to_observed_max_us: Option<i128>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PairDeltaSummary {
    pub from_source: String,
    pub to_source: String,
    pub matched: usize,
    pub from_wins: usize,
    pub to_wins: usize,
    pub same_timestamp: usize,
    pub delta_us_min: Option<i128>,
    pub delta_us_p50: Option<i128>,
    pub delta_us_p75: Option<i128>,
    pub delta_us_p90: Option<i128>,
    pub delta_us_p95: Option<i128>,
    pub delta_us_p99: Option<i128>,
    pub delta_us_max: Option<i128>,
}

pub fn summarize_observations(
    test_id: &str,
    events: &[ObservationEvent],
    min_sources: usize,
) -> MatchedObservationSummary {
    let min_sources = min_sources.max(2);
    let mut by_signature: BTreeMap<&str, Vec<&ObservationEvent>> = BTreeMap::new();
    let mut sources = BTreeMap::<(String, ObservationSourceKind), ()>::new();
    for event in events {
        by_signature
            .entry(event.signature.as_str())
            .or_default()
            .push(event);
        sources.insert((event.source_name.clone(), event.source_kind), ());
    }

    let matched = by_signature
        .iter()
        .filter_map(|(signature, observations)| {
            let unique_sources = observations
                .iter()
                .map(|event| event.source_name.as_str())
                .collect::<BTreeSet<_>>();
            (unique_sources.len() >= min_sources).then_some((*signature, observations))
        })
        .collect::<Vec<_>>();

    let source_summaries = sources
        .keys()
        .map(|(source_name, source_kind)| {
            let mut observed = 0_usize;
            let mut missing = 0_usize;
            let mut first_seen = 0_usize;
            let mut submit_deltas = Vec::new();

            for (_, observations) in &matched {
                let source_event = observations
                    .iter()
                    .filter(|event| event.source_name == *source_name)
                    .min_by_key(|event| event.observed_at);
                if let Some(event) = source_event {
                    observed += 1;
                    if observations
                        .iter()
                        .map(|candidate| candidate.observed_at)
                        .min()
                        == Some(event.observed_at)
                    {
                        first_seen += 1;
                    }
                    if let Some(submitted_at) = event.submitted_at {
                        submit_deltas.push(
                            event
                                .observed_at
                                .signed_duration_since(submitted_at)
                                .num_microseconds()
                                .unwrap_or(0) as i128,
                        );
                    }
                } else {
                    missing += 1;
                }
            }
            submit_deltas.sort_unstable();
            SourceObservationSummary {
                source_name: source_name.clone(),
                source_kind: *source_kind,
                observed,
                missing_from_matched: missing,
                first_seen,
                first_seen_share: ratio(first_seen, matched.len()),
                submit_to_observed_min_us: submit_deltas.first().copied(),
                submit_to_observed_p50_us: percentile(&submit_deltas, 0.50),
                submit_to_observed_p75_us: percentile(&submit_deltas, 0.75),
                submit_to_observed_p90_us: percentile(&submit_deltas, 0.90),
                submit_to_observed_p95_us: percentile(&submit_deltas, 0.95),
                submit_to_observed_p99_us: percentile(&submit_deltas, 0.99),
                submit_to_observed_max_us: submit_deltas.last().copied(),
            }
        })
        .collect::<Vec<_>>();

    let source_keys = sources.keys().cloned().collect::<Vec<_>>();
    let mut pair_delta_summaries = Vec::new();
    for i in 0..source_keys.len() {
        for j in (i + 1)..source_keys.len() {
            let (from_source, _) = &source_keys[i];
            let (to_source, _) = &source_keys[j];
            let mut deltas = Vec::new();
            for (_, observations) in &matched {
                let from = observations
                    .iter()
                    .filter(|event| event.source_name == *from_source)
                    .min_by_key(|event| event.observed_at);
                let to = observations
                    .iter()
                    .filter(|event| event.source_name == *to_source)
                    .min_by_key(|event| event.observed_at);
                if let (Some(from), Some(to)) = (from, to) {
                    deltas.push(
                        to.observed_at
                            .signed_duration_since(from.observed_at)
                            .num_microseconds()
                            .unwrap_or(0) as i128,
                    );
                }
            }
            deltas.sort_unstable();
            let from_wins = deltas.iter().filter(|delta| **delta > 0).count();
            let to_wins = deltas.iter().filter(|delta| **delta < 0).count();
            let same_timestamp = deltas.iter().filter(|delta| **delta == 0).count();
            pair_delta_summaries.push(PairDeltaSummary {
                from_source: from_source.clone(),
                to_source: to_source.clone(),
                matched: deltas.len(),
                from_wins,
                to_wins,
                same_timestamp,
                delta_us_min: deltas.first().copied(),
                delta_us_p50: percentile(&deltas, 0.50),
                delta_us_p75: percentile(&deltas, 0.75),
                delta_us_p90: percentile(&deltas, 0.90),
                delta_us_p95: percentile(&deltas, 0.95),
                delta_us_p99: percentile(&deltas, 0.99),
                delta_us_max: deltas.last().copied(),
            });
        }
    }

    MatchedObservationSummary {
        schema_version: 1,
        test_id: test_id.to_string(),
        generated_at: Utc::now(),
        total_observations: events.len(),
        matched_signatures: matched.len(),
        min_sources,
        source_summaries,
        pair_delta_summaries,
    }
}

pub fn observation_summary_markdown(summary: &MatchedObservationSummary) -> String {
    let mut out = String::new();
    out.push_str("# Solana Observation Summary\n\n");
    out.push_str(&format!(
        "- Test ID: `{}`\n- Generated: `{}`\n- Observations: `{}`\n- Matched signatures: `{}`\n- Minimum sources: `{}`\n\n",
        summary.test_id,
        summary.generated_at.to_rfc3339(),
        summary.total_observations,
        summary.matched_signatures,
        summary.min_sources,
    ));
    out.push_str("## Source Summary\n\n");
    out.push_str("| Source | Kind | Observed | Missing | First seen | First seen share | p50 submit->obs us | p90 | p99 | max |\n");
    out.push_str("| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n");
    for source in &summary.source_summaries {
        out.push_str(&format!(
            "| `{}` | `{:?}` | {} | {} | {} | {:.4} | {} | {} | {} | {} |\n",
            source.source_name,
            source.source_kind,
            source.observed,
            source.missing_from_matched,
            source.first_seen,
            source.first_seen_share,
            fmt(source.submit_to_observed_p50_us),
            fmt(source.submit_to_observed_p90_us),
            fmt(source.submit_to_observed_p99_us),
            fmt(source.submit_to_observed_max_us),
        ));
    }
    out.push_str("\n## Pair Delta Summary\n\n");
    out.push_str("Positive delta means `from_source` observed before `to_source`.\n\n");
    out.push_str(
        "| From | To | Matched | From wins | To wins | Same | p50 delta us | p90 | p99 | max |\n",
    );
    out.push_str("| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n");
    for pair in &summary.pair_delta_summaries {
        out.push_str(&format!(
            "| `{}` | `{}` | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            pair.from_source,
            pair.to_source,
            pair.matched,
            pair.from_wins,
            pair.to_wins,
            pair.same_timestamp,
            fmt(pair.delta_us_p50),
            fmt(pair.delta_us_p90),
            fmt(pair.delta_us_p99),
            fmt(pair.delta_us_max),
        ));
    }
    out
}

fn percentile(sorted: &[i128], p: f64) -> Option<i128> {
    if sorted.is_empty() {
        return None;
    }
    let rank = ((sorted.len() - 1) as f64 * p).ceil() as usize;
    sorted.get(rank).copied()
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        return 0.0;
    }
    numerator as f64 / denominator as f64
}

fn fmt(value: Option<i128>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn event(
        signature: &str,
        source_name: &str,
        source_kind: ObservationSourceKind,
        us: i64,
    ) -> ObservationEvent {
        let submitted_at = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        ObservationEvent {
            schema_version: 1,
            test_id: "test".to_string(),
            signature: signature.to_string(),
            source_name: source_name.to_string(),
            source_kind,
            observed_at: submitted_at + chrono::Duration::microseconds(us),
            submitted_at: Some(submitted_at),
            slot: Some(42),
            slot_index: None,
            source_sequence: None,
        }
    }

    #[test]
    fn summarizes_matched_source_deltas_and_missing() {
        let events = vec![
            event(
                "sig-a",
                "deshred",
                ObservationSourceKind::YellowstoneDeshred,
                1_000,
            ),
            event(
                "sig-a",
                "processed",
                ObservationSourceKind::YellowstoneProcessed,
                2_000,
            ),
            event(
                "sig-a",
                "imported",
                ObservationSourceKind::NdjsonImport,
                5_000,
            ),
            event(
                "sig-b",
                "deshred",
                ObservationSourceKind::YellowstoneDeshred,
                3_000,
            ),
            event(
                "sig-b",
                "processed",
                ObservationSourceKind::YellowstoneProcessed,
                4_000,
            ),
            event(
                "sig-c",
                "deshred",
                ObservationSourceKind::YellowstoneDeshred,
                7_000,
            ),
        ];

        let summary = summarize_observations("test", &events, 2);
        assert_eq!(summary.matched_signatures, 2);
        let deshred = summary
            .source_summaries
            .iter()
            .find(|source| source.source_name == "deshred")
            .expect("deshred summary");
        assert_eq!(deshred.observed, 2);
        assert_eq!(deshred.first_seen, 2);
        assert_eq!(deshred.missing_from_matched, 0);

        let imported = summary
            .source_summaries
            .iter()
            .find(|source| source.source_name == "imported")
            .expect("imported summary");
        assert_eq!(imported.observed, 1);
        assert_eq!(imported.missing_from_matched, 1);
    }
}
