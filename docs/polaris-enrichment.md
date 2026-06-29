# Polaris Private Enrichment

This public repository should stop at generic sender artifacts. Polaris-specific
landing and shred observations should remain private.

## Join Key

Use `samples.ndjson.signature`.

## Private Observation Flow

```text
solana-tx-bench artifacts
  signature, provider, provider_ack_latency_us
        |
        v
private observation store
  first deshred seen timestamp
  processed geyser timestamp
  landed slot
  slot index
  leader identity
  leader region/cohort
        |
        v
private report
  provider ACK -> first shred seen
  provider ACK -> processed update
  submit -> landed by leader cohort
```

## Why This Is Private

First-shred/deshred visibility depends on validator placement, stream provider
contracts, private ClickHouse tables, and internal gateway observability. Public
users can still reuse the benchmark by joining the signatures against their own
observation systems.

## Suggested Private Output

Private enrichment can write a separate file next to public artifacts:

```text
enriched_samples.parquet
```

Suggested fields:

- public sample fields;
- `observed_source`;
- `first_shred_seen_at`;
- `processed_seen_at`;
- `landed_slot`;
- `slot_index`;
- `leader_identity`;
- `leader_region`;
- `leader_stake_bucket`;
- `ack_to_first_shred_us`;
- `ack_to_processed_us`;
- `submit_to_first_shred_us`;
- `submit_to_processed_us`.
