# Methodology

This benchmark is designed to compare transaction observation latency across
Yellowstone processed and SubscribeDeshred sources, then enrich those
observations with a saved `getLeaderSlots` snapshot when available.

## Measurement Boundary

Primary public artifact boundary:

```text
signed transaction generated locally
  -> configured provider adapter
  -> observed by RPCEdge Yellowstone processed and SubscribeDeshred
  -> matched by signature
  -> source percentile and win-rate report
```

Portable enrichment boundary:

```text
signature + observed slot from public artifact
  -> getLeaderSlots snapshot captured during the run
  -> leader cohort, region, validator client, datacenter, route hints
```

The public tool should never require private ClickHouse or private validator
metadata. `getLeaderSlots` is the portable enrichment API; the saved snapshot is
part of the benchmark artifact.

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
  --leader-run-concurrency 1 \
  --slot-trigger grpc_slot \
  --capture-leader-slots \
  --collect-rpcedge
```

The default trigger is `grpc_slot`: the runner subscribes to RPCEdge
Yellowstone slot updates and sends from that stream instead of polling
`getSlot`. It uses captured `getLeaderSlots` snapshots for leader identity,
client, and geography metadata, refreshing those snapshots when the gRPC slot
stream approaches the cached horizon. `rpc_poll` remains available only as a
legacy/debug trigger.

The runner groups contiguous slots with the same leader and sends only once per
observed leader run. Each transaction is signed just-in-time with a fresh
blockhash.

`--txs-per-leader-run` controls how many distinct transactions are generated for
that leader run. `--leader-run-concurrency` controls how many of those
transactions are submitted simultaneously. Use concurrency `1` for clean
baselines; set it equal to `txs_per_leader_run` when intentionally measuring a
small burst into the same leader window.

When `--capture-leader-slots` is set, the runner writes
`leader-slots-snapshot-<start_slot>.json` refresh files, and
`leader-slots-snapshot.json` for legacy `rpc_poll` runs. Those JSON-RPC
responses are the portable enrichment source for leader geography, validator
client, route hints, and historical landing-latency priors. Store them with the
benchmark artifacts; a later report should use the saved snapshots rather than
fetching a newer leader schedule.

Suggested ladder:

- 5 minutes, 1 tx per leader run: smoke and artifact validation.
- 30 minutes, 1 tx per leader run: first useful route comparison.
- 2 hours, 1 tx per leader run: enough samples to start inspecting p95/p99
  tails by leader cohorts captured in `getLeaderSlots`.

Run route-isolated configs first. A multi-route race measures the product-level
path, not which route would have independently landed fastest.

## Route Strategies

The first RPCEdge strategy comparison should use two runs with the same
transaction shape, sender region, observation sources, and leader-paced trigger:

1. `tpu_quic_only`: static route set `only: [tpu_quic]` for every leader.
2. `client_aware`: route set chosen from the scheduled leader client family in
   the captured `getLeaderSlots` snapshot:
   - Jito leader: `only: [tpu_quic, jito_bundle]`
   - Harmonic leader: `only: [tpu_quic, harmonic_bundle]`
   - all other or unknown clients: `only: [tpu_quic]`

Run `client_aware` with:

```bash
cargo run --release -- run-leader-paced \
  --config bench.yaml \
  --route-strategy client_aware \
  --capture-leader-slots \
  --collect-rpcedge
```

The client-aware strategy intentionally requires `--capture-leader-slots`. If
the leader client family is missing, the strategy falls back to TPU-only for
that transaction and records `client_aware_tpu_only` in the artifacts.

For Harmonic leaders, the runner can raise the transaction's compute-unit price
only for that leader family:

```bash
--client-aware-harmonic-cu-price-microlamports 300000
```

As of the current Harmonic public docs, Harmonic does not describe a fixed
minimum priority-fee floor. It describes Harmonic tips as normal Solana
compute-unit priority fees, with no separate tip instruction. Treat the
Harmonic CU price as a tested benchmark variable and record rejects explicitly;
do not claim a provider minimum unless Harmonic publishes one or live tests prove
one.

For Jito leaders, `jito_bundle` is an internal RPCEdge route in this benchmark.
The relay adds the Jito bundle tip transaction when the route executes. The
benchmark's public fast ACK may not include the added tip signature; private
RPCEdge route-attempt telemetry records `route_tip_signature`,
`route_tip_lamports`, and `route_tip_account` for route-causal analysis.

Observation collection is required for landing metrics. Without
`--collect-rpcedge` or a separate collector keyed by the same `test_id`, the
benchmark only knows provider/client ACKs, which are diagnostics rather than
landing evidence.

For RPCEdge QUIC, ACK means the relay accepted and enqueued the request. It is
not proof that a TPU/JET backend later sent the transaction successfully.
Backend callback failures such as leader-side QUIC `disallowed` must be
interpreted from relay route telemetry and the final deshred/processed
observation rate.

## Cohort Analysis

The public artifact includes signatures, observation timestamps, and optionally
a `getLeaderSlots` snapshot. Cohorts such as leader region, stake weight,
validator client, or leader proximity should be computed from:

- observation events by `signature` and `slot`;
- landed slot / slot index observations;
- the saved `leader-slots-snapshot.json` response;

That separation lets external users compare processed-vs-deshred observation
behavior without Polaris-private ClickHouse access. Private RPCEdge reports can
still join deeper internal telemetry such as route-attempt internals or customer
dimensions, but the public report should be reproducible from local artifacts.

Generate the standalone report with:

```bash
solana-tx-bench report --artifact-dir artifacts/<test_id>
```

This command reads only local artifacts from the run directory and writes
`report.json`, `report.md`, and `report.html`.

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

## Cost-Normalized Comparison

For `tpu_quic_only` versus `client_aware`, report both raw landing quality and
cost-normalized deltas:

- deshred submit-to-observed p50/p90/p99;
- processed submit-to-observed p50/p90/p99;
- landed slot delta p50/p90;
- processed `slot_index` p50/p90/p99;
- observed success ratio;
- extra priority fee lamports versus the TPU-only baseline;
- extra route tip lamports for Jito bundle attempts;
- milliseconds improved per additional lamport;
- block-index positions improved per additional lamport.

The cost floor for the generated transaction is:

```text
base signature fee + ceil(CU limit * CU price microlamports / 1_000_000)
```

Jito route-added bundle tips are not visible in the transaction itself. Use
RPCEdge route-attempt telemetry for the actual Jito tip cost when available.
If only public artifacts are available, label Jito tip cost as unknown or an
estimate rather than mixing it into exact cost-normalized scores.

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
