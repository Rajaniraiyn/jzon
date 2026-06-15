# jzon-rs

[![crates.io](https://img.shields.io/crates/v/jzon-rs.svg)](https://crates.io/crates/jzon-rs)
[![docs.rs](https://docs.rs/jzon-rs/badge.svg)](https://docs.rs/jzon-rs)
[![CI](https://github.com/Rajaniraiyn/jzon-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/Rajaniraiyn/jzon-rs/actions)
[![MSRV](https://img.shields.io/badge/rustc-1.65%2B-blue.svg)](https://blog.rust-lang.org/2022/11/03/Rust-1.65.0.html)

Zero-copy JSON for Rust with compile-time generated parsers.

## Quick Start

```toml
[dependencies]
jzon-rs = "0.1"
```

```rust
use jzon::{ToJson, FromJson};

#[derive(ToJson, FromJson)]
struct Event<'a> {
    id: u64,
    name: &'a str,   // zero-copy — points directly into the input buffer
    tags: Vec<&'a str>,
}

fn main() {
    let src = r#"{"id":1,"name":"launch","tags":["rust","json"]}"#;
    let ev: Event = Event::from_json_str(src).unwrap();
    println!("{}", ev.to_json_string());
}
```

## Optional features

| Feature | What it adds |
|---------|-------------|
| `serde` | `jzon::from_str` / `to_string` for any `serde`-deriving type |
| `compat` | `jzon::compat` — `serde_json`-compatible API (`Value`, `json!`, etc.) |
| `simd` | u128 SWAR scanning (16 bytes/iter) |
| `fast-float` | `ryu` float serialization, `fast_float2` parsing |
| `zmij-float-ser` | Use [`zmij`](https://crates.io/crates/zmij) (Schubfach + yy_double) for float serialization instead of `ryu`. See "Float serialization backend" below for tradeoffs. MSRV 1.71. |
| `unstable` | `std::simd` portable SIMD 32–64 bytes/iter (nightly only) |
| `stats` | Allocation counters on `Scanner` |

### Float serialization backend

`zmij-float-ser` swaps `ryu` for [`zmij`](https://crates.io/crates/zmij). Wins ~30% on Linux, loses ~10% on Apple Silicon — see [#3](https://github.com/Rajaniraiyn/jzon-rs/pull/3#issuecomment-4709984480) for numbers.

### Using the serde feature

```toml
[dependencies]
jzon-rs = { version = "0.1", features = ["serde"] }
serde = { version = "1", features = ["derive"] }
```

```rust
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
struct User<'a> { id: u64, name: &'a str }

let user: User = jzon::from_str(src).unwrap();
let out = jzon::to_string(&user).unwrap();
```

### Using the compat feature

```toml
[dependencies]
jzon-rs = { version = "0.1", features = ["compat"] }
```

```rust
use jzon::compat as serde_json;  // hot-path via jzon, types from serde_json

let user: User = serde_json::from_str(src).unwrap();
let v: serde_json::Value = serde_json::from_str(src).unwrap();
```

## Highlights

- **Zero-copy** — `&'a str` fields borrow directly from the input; no heap allocation for string data.
- **SIMD scanning** — vectorised byte-search on x86-64 and aarch64 for structural character detection.
- **No `unsafe` in user code** — the derive macros emit fully safe Rust.
- **serde attribute compatibility** — `#[serde(rename = "…")]`, `#[serde(skip_serializing_if)]`, etc. are honoured by the derive macros.

## Performance

GHA matrix, criterion 0.5. Best feature combo per platform. Full table:
[`BENCHMARKS.md`](../../BENCHMARKS.md).

| Platform | twitter de | twitter ser | citm de | canada ser |
|---|--:|--:|--:|--:|
| Apple Silicon (macOS)    | 1.35 GiB/s | **53.6 GiB/s** | 2.45 GiB/s | 880 MiB/s |
| x86_64 Linux (AVX2)      | 1.22 GiB/s | 47.5 GiB/s | 2.02 GiB/s | 702 MiB/s |
| x86_64 Windows (AVX2)    | 1.18 GiB/s | 41.4 GiB/s | 2.01 GiB/s | 453 MiB/s |
| aarch64 Linux (Graviton) | 1.27 GiB/s | 39.5 GiB/s | 2.36 GiB/s | 916 MiB/s |
| Windows on ARM           | 1.15 GiB/s | 38.4 GiB/s | 2.33 GiB/s | 642 MiB/s |

vs `sonic-rs` on the same matrix: twitter ser 1.4–3× faster,
citm_catalog de +37–49% faster.

## Other Crates

| Crate | Purpose |
|-------|---------|
| [`jzon-rs-serde`](https://crates.io/crates/jzon-rs-serde) | Standalone serde `Serializer`/`Deserializer` (included via `serde` feature) |
| [`jzon-rs-compat`](https://crates.io/crates/jzon-rs-compat) | Cargo `[patch]` to replace `serde_json` for the whole dep tree |

## License

MIT

---

Made with ❤️ by [Rajaniraiyn](https://github.com/rajaniraiyn)
