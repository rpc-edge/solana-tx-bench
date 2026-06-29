# Methodology

This benchmark is designed to compare transaction submission routes without
mixing public measurements with private infrastructure observations.

## Measurement Boundary

Public artifact boundary:

```text
signed transaction generated locally
  -> configured provider adapter
  <- provider accepted/rejected response
```

Private enrichment boundary:

```text
signature from public artifact
  -> internal shred / geyser / RPC observation tables
  -> landed slot, slot index, first observed timestamp, leader cohort
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
- external private enrichment, if used, can join by signature.

## Cohort Analysis

The public artifact includes signatures and provider timing. Cohorts such as
leader region, stake weight, or leader proximity should be computed by a
downstream analyzer that joins:

- `samples.ndjson` by `signature`;
- landed slot / slot index observations;
- leader schedule and validator metadata;
- optional first-shred/deshred observations.

That separation lets external users compare provider ACK behavior even when
they do not have private validator telemetry.

## Interpretation Rules

- Provider ACK latency is not landing latency.
- Accepted by a provider does not mean landed.
- Lowest ACK provider is not always best landed provider.
- Provider-specific fees, tips, and preflight behavior must be documented per
  adapter.
- Results should state transaction count, rate, priority fee, provider config,
  cluster, and run time.
