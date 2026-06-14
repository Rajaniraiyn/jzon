use proc_macro::TokenStream;
use syn::{parse_macro_input, DeriveInput};

mod attrs;
mod rename;
mod ser;
mod de;

/// Derive `jzon::ToJson` for a named struct or unit/tuple-less enum.
///
/// Supported `#[serde(…)]` container attributes:
/// - `rename_all = "camelCase"` | `"snake_case"` | `"PascalCase"` | …
/// - `tag = "type"`, `content = "data"`, `untagged`  (enum tagging)
///
/// Supported `#[serde(…)]` field attributes:
/// - `rename = "json_name"` — override JSON key
/// - `skip` | `skip_serializing` — omit field from output
/// - `skip_serializing_if = "fn"` — omit field when predicate is true
/// - `flatten` — inline inner struct's key-value pairs
///
#[proc_macro_derive(ToJson, attributes(serde, rjson))]
pub fn derive_to_json(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    ser::expand(&input)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Derive `jzon::FromJson<'de>` for a named struct or unit/tuple-less enum.
///
/// Supported `#[serde(…)]` container attributes:
/// - `rename_all = "camelCase"` | …
/// - `deny_unknown_fields` — error on unrecognised JSON keys
/// - `default` — use `Default::default()` for every missing field
/// - `tag = "type"`, `content = "data"`, `untagged`
///
/// Supported `#[serde(…)]` field attributes:
/// - `rename = "json_name"`
/// - `alias = "alt_name"` (repeatable)
/// - `skip` | `skip_deserializing` — do not read from JSON; use `Default`
/// - `default` — use `Default::default()` if field is absent
/// - `default = "fn"` — call the given function if field is absent
/// - `flatten` — read inner struct fields from the surrounding JSON object
/// - `borrow` — for `&str` fields: explicitly request zero-copy borrow
///
/// Supported `#[rjson(…)]` extensions:
/// - `default_value = <expr>` (field, requires `unstable` feature) — inline default
#[proc_macro_derive(FromJson, attributes(serde, rjson))]
pub fn derive_from_json(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    de::expand(&input)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}
