# RPCEdge Paired Route Policy Comparison

- Generated: `2026-07-01T10:02:02.155Z`
- Primary landing source: `rpcedge_processed`
- Runs: `3`

## Methodology

Scores follow a Beam/RPCFast-style shape but use RPCEdge artifacts: landing proof comes from matched processed/deshred observations by signature, not provider ACK. Lower-is-better metrics are normalized relative to the best run in the comparison set. Landing latency and landed slots weight p90 at 80% and average at 20%. Block position weights average and p90 equally. Same-slot and success use rates so runs with different counts remain comparable. The final performance rate is the unweighted average of the five bucket scores. Only compare runs with the same transaction shape, trigger mode, sender region, and fee/tip policy; otherwise treat the score as descriptive rather than a claim.

## Comparison

| Run | Routes | Avg ACK ms | P90 ACK ms | Avg Deshred ms | P90 Deshred ms | Avg Landed ms | P90 Landed ms | Avg Landed slots | P90 Landed slots | Avg Idx-in-Block | P90 Idx-in-Block | Avg Priority Fee | Max Slots | Min Slots | Same-slot landed | Landed runs | Block seen | Total runs | Success ratio % | Performance rate % |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `always_race` | `tpu_quic+jito_bundle+harmonic_bundle (188)` | 0.364 | 0.461 | 147.611 | 240.309 | 148.842 | 241.643 | 0.069 | 0.000 | 524.856 | 1022.000 | 198.000 | 9 | 0 | 183 | 188 | 188 | 188 | 100.00 | 100.00 |
| `software_client_aware` | `tpu_quic (36), tpu_quic+harmonic_bundle (45), tpu_quic+jito_bundle (107)` | 0.362 | 0.473 | 163.007 | 252.712 | 164.332 | 253.720 | 0.075 | 0.000 | 548.128 | 1041.000 | 47.394 | 3 | 0 | 180 | 187 | 187 | 188 | 99.47 | 97.51 |
| `tpu_quic_only` | `tpu_quic (188)` | 0.363 | 0.469 | 281.246 | 252.722 | 282.588 | 253.813 | 0.343 | 0.000 | 567.044 | 1053.000 | 0.000 | 48 | 0 | 172 | 181 | 181 | 188 | 96.28 | 91.16 |

## Score Components

| Run | Landed ms | Landed slots | Block index | Same-slot | Success | Final |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `always_race` | 100.00 | 100.00 | 100.00 | 100.00 | 100.00 | 100.00 |
| `software_client_aware` | 94.31 | 98.47 | 96.96 | 98.36 | 99.47 | 97.51 |
| `tpu_quic_only` | 86.70 | 84.04 | 94.81 | 93.99 | 96.28 | 91.16 |
