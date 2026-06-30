# Public Report Template

Use this template for website-safe benchmark reports.

## Methodology

- Tool: `solana-tx-bench`
- Sources: RPCEdge Yellowstone processed Subscribe and RPCEdge SubscribeDeshred
- Matching key: transaction signature
- Minimum matched sources: 2
- Window: `<start>` to `<end>`
- Region: `<region>`
- Endpoint: `<redacted endpoint label>`

## Results

Paste `observation-summary.md` here.

## Interpretation

- Processed gRPC is final transaction metadata from the normal Yellowstone path.
- SubscribeDeshred is pre-processed transaction visibility from the deshred path.
- The report compares when each source first observed the same signatures.
- ACK latency is not used for the headline result.

## Links

- RPCEdge website: https://rpcedge.com
- RPCEdge docs: https://docs.rpcedge.com
- Benchmark repo: https://github.com/rpc-edge/solana-tx-bench
- Published report archive: https://rpc-edge.github.io/solana-tx-bench/reports/

## Publishing

Reviewed static reports live under:

```text
docs/reports/<test-id>/index.html
```

GitHub Pages publishes the `docs/` directory, so the public URL shape is:

```text
https://rpc-edge.github.io/solana-tx-bench/reports/<test-id>/
```
