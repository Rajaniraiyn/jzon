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

Apple M2, `--features simd,fast-float`, criterion 0.5

**Deserialization**

| | serde_json | sonic-rs | simd-json | jzon/A | jzon/B |
|-|-----------|---------|---------|-------|-------|
| twitter.json 617KB | 354µs | 365µs | 376µs | 360µs | 385µs |
| canada.json 2.2MB | 3.80ms | 3.32ms | 3.71ms | **2.66ms** ★ | — |
| citm_catalog 1.6MB | 1.02ms | 837µs | 907µs | **589µs** ★ | 595µs |
| micro Point 25B | 83ns | 71ns | 231ns | **47ns** ★ | 88ns |
| micro Record 52B | 92ns | 102ns | 285ns | **81ns** ★ | 108ns |

**Serialization**

| | serde_json | sonic-rs | jzon/A |
|-|-----------|---------|-------|
| twitter.json 617KB | 31.6µs | 11.5µs | **11.3µs** ★ |
| micro Record | 69ns | 61ns | **52ns** ★ |

★ = fastest. jzon/A wins on numeric/struct-heavy workloads and micro benchmarks.
Twitter de is within noise (360µs vs 354µs). Long-string serialization favours
sonic-rs which uses NEON SIMD; jzon does not yet have stable-Rust SIMD for that path.

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
