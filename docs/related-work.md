# Related Work

This project is an observation benchmark. It should borrow sender ergonomics
from small RPC benchmark scripts, but its core report must be matched
gRPC/deshred/shredstream observation latency.

## bloXroute Benchmark Article

Reference:
[Benchmarking Solana Transaction Speeds and Landing Rates](https://bloxroute.com/pulse/benchmarking-solana-transaction-speeds-and-landing-rates/)

Useful ideas:

- compare providers with the same transaction shape and fee settings;
- report landing rate, p90 latency, and slot offset;
- state endpoint modes clearly, for example swQoS versus best-effort paths.

Caveat for this repo:

- provider-written results are useful methodology references, but public claims
  need reproducible artifacts and a clear observation source;
- this repo's base submission output is only diagnostic; benchmark claims must
  come from matched observation events.

## dysnix/solana-test

Reference:
[dysnix/solana-test](https://github.com/dysnix/solana-test)

Relevant subtools observed:

- `sendtx-bench-rs`: Solana sendTransaction benchmark with Yellowstone-style
  landing tracking, blocks subscription for index-in-block, slot-edge dispatch,
  per-provider sender isolation, configurable tips, HTTP and Beam QUIC support.
- `yellowstone-bench`: compares two Yellowstone gRPC endpoints by matching
  transaction signatures and reporting which endpoint saw each transaction
  first.
- `geyser-vs-shredstream`: compares transaction timestamps between Yellowstone
  and ShredStream observations.

What to borrow:

- observer layer that can match by signature and compute gRPC/deshred/
  shredstream observation latency;
- matched-signature win-rate reporting;
- p75/p90/p95/p99 delta reporting;
- sender isolation when comparing providers concurrently;
- slot-edge dispatch as a separate benchmark mode.

What not to copy directly:

- private provider API assumptions;
- hardcoded tip accounts;
- combined public/private measurement logic in the base benchmark path.

## Astralane ping-things-rs

Reference:
[Astralane/ping-things-rs](https://github.com/Astralane/ping-things-rs)

Useful ideas:

- YAML-configured provider matrix;
- repeated runs with delay/rate controls;
- compute-budget and priority-fee knobs;
- provider type adapters such as Solana RPC, Jito, and bloXroute.

What this repo already does similarly:

- YAML config;
- provider adapter model;
- compute budget and priority fee config;
- repeated generated transactions with a memo identity.

What to add later:

- CSV output for easier spreadsheet use;
- route/provider preset examples;
- optional provider-specific tip instruction builders.

## memobench

Reference:
[benjiewheeler/memobench](https://github.com/benjiewheeler/memobench)

Useful ideas:

- simple UX for one endpoint;
- explicit warning that benchmark transactions spend real fees;
- optional separate send RPC and WebSocket observation RPC;
- rate limiting to avoid 429s;
- success rate plus confirmation timing.

What to borrow:

- single-provider quickstart mode;
- optional WebSocket/RPC observation mode for users without a private geyser
  stream;
- clearer spend warnings in generated config.

## Yellowstone Jet

Reference:
[rpcpool/yellowstone-jet](https://github.com/rpcpool/yellowstone-jet)

This is not a benchmark tool; it is a transaction sender/proxy. It is still
important related infrastructure because it exposes the kind of route surface a
benchmark should be able to target:

- QUIC TPU sender;
- JSON-RPC-compatible sendTransaction server;
- raw HTTP transaction endpoint;
- swQoS support;
- Prometheus metrics;
- dynamic identity and policy controls.

This repo should benchmark systems like Jet through adapters, then evaluate the
result by matched gRPC/deshred/shredstream observation.

## RPCFast Public Methodology

Reference:
[RPCFast Solana Node Performance](https://docs.rpcfast.com/solana-dedicated-nodes/solana-node-performance)

Useful ideas:

- compare matching transactions across two streams;
- report average delta and p75/p90/p95/p99;
- report percentage of matched transactions where one source was faster.

This is exactly the style to use for observer output. Public artifacts should
keep raw observation events so downstream analyzers can compute the same style
of report.

## Design Implications For This Repo

The repo should include these layers:

1. **Base runner:** generate signed transactions, fan them to configured
   adapters, and record submission diagnostics.
2. **Public observer, primary scope:** gRPC/deshred/shredstream signature
   observation with matched-source percentile reports.
3. **Provider adapters, ongoing scope:** clean adapters for RPCEdge, Solana
   JSON-RPC, Helius, Harmonic, RPCFast, Astralane, bloXroute, and other sender
   APIs.
4. **Private enrichment, out of public scope:** Polaris first-shred/deshred,
   processed-slot, leader geography, customer dimensions, and ClickHouse joins.

The most important product distinction:

```text
Public repo: reproducible matched observation samples and source percentiles.
Private Polaris layer: leader-region, bad-leader, validator-client, customer,
and ClickHouse cohort context.
```
