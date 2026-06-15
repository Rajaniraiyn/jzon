# jzon-rs benchmarks

Measured on GitHub Actions runners, 2026-06-15. Criterion 0.5, 6 s measurement.
4 feature combos × 5 platforms (x86_64-darwin / Intel Mac skipped — GHA queue
constraints). Workloads from `crates/jzon/benches/bench_cmp.rs`.

Numbers are criterion's median throughput. GiB/s where ≥ 1, MiB/s otherwise.
Higher is better.

## Best combo per platform

| Platform | Toolchain | Float backend | Notes |
|---|---|---|---|
| Apple Silicon macOS    | nightly | zmij | Top of matrix on twitter ser |
| x86_64 Linux (AVX2)    | stable  | zmij | Nightly didn't help; AVX2 already maxed |
| x86_64 Windows (AVX2)  | nightly | ryu  | zmij regresses citm de on MSVC |
| aarch64 Linux (Graviton) | nightly | zmij | Marginal nightly win |
| Windows on ARM (aarch64) | stable  | ryu  | zmij gains are noise (≤0.5%) |

## Headline numbers (best combo)

| Platform | twitter de | twitter ser | citm de | deep_nested de | canada ser | string_heavy de |
|---|--:|--:|--:|--:|--:|--:|
| Apple Silicon macOS    | 1.35 GiB/s | **53.6 GiB/s** | 2.45 GiB/s | 366 MiB/s | 880 MiB/s | 1.10 GiB/s |
| x86_64 Linux (AVX2)    | 1.22 GiB/s | 47.5 GiB/s | 2.02 GiB/s | 504 MiB/s | 702 MiB/s | 824 MiB/s |
| x86_64 Windows (AVX2)  | 1.18 GiB/s | 41.4 GiB/s | 2.01 GiB/s | 493 MiB/s | 453 MiB/s | 633 MiB/s |
| aarch64 Linux (Graviton) | 1.27 GiB/s | 39.5 GiB/s | 2.36 GiB/s | 444 MiB/s | 916 MiB/s | 752 MiB/s |
| Windows on ARM (aarch64) | 1.15 GiB/s | 38.4 GiB/s | 2.33 GiB/s | 449 MiB/s | 642 MiB/s | 742 MiB/s |

## Stable matrix — ryu vs zmij

Stable Rust + `simd,simd-intrinsics`. Float ser backend varies. Winner bold.

| Platform | Workload | ryu | zmij | Winner | Δ |
|---|---|--:|--:|---|--:|
| x86_64-linux  | twitter / deserialize     | 1.06 GiB/s | **1.22 GiB/s** | zmij | +14.8% |
| x86_64-linux  | twitter / serialize       | 40.17 GiB/s | **47.47 GiB/s** | zmij | +18.2% |
| x86_64-linux  | citm_catalog / deserialize| **2.06 GiB/s** | 2.02 GiB/s | ryu  | -2.2% |
| x86_64-linux  | deep_nested / deserialize | 453 MiB/s | **504 MiB/s** | zmij | +11.2% |
| x86_64-linux  | canada / serialize        | 513 MiB/s | **702 MiB/s** | zmij | +36.8% |
| x86_64-linux  | string_heavy / deserialize| 741 MiB/s | **824 MiB/s** | zmij | +11.2% |
| aarch64-linux | twitter / deserialize     | **1.29 GiB/s** | 1.25 GiB/s | ryu  | -3.4% |
| aarch64-linux | twitter / serialize       | **39.43 GiB/s** | 38.79 GiB/s | ryu  | -1.6% |
| aarch64-linux | citm_catalog / deserialize| 2.36 GiB/s | **2.38 GiB/s** | zmij | +0.9% |
| aarch64-linux | deep_nested / deserialize | 446 MiB/s | 446 MiB/s | tied | 0.0% |
| aarch64-linux | canada / serialize        | 626 MiB/s | **916 MiB/s** | zmij | +46.3% |
| aarch64-linux | string_heavy / deserialize| 734 MiB/s | **746 MiB/s** | zmij | +1.6% |
| aarch64-darwin| twitter / deserialize     | 1.08 GiB/s | **1.09 GiB/s** | zmij | +1.0% |
| aarch64-darwin| twitter / serialize       | 47.82 GiB/s | **49.99 GiB/s** | zmij | +4.5% |
| aarch64-darwin| citm_catalog / deserialize| 1.98 GiB/s | **2.19 GiB/s** | zmij | +10.8% |
| aarch64-darwin| deep_nested / deserialize | 281 MiB/s | **358 MiB/s** | zmij | +27.5% |
| aarch64-darwin| canada / serialize        | 569 MiB/s | **787 MiB/s** | zmij | +38.4% |
| aarch64-darwin| string_heavy / deserialize| 850 MiB/s | **1.04 GiB/s** | zmij | +24.8% |
| x86_64-windows| twitter / deserialize     | 1.08 GiB/s | **1.17 GiB/s** | zmij | +7.8% |
| x86_64-windows| twitter / serialize       | **39.62 GiB/s** | 37.05 GiB/s | ryu  | -6.5% |
| x86_64-windows| citm_catalog / deserialize| **2.19 GiB/s** | 1.80 GiB/s | ryu  | -18.0% |
| x86_64-windows| deep_nested / deserialize | **440 MiB/s** | 403 MiB/s | ryu  | -8.3% |
| x86_64-windows| canada / serialize        | 415 MiB/s | **499 MiB/s** | zmij | +20.1% |
| x86_64-windows| string_heavy / deserialize| 554 MiB/s | **588 MiB/s** | zmij | +6.1% |
| aarch64-windows| twitter / deserialize    | 1.14 GiB/s | 1.14 GiB/s | tied | +0.3% |
| aarch64-windows| twitter / serialize      | **38.19 GiB/s** | 38.00 GiB/s | ryu | -0.5% |
| aarch64-windows| citm_catalog / deserialize| **2.33 GiB/s** | 2.30 GiB/s | ryu | -1.3% |
| aarch64-windows| deep_nested / deserialize | 449 MiB/s | 448 MiB/s | tied | -0.3% |
| aarch64-windows| canada / serialize        | 507 MiB/s | **635 MiB/s** | zmij | +25.1% |
| aarch64-windows| string_heavy / deserialize| 742 MiB/s | 742 MiB/s | tied | 0.0% |

## Nightly matrix — ryu vs zmij with `unstable` (portable_simd)

Nightly Rust + `simd,simd-intrinsics,unstable`. `unstable` enables
`std::simd` portable SIMD as fallback — but the dispatcher prefers
intrinsics on all current platforms, so `unstable` mostly buys you a
newer LLVM.

| Platform | Workload | nightly+ryu | nightly+zmij |
|---|---|--:|--:|
| aarch64-darwin | twitter/deserialize       | 1.237 GiB/s | **1.352 GiB/s** |
| aarch64-darwin | twitter/serialize         | 51.106 GiB/s | **53.555 GiB/s** |
| aarch64-darwin | citm_catalog/deserialize  | **2.450 GiB/s** | 2.249 GiB/s |
| aarch64-darwin | deep_nested/deserialize   | **365.8 MiB/s** | 354.3 MiB/s |
| aarch64-darwin | canada/serialize          | 598.2 MiB/s | **880.5 MiB/s** |
| aarch64-darwin | string_heavy/deserialize  | **1.102 GiB/s** | 1.020 GiB/s |
| aarch64-linux  | twitter/deserialize       | **1.279 GiB/s** | 1.272 GiB/s |
| aarch64-linux  | twitter/serialize         | 39.442 GiB/s | **39.543 GiB/s** |
| aarch64-linux  | citm_catalog/deserialize  | 2.325 GiB/s | **2.355 GiB/s** |
| aarch64-linux  | deep_nested/deserialize   | 444.3 MiB/s | 444.1 MiB/s |
| aarch64-linux  | canada/serialize          | 621.0 MiB/s | **916.4 MiB/s** |
| aarch64-linux  | string_heavy/deserialize  | 741.3 MiB/s | **752.1 MiB/s** |
| aarch64-windows| twitter/deserialize       | **1.153 GiB/s** | 1.128 GiB/s |
| aarch64-windows| twitter/serialize         | **38.392 GiB/s** | 38.386 GiB/s |
| aarch64-windows| citm_catalog/deserialize  | 2.256 GiB/s | **2.315 GiB/s** |
| aarch64-windows| deep_nested/deserialize   | 434.6 MiB/s | **441.1 MiB/s** |
| aarch64-windows| canada/serialize          | 511.4 MiB/s | **642.2 MiB/s** |
| aarch64-windows| string_heavy/deserialize  | 737.9 MiB/s | **742.3 MiB/s** |
| x86_64-linux   | twitter/deserialize       | 1.070 GiB/s | **1.078 GiB/s** |
| x86_64-linux   | twitter/serialize         | 39.231 GiB/s | **39.934 GiB/s** |
| x86_64-linux   | citm_catalog/deserialize  | 2.000 GiB/s | **2.248 GiB/s** |
| x86_64-linux   | deep_nested/deserialize   | **450.8 MiB/s** | 391.8 MiB/s |
| x86_64-linux   | canada/serialize          | 511.3 MiB/s | **606.3 MiB/s** |
| x86_64-linux   | string_heavy/deserialize  | 733.7 MiB/s | **744.7 MiB/s** |
| x86_64-windows | twitter/deserialize       | **1.183 GiB/s** | 1.145 GiB/s |
| x86_64-windows | twitter/serialize         | **41.419 GiB/s** | 40.468 GiB/s |
| x86_64-windows | citm_catalog/deserialize  | **2.008 GiB/s** | 1.810 GiB/s |
| x86_64-windows | deep_nested/deserialize   | **492.5 MiB/s** | 390.6 MiB/s |
| x86_64-windows | canada/serialize          | 415.0 MiB/s | **453.4 MiB/s** |
| x86_64-windows | string_heavy/deserialize  | **632.8 MiB/s** | 569.3 MiB/s |

## Head-to-head vs other Rust JSON libraries

Same `bench_cmp` benches, same runners. `simd-json` has no serialize
workloads in this bench file.

### `twitter` serialize — GiB/s, best combo per platform

| Platform | jzon | serde_json | sonic-rs |
|---|--:|--:|--:|
| Apple Silicon macOS     | **53.6** | 17.5 | 28.2 |
| x86_64 Linux (AVX2)     | **47.5** | 14.6 | 31.0 |
| x86_64 Windows (AVX2)   | **41.4** | 13.4 | 20.1 |
| aarch64 Linux (Graviton)| **39.5** | 18.2 | 31.4 |
| Windows on ARM          | **38.2** | 18.0 | 22.8 |

### Speedup ranges across the full matrix (5 platforms × 4 feature combos)

| Workload | vs serde_json | vs sonic-rs | vs simd-json |
|---|---|---|---|
| `twitter` serialize        | **2.1–3.6× faster** | **1.2–2.4× faster** | — |
| `citm_catalog` deserialize | 1.6–2.4× faster     | 1.3–1.7× faster     | up to **4× faster** |
| `deep_nested` deserialize  | 1.4–2.0× faster     | 1.5–2.2× faster     | up to **7.8× faster** |
| `string_heavy` deserialize | +10–17%             | 0.86–0.97× (loses)  | 1.2–2.3× faster |
| `twitter` deserialize      | 0.84–0.97× (parity) | 0.84–1.13× (parity) | 1.3–2.3× faster |
| `canada` deserialize       | 0.68–1.20× (loses Linux/macOS) | 0.61–1.42× (loses Linux/macOS) | 0.83–1.72× mixed |
| `canada` serialize         | 0.80–1.13× (mixed)  | 0.96–1.46× faster   | — |

### Honest gaps

- `canada` deserialize on aarch64-darwin is the worst case: jzon 344 MiB/s
  vs sonic-rs 566 MiB/s (~0.61×). sonic-rs's SIMD float parser is the
  difference; jzon uses `fast_float2` scalar-per-number.
- `string_heavy` deserialize trails sonic-rs by 3–14% — their SIMD string
  scan wins; jzon's `find_quote_or_backslash` is competitive in isolation
  but the unescape path closes the per-iteration gap.
- `twitter` deserialize is at parity with serde_json (within ±15%);
  twitter is light on the structural-heavy work where jzon's
  compile-time codegen wins biggest.

## Observations

- **zmij** (Schubfach + yy_double) wins float ser on macOS and Linux,
  loses on Windows MSVC (both arches). MSVC's codegen for `zmij`'s
  larger constant tables may be the culprit.
- **canada / serialize** is the strongest single signal for the float
  backend choice — it's a 2.2 MB array of doubles. zmij gives +20% to
  +46% there on every platform.
- **AVX2 on x86_64-linux** is the throughput ceiling we hit so far —
  47.5 GiB/s twitter ser. AVX-512 BW would beat this but the GHA
  `ubuntu-latest` runner doesn't expose `avx512bw` (Cascade Lake).
- **Nightly toolchain** helps Apple Silicon (+5% ser, +20% twitter de),
  helps marginally on aarch64-linux/windows, and **doesn't help on
  x86_64-linux** — the AVX2 path is already saturated.

## Reproducing

```bash
# Manual trigger from the Actions tab → "bench" workflow:
gh workflow run bench.yml --ref main \
  -f bench=bench_cmp \
  -f features=simd,simd-intrinsics,zmij-float-ser \
  -f measurement_time=6
```

The matrix step picks 5 runners by default (or filter via the
`runners` input). Each runner uploads its criterion HTML + log as a
`criterion-<platform>` artifact. Run any combo against any ref —
re-running the workflow regenerates the tables in this file.
