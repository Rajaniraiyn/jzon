# jzon-rs

[![Crates.io](https://img.shields.io/crates/v/jzon-rs.svg)](https://crates.io/crates/jzon-rs)
[![Docs.rs](https://docs.rs/jzon-rs/badge.svg)](https://docs.rs/jzon-rs)
[![CI](https://github.com/Rajaniraiyn/jzon-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/Rajaniraiyn/jzon-rs/actions)
[![MSRV](https://img.shields.io/badge/rustc-1.65%2B-blue.svg)](https://blog.rust-lang.org/2022/11/03/Rust-1.65.0.html)

Zero-copy JSON for Rust. A proc-macro generates a typed, monomorphised
parser and serializer per struct at compile time — no runtime dispatch,
no intermediate `Value`, no unnecessary allocations.

## Three modes

### Mode A — custom derive (fastest)

Add `jzon-rs`. The `derive` feature is on by default.

```toml
[dependencies]
jzon-rs = "0.1"
```

```rust
use jzon::{ToJson, FromJson};

#[derive(ToJson, FromJson)]
#[serde(rename_all = "camelCase")]
struct User<'a> {
    id:    u64,
    name:  &'a str,  // zero-copy: borrows directly from the input bytes
    score: f64,
}

let user = User::from_json_str(input)?;
let out  = user.to_json_string();
```

### Mode B — any serde type

Add `jzon-rs-serde`. No other changes to your code.

```toml
[dependencies]
jzon-rs-serde = "0.1"
serde = { version = "1", features = ["derive"] }
```

```rust
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
struct User<'a> { id: u64, name: &'a str }

let user: User = jzon_serde::from_str(input)?;
let out = jzon_serde::to_string(&user)?;
```

### Mode C — drop-in for serde_json

Add one line to your workspace `Cargo.toml`. Zero code changes required — every
`serde_json` call across your entire dep tree (reqwest, axum, etc.) routes
through jzon automatically.

```toml
[patch.crates-io]
serde_json = { package = "jzon-rs-compat", version = "0.1" }
```

## Features

### jzon-rs

| Feature | Default | What it does |
|---------|---------|-------------|
| `derive` | ✓ | `#[derive(ToJson, FromJson)]` proc-macros |
| `simd` | | u128 SWAR (16 bytes/iter) |
| `simd-intrinsics` | | Hand-written `std::arch` kernels — aarch64 NEON, x86_64 SSE2/AVX2 |
| `simd + unstable` | | `std::simd` portable SIMD, 32–64 bytes/iter (nightly) |
| `fast-float` | | ryu for serialization, fast_float2 for parsing |
| `zmij-float-ser` | | [zmij](https://crates.io/crates/zmij) (Schubfach+yy) float ser instead of ryu. ~30 % faster on Linux, ~10 % slower on Apple Silicon. MSRV 1.71. |
| `stats` | | per-parse allocation counters on Scanner |

### jzon-rs-serde / jzon-rs-compat

Both crates expose the same flags: `simd`, `fast-float`, `unstable`, `stats`.
`jzon-rs-compat` also has `fast-float` **on by default** (sensible for a
drop-in replacement).

## Benchmarks

GHA matrix, criterion 0.5, `bench_cmp` workloads. Best feature combo per
platform — full matrix in [`BENCHMARKS.md`](./BENCHMARKS.md).

| Platform | twitter de | twitter ser | citm de | canada ser |
|---|--:|--:|--:|--:|
| Apple Silicon (M-series macOS) | 1.35 GiB/s | **53.6 GiB/s** | 2.45 GiB/s | 880 MiB/s |
| x86_64 Linux (AVX2)            | 1.22 GiB/s | 47.5 GiB/s | 2.02 GiB/s | 702 MiB/s |
| x86_64 Windows (AVX2)          | 1.18 GiB/s | 41.4 GiB/s | 2.01 GiB/s | 453 MiB/s |
| aarch64 Linux (Graviton)       | 1.27 GiB/s | 39.5 GiB/s | 2.36 GiB/s | 916 MiB/s |
| Windows on ARM (aarch64)       | 1.15 GiB/s | 38.4 GiB/s | 2.33 GiB/s | 642 MiB/s |

**Head-to-head** (range across all 5 platforms × 4 feature combos):

| Workload | vs `serde_json` | vs `sonic-rs` | vs `simd-json` |
|---|---|---|---|
| `twitter` serialize        | **2.1–3.6× faster** | **1.2–2.4× faster** | — (no ser bench) |
| `citm_catalog` deserialize | **1.6–2.4× faster** | 1.3–1.7× faster     | up to **4× faster** |
| `deep_nested` deserialize  | 1.4–2.0× faster     | 1.5–2.2× faster     | up to **7.8× faster** |
| `string_heavy` deserialize | +10–17%             | _0.86–0.97× (slight loss)_ | 1.2–2.3× faster |
| `twitter` deserialize      | _parity_ (0.84–0.97×) | _parity_ (0.84–1.13×) | 1.3–2.3× faster |
| `canada` deserialize       | _loses on Linux/macOS_ | _loses on Linux/macOS_ | mixed |

Honest gaps: `canada` deserialize loses on Linux/macOS where sonic-rs's
SIMD float parser wins; `string_heavy` deserialize loses to sonic-rs's
SIMD string scan by 3–14%; `twitter` deserialize is at parity. Wins are
serialization and structural-heavy deserialization.

## How it works

**Deserialization** — the derive macro generates a field-dispatch loop where
keys ≤ 8 bytes compare as a single `u64` (one CPU instruction). A one-word
*field-hint* variable predicts the next key; for in-order JSON this makes
almost every dispatch O(1) without hashing. `&'de str` fields borrow directly
from the input — no allocation unless the string contains escape sequences.
With `fast-float`, floats are parsed in one pass via `fast_float2`.

**Serialization** — field keys are compile-time `b"\"name\":"` byte literals.
Integer and float rendering use `ryu`/custom digit writers. String escaping
bulk-copies safe byte runs using SWAR u64/u128 arithmetic (or `std::simd`
on nightly), falling back to per-byte for escape characters.

**Serde layer** — `jzon-rs-serde` wraps the same scanner behind a
`serde::Serializer`/`Deserializer`. `visit_borrowed_str` propagates zero-copy
borrowing to any `#[derive(Deserialize)]` type.

## Serde attributes supported

`rename`, `rename_all` (8 modes), `skip`, `skip_serializing`,
`skip_deserializing`, `skip_serializing_if`, `default`, `alias`,
`deny_unknown_fields`, `tag` (internally-tagged enums), `transparent`.

Types: all primitives, `String`, `&'de str`, `Option<T>`, `Vec<T>`,
`HashMap`, `BTreeMap`, `char`, `()`, tuples 1–12, `u128`/`i128`,
newtype structs, tuple structs, enum struct variants.

---

Made with ❤️ by [Rajaniraiyn](https://github.com/rajaniraiyn)
