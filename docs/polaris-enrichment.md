# RPCEdge And Private Enrichment

This public repository should produce generic matched observation artifacts and
can optionally capture public/customer-safe `getLeaderSlots` context. Deeper
Polaris-specific joins remain private.

## Boundary

Keep this repository focused on portable collection:

- send benchmark transactions;
- write request/signature artifacts;
- optionally collect generic Yellowstone processed and SubscribeDeshred
  observations;
- optionally capture JSON-RPC `getLeaderSlots` snapshots;
- let users join those signatures and slots against their own metadata when they
  are not using RPCEdge enrichment.

Do not add RPCEdge ClickHouse table names, customer/key dimensions, private
validator-location snapshots, or report hosting paths here. Public reports can
use saved `leader-slots-snapshot.json` files when the endpoint returns
customer-safe leader metadata.

## Join Key

Use `ObservationEvent.signature` and, where available, `ObservationEvent.slot`.
Use `leader-slots-snapshot.json` as the portable leader/cohort lookup table for
the run.

## Portable RPCEdge Flow

```text
solana-tx-bench artifacts
  processed/deshred observation events
  leader-slots-snapshot.json
        |
        v
local report
  observation latency by leader cohort
  observation latency by validator client
  observation latency by datacenter/region
```

## Private Observation Flow

```text
solana-tx-bench artifacts
  processed/deshred observation events
  matched source percentile summary
        |
        v
private metadata store
  customer/key dimensions
  internal route-attempt telemetry
  gateway persistence diagnostics
  private profile-builder state
        |
        v
private report
  observation latency by leader cohort
  observation latency by validator client
  observation latency by datacenter/region
  bad leader and tail attribution
```

## What Remains Private

Customer dimensions, private ClickHouse tables, and internal gateway
observability are Polaris-specific. Public users can still reuse the benchmark by
capturing `getLeaderSlots` from RPCEdge or by joining signatures and slots
against their own metadata.

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

Store latency artifacts in microseconds for precision. Human reports should
format those columns as milliseconds.
