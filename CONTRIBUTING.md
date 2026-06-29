# Contributing

Contributions should keep the benchmark provider-neutral and reproducible.

## Adapter Contract

New send providers should implement `ProviderAdapter` in `src/adapters.rs`.

An adapter should:

- use the configured transaction bytes exactly as generated;
- record provider send start and finish timestamps;
- return accepted/rejected status without hiding provider errors;
- avoid logging API keys, private keys, or complete request headers;
- support deterministic config through YAML.

An adapter should not:

- mutate the transaction unless the adapter contract explicitly says so;
- add hidden tips or route-specific fees;
- write provider credentials into generated artifacts;
- make product claims from provider ACK latency alone.

## Tests

Before opening a PR:

```bash
cargo fmt --check
cargo test
cargo check
```

Use unit tests for payload shape and provider response parsing. Use local mock
HTTP servers for adapter behavior; do not require live provider credentials in
CI.

## Adding A Provider

1. Add a variant to `ProviderConfig`.
2. Add the matching `ProviderKind`.
3. Implement `ProviderAdapter`.
4. Add an example config section.
5. Document provider-specific caveats in `docs/providers.md`.
6. Add tests for success, provider rejection, malformed response, and transport
   failure.
