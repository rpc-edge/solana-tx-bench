# solana-tx-bench

Reusable Solana transaction sender benchmark.

The tool generates small signed mainnet transactions, submits each transaction to
one or more configured providers, and writes reproducible artifacts for route
latency analysis.

It is intentionally provider-neutral. Add a provider by implementing a sender
adapter, not by hard-coding a benchmark path.

## What It Measures

This public repo measures client-to-provider ACK latency:

```text
bench process
  -> provider endpoint
  <- accepted / rejected response
```

It does not claim landing latency by itself. Landing, first-shred seen, processed
slot, leader geography, and private deshred observations require external data
and can be joined downstream using the transaction signature.

## Supported Adapters

- `solana_rpc`: standard JSON-RPC `sendTransaction`.
- `rpcedge_raw_http`: raw transaction bytes over HTTP.
- `rpcedge_route_aware_http`: RPCEdge route-aware JSON submit endpoint.

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

Artifacts are written under `artifact_dir/test_id/`:

- `manifest.json`
- `samples.ndjson`
- `summary.json`
- `summary.md`

## Safety Defaults

The generated transaction is a self-transfer with a memo. The configured
`lamports` move from the keypair back to the same keypair, but the transaction
still spends Solana fees and any priority fee you configure.

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

## Private First-Shred Enrichment

See [docs/polaris-enrichment.md](docs/polaris-enrichment.md) for the intended
private join path. That data should not be required by this public benchmark
repo.
