# jzon-rs-derive

[![crates.io](https://img.shields.io/crates/v/jzon-rs-derive.svg)](https://crates.io/crates/jzon-rs-derive)
[![docs.rs](https://docs.rs/jzon-rs-derive/badge.svg)](https://docs.rs/jzon-rs-derive)

Proc-macro crate for `#[derive(ToJson, FromJson)]` — part of [jzon](https://crates.io/crates/jzon-rs).

> **Do not add this crate directly.**
> Add `jzon-rs` instead — it re-exports both macros automatically.

## Example

```rust
use jzon::{ToJson, FromJson};

#[derive(ToJson, FromJson)]
struct Point {
    x: f64,
    y: f64,
}
```

## See Also

- [jzon-rs](https://crates.io/crates/jzon-rs) — main crate and full documentation
- [GitHub](https://github.com/Rajaniraiyn/jzon-rs)

## License

MIT
