# Artifacts

Each run writes one directory under `artifact_dir`:

```text
artifacts/<test_id>/
  manifest.json
  samples.ndjson
  summary.json
  summary.md
```

## manifest.json

Run-level metadata:

- schema version;
- generated timestamp;
- redacted RPC URL label;
- payer pubkey;
- count/rate settings;
- transaction fee settings;
- provider names and kinds.

Do not put API keys, private keys, or full secret-bearing URLs into the
manifest.

## samples.ndjson

One JSON object per transaction/provider attempt.

Important fields:

- `test_id`
- `iteration`
- `signature`
- `provider_name`
- `provider_kind`
- `accepted`
- `client_started_at`
- `client_finished_at`
- `client_ack_latency_us`
- `provider_send_started_at`
- `provider_send_finished_at`
- `provider_ack_latency_us`
- `provider_request_id`
- `returned_signature`
- `status_code`
- `error_class`
- `error`

`provider_ack_latency_us` is the main public route-comparison number.
`client_ack_latency_us` is the elapsed wall time for the full client submission
round for that transaction and can include concurrent fanout effects.

## summary.json

Machine-readable summary grouped by provider:

- count;
- accepted;
- errors;
- min;
- p50;
- p75;
- p90;
- p95;
- p99;
- max.

Percentiles are based on accepted provider ACK latency.

## summary.md

Human-readable version of `summary.json`.

## Schema Compatibility

The artifact schema is intentionally append-only. Downstream analyzers should
ignore unknown fields so future versions can add:

- confirmation observation IDs;
- provider route details;
- request size;
- response size;
- landed-slot enrichment.
