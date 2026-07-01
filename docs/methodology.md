# Methodology

This benchmark is designed to compare transaction observation latency across
Yellowstone processed and SubscribeDeshred sources, then optionally enrich
those observations with private infrastructure context.

## Measurement Boundary

Primary public artifact boundary:

```text
signed transaction generated locally
  -> configured provider adapter
  -> observed by RPCEdge Yellowstone processed and SubscribeDeshred
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
- iteration-varying transfer amount for deterministic unique signatures.

The default transaction intentionally does not include a Memo instruction. The
benchmark identifies transactions by signature and generated artifacts, so Memo
compute overhead would only make cheap latency probes more expensive. The
self-transfer avoids receiver account management and is net-neutral for
lamports, but the transaction still spends normal Solana fees and any configured
priority fee.

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

## Leader-Paced Runs

For geography and leader-cohort coverage, prefer leader-paced runs over fixed
rate runs:

```bash
cargo run --release -- run-leader-paced \
  --config bench.yaml \
  --duration-seconds 300 \
  --txs-per-leader-run 1 \
  --collect-rpcedge
```

The runner polls the current slot, fetches nearby slot leaders with
`getSlotLeaders`, groups contiguous slots with the same leader, and sends only
once per observed leader run. Each transaction is signed just-in-time with a
fresh blockhash.

Suggested ladder:

- 5 minutes, 1 tx per leader run: smoke and artifact validation.
- 30 minutes, 1 tx per leader run: first useful route comparison.
- 2 hours, 1 tx per leader run: enough samples to start inspecting p95/p99
  tails by private leader cohorts.

Run route-isolated configs first. A multi-route race measures the product-level
path, not which route would have independently landed fastest.

## Cohort Analysis

The public artifact includes signatures and observation timestamps. Cohorts such
as leader region, stake weight, validator client, or leader proximity should be
computed by a downstream analyzer that joins:

- observation events by `signature` and `slot`;
- landed slot / slot index observations;
- leader schedule and validator metadata;

That separation lets external users compare processed-vs-deshred observation
behavior even when they do not have Polaris-private validator metadata.

## Landing-Performance Buckets

Beam-style provider benchmarks use the right broad dimensions for transaction
landing comparisons. This project reports those dimensions as raw measurements
first:

- **Landing latency in milliseconds**: submit timestamp to deshred and
  processed observation by signature.
- **Landing latency in slots**: observed landed slot minus sender-observed slot
  at submission.
- **Transaction position within the block**: processed transaction
  `slot_index`, when the observation source provides it.
- **Same-slot landing rate**: share of transactions with landed-slot delta
  equal to zero.
- **Overall success rate**: observed signatures divided by submitted
  signatures.

For a single-provider or single-route run, a normalized 0-100 score is usually
misleading because there is no comparison set. For route-isolated or
multi-provider runs, the raw buckets above can be normalized against the best
provider in that run. Tail latency should receive more weight than averages
because trading systems care more about bad windows than headline mean latency.

## Matched Observation Reports

The core benchmark uses matched-signature comparisons: send
or observe the same transaction stream from multiple sources, then report which
source saw each signature first.

The first live collector is deliberately narrow:

```text
RPCEdge Yellowstone processed Subscribe
RPCEdge Yellowstone SubscribeDeshred
```

Both are read from the same endpoint and token via `.env`:

```text
RPCEDGE_GRPC_URL=...
YELLOWSTONE_X_TOKEN=...
```

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
- The primary benchmark report is matched processed/deshred observation.
- Provider-specific fees, tips, and preflight behavior must be documented per
  adapter.
- Results should state transaction count, rate, priority fee, provider config,
  cluster, and run time.
- Provider comparison claims should state whether they are ACK-only,
  confirmation-based, processed-observation-based, or first-shred/deshred-based.
