# jzon-rs-compat

[![crates.io](https://img.shields.io/crates/v/jzon-rs-compat.svg)](https://crates.io/crates/jzon-rs-compat)
[![docs.rs](https://docs.rs/jzon-rs-compat/badge.svg)](https://docs.rs/jzon-rs-compat)

Drop-in replacement for `serde_json` via Cargo's `[patch]` mechanism.

## Setup

Add one line to your workspace `Cargo.toml` — no code changes required:

```toml
[patch.crates-io]
serde_json = { package = "jzon-rs-compat", version = "0.1" }
```

That's it. Cargo replaces every transitive `serde_json` dependency with this crate.

## What it does

- Routes `from_str` / `to_string` through `jzon_serde`'s SIMD engine.
- Re-exports all `serde_json` public types (`Value`, `Map`, `Number`, `Error`, etc.) unchanged, so any code referencing `serde_json::Value` continues to compile without modification.

## Part of the jzon family

| Crate | Purpose |
|-------|---------|
| [jzon-rs](https://crates.io/crates/jzon-rs) | Core zero-copy JSON with `#[derive(ToJson, FromJson)]` |
| [jzon-rs-serde](https://crates.io/crates/jzon-rs-serde) | SIMD-backed serde `Serializer`/`Deserializer` |

## License

MIT
