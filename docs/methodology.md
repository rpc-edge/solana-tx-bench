# Methodology

This benchmark is designed to compare transaction observation latency across
gRPC/deshred/shredstream sources, then optionally enrich those observations with
private infrastructure context.

## Measurement Boundary

Primary public artifact boundary:

```text
signed transaction generated locally
  -> configured provider adapter
  -> observed by configured gRPC/deshred/shredstream sources
  -> matched by signature
  -> source percentile and win-rate report
```

Private enrichment boundary:

```text
signature + observed slot from public artifact
  -> internal shred / geyser / RPC observation tables
  -> leader cohort, region, validator client, datacenter, customer dimensions
```

The public tool should never require private ClickHouse, validator, deshred, or
gateway access.

## Transaction Shape

Each generated transaction contains:

- compute-unit-limit instruction;
- optional compute-unit-price instruction;
- self-transfer from the funded keypair back to itself;
- memo with benchmark identifier and iteration.

This keeps the transaction easy to identify while avoiding receiver account
management. It still spends normal Solana fees and any configured priority fee.

## Recommended Run Ladder

Start with correctness:

```text
count=1, one provider
count=1, each provider individually
count=5, all providers
```

Then run low-rate benchmarks:

```text
duration_seconds=120
rate_per_second=0.2, 0.5, 1.0
```

Only increase rate after:

- provider rejects are understood;
- spend cap is correct;
- artifact persistence is clean;
- observation collectors can join by signature.

## Cohort Analysis

The public artifact includes signatures and observation timestamps. Cohorts such
as leader region, stake weight, validator client, or leader proximity should be
computed by a downstream analyzer that joins:

- observation events by `signature` and `slot`;
- landed slot / slot index observations;
- leader schedule and validator metadata;

That separation lets external users compare gRPC/deshred/shredstream
observation behavior even when they do not have Polaris-private validator
metadata.

## Matched Observation Reports

The core benchmark uses matched-signature comparisons: send
or observe the same transaction stream from multiple sources, then report which
source saw each signature first.

Good matched-observation outputs:

- matched transaction count;
- unmatched count per source;
- win rate by source;
- average delta;
- p75/p90/p95/p99 deltas;
- landed slot and slot index when available.

Observation events include the stable join key: `signature`.

## Interpretation Rules

- Provider ACK latency is not landing latency and is not the benchmark result.
- Accepted by a provider does not mean landed.
- Lowest ACK provider is not always best landed provider.
- The primary benchmark report is matched gRPC/deshred/shredstream observation.
- Provider-specific fees, tips, and preflight behavior must be documented per
  adapter.
- Results should state transaction count, rate, priority fee, provider config,
  cluster, and run time.
- Provider comparison claims should state whether they are ACK-only,
  confirmation-based, processed-observation-based, or first-shred/deshred-based.
