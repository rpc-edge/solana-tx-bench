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
