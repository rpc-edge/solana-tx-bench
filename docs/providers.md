# Provider Notes

Provider behavior is not uniform. Keep adapter-specific assumptions here.

## Solana JSON-RPC

Adapter: `solana_rpc`

Sends:

```json
{
  "method": "sendTransaction",
  "params": [
    "<base64 transaction>",
    {
      "encoding": "base64",
      "skipPreflight": true,
      "maxRetries": 0
    }
  ]
}
```

The provider ACK only confirms that the RPC accepted or rejected the request.

## RPCEdge Raw HTTP

Adapter: `rpcedge_raw_http`

Sends raw transaction bytes with `content-type: application/octet-stream`.
Optional API key is read from `api_key_env` and sent as `x-api-key`.

## RPCEdge Route-Aware HTTP

Adapter: `rpcedge_route_aware_http`

Sends a JSON body with transaction base64 and route selection. Route selection
supports:

- `server_default`
- `only`
- `default_plus`
- `default_minus`

The route list is adapter-specific. Example:

```yaml
route_mode: only
routes:
  - tpu_quic
```

## RPCEdge QUIC Raw Transaction

Adapter: `rpcedge_quic_raw_tx`

Uses a persistent QUIC connection to the RPCEdge relay and sends raw
transactions with an RPCEdge route set. Static route selection uses the
configured `route_mode` and `routes`.

In `run-leader-paced --route-strategy always_race`, the runner overrides the
route set per transaction to:

- `tpu_quic + jito_bundle + harmonic_bundle`

This strategy is deliberately expensive. Use it as a control to measure whether
extra provider routes improve landing quality enough to justify the added
priority/tip spend.

In `run-leader-paced --route-strategy software_client_aware`, the runner
overrides the route set per transaction using the scheduled leader's
`client.software` and `client.softwareClientId` from the captured
`getLeaderSlots` snapshot:

- `JitoLabs` / ID `1`: `tpu_quic + jito_bundle`
- `AgaveBam` / ID `6`: `tpu_quic + jito_bundle`
- `FireBAM` / ID `12`: `tpu_quic + jito_bundle`
- `HarmonicAgave` / ID `10`: `tpu_quic + harmonic_bundle`
- `HarmonicFiredancer` / ID `9`: `tpu_quic + harmonic_bundle`
- `HarmonicFrankendancer` / ID `11`: `tpu_quic + harmonic_bundle`
- anything else or unknown: `tpu_quic`

The selected route policy and selected route list are written to
`samples.ndjson` and `leader-sends.ndjson`.

The older `client_aware` strategy is still available for old report
reproducibility, but it uses only normalized `client.family` and is too coarse
for BAM/Rakurai/Raiku semantics.

Single-policy `run-leader-paced` invocations are route smoke tests. They are not
valid A/B comparisons against other policies unless the artifacts come from a
paired multi-policy run where all policies are sent inside the same leader
window and comparison group.

For paired comparison runs, use:

```bash
solana-tx-bench run-leader-paced \
  --route-strategy paired_route_policies \
  --slot-trigger grpc_slot \
  --capture-leader-slots \
  --collect-rpcedge
```

This sends `tpu_quic_only`, `always_race`, and `software_client_aware` arms
concurrently for every leader run. The configured QUIC provider remains the
transport; the per-transaction RPCEdge route set is overridden by the selected
arm.

## Harmonic

Harmonic bundles use ordinary Solana compute-unit priority fees as the economic
signal. There is no Jito-style transfer to a tip account for Harmonic, and the
current public docs do not state a fixed minimum priority-fee floor.

Benchmark implication: configure the Harmonic CU price explicitly, record it in
the manifest and samples, and treat live provider rejects as evidence. Do not
document a fixed Harmonic minimum unless Harmonic publishes one or repeated live
tests establish one.

## Jito Bundle Route

RPCEdge's internal `jito_bundle` route adds a separate Jito tip transaction when
the route executes. The amount is controlled by relay-side configuration and
Jito tip-priority logic, not by the caller transaction's compute-unit price.

Private RPCEdge route-attempt telemetry records the added tip signature,
lamports, and account. Public benchmark artifacts should not assume exact Jito
tip cost unless that route-attempt data or route-detailed ACK data is present.

## Adding External Providers

External provider adapters should be added as isolated config variants. Good
candidate examples:

- RPCFast;
- Astralane;
- Helius Sender;
- Harmonic.
- bloXroute;
- Nozomi / Temporal-style senders;
- TPU sender proxies such as Yellowstone Jet.

Provider-specific fee floors, minimum priority fees, payload limits, preflight
rules, and bundle rules must be documented with the adapter.

## Adapter Design Notes From Related Tools

Provider adapters should keep protocol-specific behavior isolated:

- HTTP JSON-RPC senders should expose preflight, retry, and encoding settings.
- Raw HTTP senders should record request size and status code.
- QUIC senders should document connection reuse, stream model, and whether the
  provider returns an ACK or is fire-and-forget.
- Bundle senders should document bundle size limits and tip requirements.
- Provider-specific tip builders should be opt-in and visible in config.

When comparing providers concurrently, prefer one funded sender keypair per
provider. Shared fee-payer accounts can serialize or otherwise couple traffic
and make route comparisons less clean.
