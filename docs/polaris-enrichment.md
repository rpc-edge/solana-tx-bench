# Polaris Private Enrichment

This public repository should produce generic matched observation artifacts.
Polaris-specific grouping and context should remain private.

## Join Key

Use `ObservationEvent.signature` and, where available, `ObservationEvent.slot`.

## Private Observation Flow

```text
solana-tx-bench artifacts
  processed/deshred observation events
  matched source percentile summary
        |
        v
private metadata store
  leader identity
  leader region/cohort
  validator client
  datacenter
  customer/key dimensions
        |
        v
private report
  observation latency by leader cohort
  observation latency by validator client
  observation latency by datacenter/region
  bad leader and tail attribution
```

## Why This Is Private

Leader cohort, validator client, customer dimensions, private ClickHouse tables,
and internal gateway observability are Polaris-specific. Public users can still
reuse the benchmark by joining signatures and slots against their own metadata.

## Suggested Private Output

Private enrichment can write a separate file next to public artifacts:

```text
enriched_samples.parquet
```

Suggested fields:

- public observation fields;
- `leader_identity`;
- `leader_region`;
- `leader_stake_bucket`;
- `leader_client`;
- `leader_datacenter`;
- `submit_to_observed_us`.
