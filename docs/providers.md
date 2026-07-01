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

In `run-leader-paced --route-strategy client_aware`, the runner overrides the
route set per transaction using the scheduled leader client family from the
captured `getLeaderSlots` snapshot:

- `jito`: `tpu_quic + jito_bundle`
- `harmonic`: `tpu_quic + harmonic_bundle`
- anything else or unknown: `tpu_quic`

The selected route policy and selected route list are written to
`samples.ndjson` and `leader-sends.ndjson`.

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
