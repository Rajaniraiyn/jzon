# jzon-rs-serde

[![crates.io](https://img.shields.io/crates/v/jzon-rs-serde.svg)](https://crates.io/crates/jzon-rs-serde)
[![docs.rs](https://docs.rs/jzon-rs-serde/badge.svg)](https://docs.rs/jzon-rs-serde)

SIMD-backed serde `Serializer`/`Deserializer` for any type deriving `serde::Serialize`/`serde::Deserialize`.

## Usage

```toml
[dependencies]
jzon-rs-serde = "0.1"
serde = { version = "1", features = ["derive"] }
```

```rust
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct User<'a> {
    id: u64,
    name: &'a str,   // zero-copy via visit_borrowed_str
}

fn main() {
    let src = r#"{"id":42,"name":"ada"}"#;
    let user: User = jzon_serde::from_str(src).unwrap();
    let out: String = jzon_serde::to_string(&user).unwrap();
    println!("{out}");
}
```

Zero-copy `&str` fields work transparently: the deserializer calls `visit_borrowed_str`, so the string data is borrowed directly from the input slice with no allocation.

## Feature Flags

Feature flags mirror those of [jzon-rs](https://crates.io/crates/jzon-rs).

| Flag | Default | Description |
|------|---------|-------------|
| `simd` | on | Enable SIMD structural scanning |
| `float-ryu` | on | Use `ryu` for fast float serialization |
| `int-itoa` | on | Use `itoa` for fast integer serialization |

## Part of the jzon family

| Crate | Purpose |
|-------|---------|
| [jzon-rs](https://crates.io/crates/jzon-rs) | Core zero-copy JSON with `#[derive(ToJson, FromJson)]` |
| [jzon-rs-compat](https://crates.io/crates/jzon-rs-compat) | Drop-in `serde_json` replacement |

## License

MIT OR Apache-2.0
