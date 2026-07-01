# Transaction Landing Report: `leader-paced-2026-07-01T095542.712Z`

- Generated: `2026-07-01T10:02:02.145Z`
- Artifact dir: `/home/sol/solana-tx-bench/artifacts/leader-paced-2026-07-01T095542.712Z`
- Sent transactions: `564`
- Provider samples: `564`
- Observations: `1112`
- Matched signatures: `556`
- Leader-slot snapshots: `10`

This report is generated only from local benchmark artifacts. Leader metadata comes from saved `getLeaderSlots` snapshots captured during the run; no ClickHouse or private database join is required.

## Provider ACK

| Provider | Policy | Routes | Count | Accepted | Errors | p50 ms | p90 ms | p99 ms | max ms |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `rpcedge-quic-frankfurt` | `always_race` | `tpu_quic,jito_bundle,harmonic_bundle` | 188 | 188 | 0 | 0.358 | 0.461 | 0.630 | 0.666 |
| `rpcedge-quic-frankfurt` | `software_client_aware_harmonic` | `tpu_quic,harmonic_bundle` | 45 | 45 | 0 | 0.351 | 0.463 | 0.628 | 0.628 |
| `rpcedge-quic-frankfurt` | `software_client_aware_jito` | `tpu_quic,jito_bundle` | 107 | 107 | 0 | 0.349 | 0.469 | 0.556 | 0.636 |
| `rpcedge-quic-frankfurt` | `software_client_aware_tpu_only` | `tpu_quic` | 36 | 36 | 0 | 0.366 | 0.483 | 0.548 | 0.548 |
| `rpcedge-quic-frankfurt` | `tpu_quic_only` | `tpu_quic` | 188 | 188 | 0 | 0.354 | 0.469 | 0.652 | 0.663 |

Provider ACK is a sender diagnostic, not landing proof.

## Observation Sources

| Source | Count | First seen | p50 ms | p90 ms | p99 ms | max ms | same-slot | slot delta p50 | slot index p50 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `rpcedge_deshred` | 556 | 556 | 103.957 | 248.073 | 1732.464 | 20843.274 | 535 / 96.2% | 0 | - |
| `rpcedge_processed` | 556 | 0 | 105.168 | 248.895 | 1734.634 | 20847.957 | 535 / 96.2% | 0 | 479 |

## Cohort: `leader_region`

| Value | Source | Count | p50 ms | p90 ms | p99 ms | max ms |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `Europe` | `rpcedge_deshred` | 373 | 91.343 | 248.073 | 3060.467 | 20843.274 |
| `Europe` | `rpcedge_processed` | 373 | 92.318 | 248.895 | 3062.746 | 20847.957 |
| `AsiaPacific` | `rpcedge_deshred` | 62 | 216.581 | 252.218 | 692.045 | 692.045 |
| `AsiaPacific` | `rpcedge_processed` | 62 | 217.883 | 253.556 | 692.902 | 692.902 |
| `Unknown` | `rpcedge_deshred` | 57 | 95.259 | 143.022 | 149.070 | 149.070 |
| `Unknown` | `rpcedge_processed` | 57 | 95.805 | 144.616 | 150.069 | 150.069 |
| `UsEast` | `rpcedge_deshred` | 33 | 159.340 | 593.374 | 1640.582 | 1640.582 |
| `UsEast` | `rpcedge_processed` | 33 | 159.623 | 594.837 | 1641.694 | 1641.694 |
| `UsCentral` | `rpcedge_deshred` | 19 | 129.251 | 960.849 | 1027.783 | 1027.783 |
| `UsCentral` | `rpcedge_processed` | 19 | 130.623 | 961.727 | 1031.371 | 1031.371 |
| `UsWest` | `rpcedge_deshred` | 12 | 172.568 | 172.650 | 172.654 | 172.654 |
| `UsWest` | `rpcedge_processed` | 12 | 173.197 | 174.216 | 174.396 | 174.396 |

## Cohort: `leader_city`

| Value | Source | Count | p50 ms | p90 ms | p99 ms | max ms |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `Amsterdam` | `rpcedge_deshred` | 106 | 93.997 | 147.660 | 591.192 | 20843.274 |
| `Amsterdam` | `rpcedge_processed` | 106 | 94.746 | 149.637 | 592.374 | 20847.957 |
| `Frankfurt am Main` | `rpcedge_deshred` | 102 | 92.440 | 257.873 | 3060.467 | 3060.475 |
| `Frankfurt am Main` | `rpcedge_processed` | 102 | 94.041 | 259.680 | 3062.746 | 3062.905 |
| `City of London` | `rpcedge_deshred` | 33 | 96.064 | 110.116 | 145.724 | 145.724 |
| `City of London` | `rpcedge_processed` | 33 | 97.893 | 111.664 | 146.673 | 146.673 |
| `Vilnius` | `rpcedge_deshred` | 33 | 64.319 | 112.696 | 148.468 | 148.468 |
| `Vilnius` | `rpcedge_processed` | 33 | 65.785 | 114.196 | 148.939 | 148.939 |
| `London` | `rpcedge_deshred` | 30 | 122.837 | 417.882 | 431.414 | 431.414 |
| `London` | `rpcedge_processed` | 30 | 123.581 | 419.814 | 432.045 | 432.045 |
| `Tokyo` | `rpcedge_deshred` | 24 | 224.091 | 287.809 | 692.045 | 692.045 |
| `Tokyo` | `rpcedge_processed` | 24 | 224.683 | 288.866 | 692.902 | 692.902 |
| `Beauharnois` | `rpcedge_deshred` | 21 | 120.333 | 149.054 | 149.070 | 149.070 |
| `Beauharnois` | `rpcedge_processed` | 21 | 122.633 | 149.684 | 150.069 | 150.069 |
| `Asia/Singapore` | `rpcedge_deshred` | 20 | 186.327 | 223.355 | 223.366 | 223.366 |
| `Asia/Singapore` | `rpcedge_processed` | 20 | 187.240 | 224.522 | 224.533 | 224.533 |
| `Rödelheim` | `rpcedge_deshred` | 15 | 89.633 | 407.455 | 407.461 | 407.461 |
| `Rödelheim` | `rpcedge_processed` | 15 | 90.094 | 408.665 | 408.671 | 408.671 |
| `Singapore` | `rpcedge_deshred` | 12 | 173.536 | 232.882 | 232.887 | 232.887 |
| `Singapore` | `rpcedge_processed` | 12 | 174.547 | 235.814 | 235.824 | 235.824 |
| `Stockholm` | `rpcedge_deshred` | 12 | 46.746 | 65.099 | 65.104 | 65.104 |
| `Stockholm` | `rpcedge_processed` | 12 | 47.580 | 66.379 | 66.385 | 66.385 |
| `America/Chicago` | `rpcedge_deshred` | 10 | 56.901 | 1027.783 | 1027.783 | 1027.783 |
| `America/Chicago` | `rpcedge_processed` | 10 | 59.952 | 1031.371 | 1031.371 | 1031.371 |

## Cohort: `data_center_key`

| Value | Source | Count | p50 ms | p90 ms | p99 ms | max ms |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `20326-NL-Amsterdam` | `rpcedge_deshred` | 51 | 86.540 | 112.298 | 168.085 | 168.085 |
| `20326-NL-Amsterdam` | `rpcedge_processed` | 51 | 87.153 | 113.740 | 169.645 | 169.645 |
| `20326-DE-Frankfurt am Main` | `rpcedge_deshred` | 30 | 87.326 | 257.873 | 257.887 | 257.887 |
| `20326-DE-Frankfurt am Main` | `rpcedge_processed` | 30 | 88.006 | 259.680 | 259.919 | 259.919 |
| `16125-LT-Vilnius` | `rpcedge_deshred` | 24 | 64.305 | 112.682 | 112.696 | 112.696 |
| `16125-LT-Vilnius` | `rpcedge_processed` | 24 | 65.543 | 113.970 | 114.196 | 114.196 |
| `16276-CA-Beauharnois` | `rpcedge_deshred` | 21 | 120.333 | 149.054 | 149.070 | 149.070 |
| `16276-CA-Beauharnois` | `rpcedge_processed` | 21 | 122.633 | 149.684 | 150.069 | 150.069 |
| `396356-DE-Frankfurt am Main` | `rpcedge_deshred` | 21 | 105.440 | 468.584 | 468.599 | 468.599 |
| `396356-DE-Frankfurt am Main` | `rpcedge_processed` | 21 | 106.909 | 469.923 | 469.940 | 469.940 |
| `20326-JP-Tokyo` | `rpcedge_deshred` | 18 | 217.135 | 236.562 | 236.567 | 236.567 |
| `20326-JP-Tokyo` | `rpcedge_processed` | 18 | 218.679 | 237.665 | 237.671 | 237.671 |
| `29066-DE-Frankfurt am Main` | `rpcedge_deshred` | 18 | 34.760 | 92.141 | 92.152 | 92.152 |
| `29066-DE-Frankfurt am Main` | `rpcedge_processed` | 18 | 36.908 | 93.986 | 93.996 | 93.996 |
| `399460-GB-City of London` | `rpcedge_deshred` | 18 | 69.502 | 96.536 | 99.067 | 99.067 |
| `399460-GB-City of London` | `rpcedge_processed` | 18 | 71.117 | 98.071 | 100.369 | 100.369 |
| `60068-NL-Amsterdam` | `rpcedge_deshred` | 18 | 94.627 | 138.905 | 154.037 | 154.037 |
| `60068-NL-Amsterdam` | `rpcedge_processed` | 18 | 95.240 | 139.699 | 154.739 | 154.739 |
| `395201-DE-Rödelheim` | `rpcedge_deshred` | 15 | 89.633 | 407.455 | 407.461 | 407.461 |
| `395201-DE-Rödelheim` | `rpcedge_processed` | 15 | 90.094 | 408.665 | 408.671 | 408.671 |
| `45102-SG-Asia/Singapore` | `rpcedge_deshred` | 15 | 177.136 | 216.576 | 216.581 | 216.581 |
| `45102-SG-Asia/Singapore` | `rpcedge_processed` | 15 | 177.903 | 218.494 | 218.944 | 218.944 |
| `20326-NL-Europe/Amsterdam` | `rpcedge_deshred` | 13 | 107.205 | 318.671 | 20843.274 | 20843.274 |
| `20326-NL-Europe/Amsterdam` | `rpcedge_processed` | 13 | 109.443 | 321.516 | 20847.957 | 20847.957 |

## Cohort: `client_family`

| Value | Source | Count | p50 ms | p90 ms | p99 ms | max ms |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `bam` | `rpcedge_deshred` | 221 | 95.245 | 219.013 | 323.889 | 692.045 |
| `bam` | `rpcedge_processed` | 221 | 95.752 | 221.381 | 324.621 | 692.902 |
| `harmonic` | `rpcedge_deshred` | 128 | 107.213 | 556.428 | 3060.475 | 20843.274 |
| `harmonic` | `rpcedge_processed` | 128 | 109.855 | 557.282 | 3062.905 | 20847.957 |
| `jito` | `rpcedge_deshred` | 99 | 78.985 | 222.547 | 3891.270 | 3891.270 |
| `jito` | `rpcedge_processed` | 99 | 79.434 | 224.180 | 3892.847 | 3892.847 |
| `rakurai` | `rpcedge_deshred` | 60 | 114.227 | 263.896 | 468.599 | 468.599 |
| `rakurai` | `rpcedge_processed` | 60 | 116.388 | 264.890 | 469.940 | 469.940 |
| `frankendancer` | `rpcedge_deshred` | 27 | 113.629 | 417.882 | 431.414 | 431.414 |
| `frankendancer` | `rpcedge_processed` | 27 | 115.282 | 419.814 | 432.045 | 432.045 |
| `agave` | `rpcedge_deshred` | 18 | 64.305 | 172.650 | 172.654 | 172.654 |
| `agave` | `rpcedge_processed` | 18 | 65.543 | 174.216 | 174.396 | 174.396 |
| `firedancer` | `rpcedge_deshred` | 3 | 108.102 | 108.108 | 108.108 | 108.108 |
| `firedancer` | `rpcedge_processed` | 3 | 108.758 | 108.793 | 108.793 | 108.793 |

## Cohort: `client_software`

| Value | Source | Count | p50 ms | p90 ms | p99 ms | max ms |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `AgaveBam` | `rpcedge_deshred` | 221 | 95.245 | 219.013 | 323.889 | 692.045 |
| `AgaveBam` | `rpcedge_processed` | 221 | 95.752 | 221.381 | 324.621 | 692.902 |
| `HarmonicAgave` | `rpcedge_deshred` | 104 | 104.684 | 272.131 | 1027.783 | 20843.274 |
| `HarmonicAgave` | `rpcedge_processed` | 104 | 105.832 | 272.694 | 1031.371 | 20847.957 |
| `JitoLabs` | `rpcedge_deshred` | 99 | 78.985 | 222.547 | 3891.270 | 3891.270 |
| `JitoLabs` | `rpcedge_processed` | 99 | 79.434 | 224.180 | 3892.847 | 3892.847 |
| `Rakurai` | `rpcedge_deshred` | 60 | 114.227 | 263.896 | 468.599 | 468.599 |
| `Rakurai` | `rpcedge_processed` | 60 | 116.388 | 264.890 | 469.940 | 469.940 |
| `Frankendancer` | `rpcedge_deshred` | 27 | 113.629 | 417.882 | 431.414 | 431.414 |
| `Frankendancer` | `rpcedge_processed` | 27 | 115.282 | 419.814 | 432.045 | 432.045 |
| `HarmonicFrankendancer` | `rpcedge_deshred` | 24 | 219.116 | 1732.479 | 3060.475 | 3060.475 |
| `HarmonicFrankendancer` | `rpcedge_processed` | 24 | 221.821 | 1734.647 | 3062.905 | 3062.905 |
| `Agave` | `rpcedge_deshred` | 18 | 64.305 | 172.650 | 172.654 | 172.654 |
| `Agave` | `rpcedge_processed` | 18 | 65.543 | 174.216 | 174.396 | 174.396 |
| `Firedancer` | `rpcedge_deshred` | 3 | 108.102 | 108.108 | 108.108 | 108.108 |
| `Firedancer` | `rpcedge_processed` | 3 | 108.758 | 108.793 | 108.793 | 108.793 |

## Cohort: `stake_bucket`

| Value | Source | Count | p50 ms | p90 ms | p99 ms | max ms |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `>=1m_sol` | `rpcedge_deshred` | 389 | 103.143 | 236.552 | 1126.618 | 20843.274 |
| `>=1m_sol` | `rpcedge_processed` | 389 | 104.567 | 237.655 | 1127.818 | 20847.957 |
| `100k_500k_sol` | `rpcedge_deshred` | 84 | 113.132 | 248.046 | 1732.479 | 1732.479 |
| `100k_500k_sol` | `rpcedge_processed` | 84 | 115.428 | 248.878 | 1734.647 | 1734.647 |
| `500k_1m_sol` | `rpcedge_deshred` | 66 | 107.205 | 272.131 | 3060.475 | 3060.475 |
| `500k_1m_sol` | `rpcedge_processed` | 66 | 109.443 | 272.694 | 3062.905 | 3062.905 |
| `<100k_sol` | `rpcedge_deshred` | 17 | 153.662 | 264.660 | 264.669 | 264.669 |
| `<100k_sol` | `rpcedge_processed` | 17 | 154.618 | 265.764 | 265.778 | 265.778 |

## Tail Events

| Signature | Source | Latency ms | Send slot | Landed slot | Slot index | Region | City | Client |
| --- | --- | ---: | ---: | ---: | ---: | --- | --- | --- |
| `4ucVyy7c...9ihuu45F` | `rpcedge_processed` | 20847.957 | 430070084 | 430070132 | 711 | `Europe` | `Amsterdam` | `HarmonicAgave` |
| `4ucVyy7c...9ihuu45F` | `rpcedge_deshred` | 20843.274 | 430070084 | 430070132 | - | `Europe` | `Amsterdam` | `HarmonicAgave` |
| `5fM5SzgS...SLkFt5Lv` | `rpcedge_processed` | 3892.847 | 430070136 | 430070145 | 1376 | `Europe` | `Offenbach` | `JitoLabs` |
| `5fM5SzgS...SLkFt5Lv` | `rpcedge_deshred` | 3891.270 | 430070136 | 430070145 | - | `Europe` | `Offenbach` | `JitoLabs` |
| `5LbB7t11...ENHR5SMP` | `rpcedge_processed` | 3062.905 | 430070128 | 430070131 | 2129 | `Europe` | `Frankfurt am Main` | `HarmonicFrankendancer` |
| `3QR4Dq7E...eTutVeWx` | `rpcedge_processed` | 3062.746 | 430070128 | 430070131 | 2127 | `Europe` | `Frankfurt am Main` | `HarmonicFrankendancer` |
| `5LbB7t11...ENHR5SMP` | `rpcedge_deshred` | 3060.475 | 430070128 | 430070131 | - | `Europe` | `Frankfurt am Main` | `HarmonicFrankendancer` |
| `3QR4Dq7E...eTutVeWx` | `rpcedge_deshred` | 3060.467 | 430070128 | 430070131 | - | `Europe` | `Frankfurt am Main` | `HarmonicFrankendancer` |
| `4mvyy7zy...CQzf34PJ` | `rpcedge_processed` | 1734.647 | 430069792 | 430069795 | 1319 | `Europe` | `Frankfurt am Main` | `HarmonicFrankendancer` |
| `2ap5KW4m...Ceus495k` | `rpcedge_processed` | 1734.634 | 430069792 | 430069795 | 1317 | `Europe` | `Frankfurt am Main` | `HarmonicFrankendancer` |
| `4mvyy7zy...CQzf34PJ` | `rpcedge_deshred` | 1732.479 | 430069792 | 430069795 | - | `Europe` | `Frankfurt am Main` | `HarmonicFrankendancer` |
| `2ap5KW4m...Ceus495k` | `rpcedge_deshred` | 1732.464 | 430069792 | 430069795 | - | `Europe` | `Frankfurt am Main` | `HarmonicFrankendancer` |
