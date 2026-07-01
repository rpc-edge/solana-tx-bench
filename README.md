# solana-tx-bench

Reusable Solana transaction observation benchmark.

The tool generates small signed mainnet transactions, submits each transaction to
one or more configured providers, and writes reproducible artifacts for
Yellowstone processed and SubscribeDeshred observation analysis.

It is intentionally provider-neutral. Add a provider by implementing a sender
adapter, not by hard-coding a benchmark path.

RPCEdge links:

- Website: https://rpcedge.com
- Docs: https://docs.rpcedge.com

## What It Measures

Primary measurement:

```text
signed transaction submitted
  -> observed on Yellowstone processed gRPC
  -> observed on Yellowstone deshred / SubscribeDeshred
  -> matched by signature
  -> source win rate, missing rate, and percentile deltas
```

Provider ACK latency is retained only as a diagnostic side channel. It is not
the benchmark result.

Leader geography, validator client, datacenter, route hints, and bad-leader
context should come from the saved `getLeaderSlots` snapshot when building the
public report. Private ClickHouse joins are optional internal diagnostics, not
required for the benchmarker.

## Supported Adapters

- `solana_rpc`: standard JSON-RPC `sendTransaction`.
- `rpcedge_raw_http`: raw transaction bytes over HTTP.
- `rpcedge_route_aware_http`: RPCEdge route-aware JSON submit endpoint.
- `rpcedge_quic_raw_tx`: raw transaction bytes over persistent RPCEdge QUIC.

Planned adapter examples:

- RPCFast
- Astralane
- Helius Sender
- Harmonic
- Other provider-specific low-latency senders

## Quick Start

```bash
git clone <repo-url>
cd solana-tx-bench
cargo run -- init-config --output bench.yaml
```

Edit `bench.yaml`:

- set `keypair_path` to a funded low-value keypair;
- set `max_spend_lamports`;
- configure providers and API key environment variable names.

Run one transaction:

```bash
export RPCEDGE_API_KEY=...
cargo run -- run --config bench.yaml
```

Submission diagnostics are written under `artifact_dir/test_id/`:

- `manifest.json`
- `samples.ndjson`
- `summary.json`
- `summary.md`

Run a leader-paced smoke test:

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

This sends at most one transaction for each observed contiguous leader run,
signing each transaction with a fresh blockhash. It is the preferred first
benchmark shape because it avoids spammy fixed-rate traffic and naturally
samples different leaders over the run window.

Set `--txs-per-leader-run N --leader-run-concurrency N` to send multiple
distinct transactions at the same time for each observed leader run. Keep the
default concurrency of `1` for baseline reports unless you are intentionally
testing burst behavior.

By default, `run-leader-paced` is driven by the RPCEdge Yellowstone gRPC slot
stream (`--slot-trigger grpc_slot`). This avoids `getSlot` polling for send
timing. `--slot-trigger rpc_poll` exists only as a legacy/debug fallback.

`--capture-leader-slots` calls JSON-RPC `getLeaderSlots` before the run and
writes `leader-slots-snapshot.json` beside the samples. When pointed at
RPCEdge's RPC gateway, that snapshot can include leader geography, validator
client, route hints, and historical landing-latency profiles. This makes cohort
reports reproducible from local artifacts instead of requiring a private
database join. If `--leader-slots-rpc-url` is omitted, the runner uses the
`rpc_url` from `bench.yaml`.

In `grpc_slot` mode the runner refreshes the leader-slot snapshot when the
gRPC-observed slot approaches or leaves the cached horizon. Refresh files are
written as `leader-slots-snapshot-<start_slot>.json`.

For a QUIC-only RPCEdge sender benchmark, configure a single provider:

```yaml
providers:
  - name: rpcedge-quic-frankfurt
    kind: rpcedge_quic_raw_tx
    endpoint: "185.191.118.181:4433"
    api_key_env: "RPCEDGE_API_KEY"
    route_mode: only
    routes:
      - tpu_quic
    server_name: "relay.rpcedge.com"
```

Use `run` for a tiny fixed-count canary, then `run-leader-paced` for the
leader/cohort benchmark.

To compare TPU-only against leader-client-aware routing, run the same
leader-paced benchmark twice:

```bash
# Baseline: static TPU QUIC only from bench.yaml.
cargo run --release -- run-leader-paced \
  --config examples/rpcedge-quic-frankfurt.yaml \
  --duration-seconds 1800 \
  --slot-trigger grpc_slot \
  --capture-leader-slots \
  --collect-rpcedge

# Strategy: Jito leaders get TPU+Jito bundle, Harmonic leaders get
# TPU+Harmonic, and all other leaders stay TPU-only.
cargo run --release -- run-leader-paced \
  --config examples/rpcedge-quic-frankfurt.yaml \
  --duration-seconds 1800 \
  --slot-trigger grpc_slot \
  --route-strategy client_aware \
  --client-aware-harmonic-cu-price-microlamports 300000 \
  --capture-leader-slots \
  --collect-rpcedge
```

Use the same transaction shape and observation endpoints for both runs. Compare
submit-to-deshred, submit-to-processed, landed slot delta, processed block
`slot_index`, success ratio, and extra priority/tip cost.

Leader-paced outputs add:

- `leader-sends.ndjson`
- `leader-slots-snapshot.json`, when `--capture-leader-slots` is enabled
- `leader-slots-snapshot-<start_slot>.json`, for rolling gRPC-slot refreshes
- `matched-observations.ndjson`
- `matched-observation-summary.json`
- `matched-observation-summary.md`

Build the self-contained report:

```bash
cargo run --release -- report \
  --artifact-dir artifacts/<test_id>
```

The report command writes `report.json`, `report.md`, and `report.html` using
only local artifacts from the run directory. Leader/client/location cohorts are
computed from saved `getLeaderSlots` snapshots.

Compare two or more runs:

```bash
cargo run --release -- compare \
  --artifact-dir artifacts/<tpu_only_test_id> \
  --label "TPU QUIC only" \
  --artifact-dir artifacts/<client_aware_test_id> \
  --label "Client aware" \
  --output-dir docs/reports/<comparison_slug> \
  --title "RPCEdge TPU QUIC vs Client-Aware Routing"
```

The compare command writes `comparison.json`, `comparison.md`, `comparison.html`,
and `index.html` for GitHub Pages. It uses a Beam/RPCFast-style score across
landing milliseconds, landed slots, block position, same-slot rate, and success
ratio. Provider ACK remains a diagnostic column and is not used as landing
proof.

Collect matched observations from RPCEdge Yellowstone processed + SubscribeDeshred:

```bash
cargo run -- collect-rpcedge \
  --test-id my-run \
  --duration-seconds 120 \
  --min-sources 2
```

For end-to-end landing attribution, run either `--collect-rpcedge` during the
benchmark or run a separate RPCEdge collector for the same `test_id`. Sender ACK
artifacts alone do not prove shred or processed observation.

Observation summaries:

- `observation-summary.json`
- `observation-summary.md`

## Safety Defaults

The generated transaction is a memo-free self-transfer. The configured
`lamports` move from the keypair back to the same keypair, and the tool adds the
iteration number to the transfer amount so every transaction has a unique
signature without Memo program compute overhead. It still spends Solana fees and
any priority fee you configure.

Always set `max_spend_lamports`. This is only a local estimated fee cap; it is
not a replacement for using a throwaway keypair with tiny funds.

## Example Config

See [examples/bench.example.yaml](examples/bench.example.yaml).

## Methodology

See [docs/methodology.md](docs/methodology.md).

## Related Work

See [docs/related-work.md](docs/related-work.md) for how this repo compares to
existing Solana sender and stream benchmark tools.

## Artifact Schema

See [docs/artifacts.md](docs/artifacts.md).

## RPCEdge Enrichment Boundary

See [docs/polaris-enrichment.md](docs/polaris-enrichment.md) for the intended
boundary between portable `getLeaderSlots` enrichment and Polaris-private
gateway/customer diagnostics. Private data should not be required by this public
benchmark repo.

## Report Visualization

The artifacts are plain NDJSON and JSON. The built-in report command writes
portable `report.json`, `report.md`, and `report.html` files directly from the
artifact directory:

```bash
cargo run --release -- report --artifact-dir artifacts/<test_id>
```

Jupyter or Python notebooks can still be useful for custom charts, but they are
not required for the standard public report. Keep provider ACK charts visually
separate from processed/deshred observation charts.

Published report archive:

- https://rpc-edge.github.io/solana-tx-bench/reports/
