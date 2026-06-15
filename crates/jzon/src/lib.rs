//! **jzon** — purpose-built, zero-copy JSON serialization for specific structs.
//!
//! # Design
//!
//! Use `#[derive(ToJson, FromJson)]` to generate a **monomorphised** JSON
//! (de)serializer for each of your types at compile time.  No generic visitor
//! indirection, no intermediate `Value` allocation, no format-string overhead.
//!
//! ## Cargo features
//!
//! | Feature | Default | Effect |
//! |---------|---------|--------|
//! | `derive` | ✓ | `#[derive(ToJson, FromJson)]` proc-macros |
//! | `simd` | | u128 SWAR scanning (16 B/iter) |
//! | `simd + unstable` | | `std::simd` portable SIMD (32–64 B/iter, nightly) |
//! | `fast-float` | | `ryu` serialization, `fast_float2` parsing |
//! | `stats` | | `ScannerStats` allocation/cache-hit counters |
//!
//! For serde integration see [`jzon-rs-serde`](https://crates.io/crates/jzon-rs-serde).
//! For a `serde_json` drop-in see [`jzon-rs-compat`](https://crates.io/crates/jzon-rs-compat).
//!
//! ## Zero-copy deserialization
//!
//! Fields typed `&'de str` borrow **directly** from the input — no `String` is
//! allocated unless the JSON string contains escape sequences.
//!
//! ## Field-hint cache
//!
//! The generated `FromJson` impl maintains a one-word *field-hint* variable
//! that predicts which field key to expect next.  For JSON whose field order
//! matches the struct definition — the common case — almost every key dispatch
//! is O(1) without hashing.
//!
//! ## Safe Rust only
//!
//! There are **no `unsafe` blocks** in this crate.  All SIMD scanning uses
//! `std::simd` (nightly) or pure u64/u128 arithmetic (SWAR).
//!
//! # Quick start
//!
//! ```rust,ignore
//! use jzon::{ToJson, FromJson};
//!
//! #[derive(ToJson, FromJson, Debug, PartialEq)]
//! #[serde(rename_all = "camelCase")]
//! struct User<'a> {
//!     user_id:  u64,
//!     name:     &'a str,
//!     #[serde(skip_serializing_if = "Option::is_none")]
//!     email:    Option<String>,
//!     #[serde(default)]
//!     score:    f64,
//! }
//!
//! let input = r#"{"userId":1,"name":"alice","score":9.5}"#;
//! let user: User = User::from_json_str(input).unwrap();
//! let out = user.to_json_string();
//! ```

// Enable `std::simd` portable SIMD on nightly when both features are set.
#![cfg_attr(all(feature = "simd", feature = "unstable"), feature(portable_simd))]

pub mod error;
pub mod scanner;
pub mod ser;
pub mod de;
pub mod simd;
pub mod fixed;
#[cfg(feature = "stats")]
pub mod stats;

pub use error::Error;
pub use scanner::{JsonStr, Scanner};
pub use ser::ToJson;
pub use de::FromJson;
pub use fixed::{FixedBuf, ToJsonExt, json_str_len};

#[cfg(feature = "derive")]
pub use jzon_derive::{FromJson, ToJson};
