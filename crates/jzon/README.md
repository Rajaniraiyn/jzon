# jzon-rs

[![crates.io](https://img.shields.io/crates/v/jzon-rs.svg)](https://crates.io/crates/jzon-rs)
[![docs.rs](https://docs.rs/jzon-rs/badge.svg)](https://docs.rs/jzon-rs)
[![CI](https://github.com/rajaniraiyn/jzon/actions/workflows/ci.yml/badge.svg)](https://github.com/rajaniraiyn/jzon/actions)

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
    let ev: Event = jzon::from_str(src).unwrap();
    println!("{}", jzon::to_string(&ev).unwrap());
}
```

## Highlights

- **Zero-copy** — `&'a str` fields borrow directly from the input; no heap allocation for string data.
- **SIMD scanning** — uses vectorised byte-search on x86-64 and aarch64 for structural character detection.
- **No `unsafe` in user code** — the derive macros emit fully safe Rust.
- **serde attribute compatibility** — `#[serde(rename = "…")]`, `#[serde(skip_serializing_if)]`, etc. are honoured by the derive macros.

## Performance

Benchmarked on Apple M2, `rustc 1.78`, release mode, `criterion` 0.5.

| Dataset | jzon | serde\_json | sonic-rs |
|---------|------|------------|---------|
| twitter de | **★ 316 µs** | 327 µs | 345 µs |
| canada de | **★ 2.43 ms** | 3.51 ms | 3.03 ms |
| micro `Point` de | **★ 41 ns** | 74 ns | 63 ns |

## Other Crates

| Crate | Purpose |
|-------|---------|
| [`jzon-rs-serde`](https://crates.io/crates/jzon-rs-serde) | SIMD-backed serde `Serializer`/`Deserializer` for any `serde`-deriving type |
| [`jzon-rs-compat`](https://crates.io/crates/jzon-rs-compat) | Drop-in `serde_json` replacement via Cargo's `[patch]` mechanism |

## License

MIT OR Apache-2.0

---

Made with ❤️ by [Rajaniraiyn](https://github.com/rajaniraiyn)
