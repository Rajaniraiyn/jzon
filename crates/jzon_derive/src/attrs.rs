//! Attribute parsing for both `#[serde(...)]` and `#[rjson(...)]` namespaces.
//!
//! We mirror every serde container / field attribute relevant to JSON so that
//! structs annotated purely with `#[derive(serde::Serialize, serde::Deserialize)]`
//! and `#[serde(…)]` work with jzon out of the box — users need not add any
//! new annotations.  jzon-specific extensions live under `#[rjson(…)]`.

use syn::{Attribute, Error, ExprPath, LitStr, Result};

// ── RenameAll ─────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RenameAll {
    LowerCase,
    UpperCase,
    PascalCase,
    CamelCase,
    SnakeCase,
    ScreamingSnakeCase,
    KebabCase,
    ScreamingKebabCase,
}

impl RenameAll {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "lowercase"            => Some(Self::LowerCase),
            "UPPERCASE"            => Some(Self::UpperCase),
            "PascalCase"           => Some(Self::PascalCase),
            "camelCase"            => Some(Self::CamelCase),
            "snake_case"           => Some(Self::SnakeCase),
            "SCREAMING_SNAKE_CASE" => Some(Self::ScreamingSnakeCase),
            "kebab-case"           => Some(Self::KebabCase),
            "SCREAMING-KEBAB-CASE" => Some(Self::ScreamingKebabCase),
            _ => None,
        }
    }
}

// ── FieldDefault ──────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
pub enum FieldDefault {
    #[default]
    None,
    /// `#[serde(default)]` — call `Default::default()`
    Default,
    /// `#[serde(default = "path")]` — call the given function
    Path(ExprPath),
}

// ── ContainerAttrs ────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct ContainerAttrs {
    pub rename_all: Option<RenameAll>,
    pub deny_unknown_fields: bool,
    pub default: bool,
    pub tag: Option<String>,
    pub content: Option<String>,
    pub untagged: bool,
    pub transparent: bool,
}

// ── FieldAttrs ────────────────────────────────────────────────────────────────

#[derive(Default, Clone)]
pub struct FieldAttrs {
    pub rename: Option<String>,
    pub aliases: Vec<String>,
    pub skip: bool,
    pub skip_serializing: bool,
    pub skip_deserializing: bool,
    pub skip_serializing_if: Option<ExprPath>,
    pub default: FieldDefault,
    pub flatten: bool,
    /// `#[serde(other)]` on an enum variant — catch-all for unknown variants
    pub other: bool,
    /// `#[rjson(serialize_with = "path")]` — fn(&T, &mut Vec<u8>)
    pub serialize_with: Option<ExprPath>,
    /// `#[rjson(deserialize_with = "path")]` — fn(&mut Scanner<'de>) -> Result<T, Error>
    pub deserialize_with: Option<ExprPath>,
}

// ── parsing ───────────────────────────────────────────────────────────────────

fn is_serde_or_rjson(attr: &Attribute) -> Option<bool> {
    if attr.path().is_ident("serde")  { return Some(false); }
    if attr.path().is_ident("rjson")  { return Some(true);  }
    None
}

pub fn parse_container_attrs(attrs: &[Attribute]) -> Result<ContainerAttrs> {
    let mut out = ContainerAttrs::default();
    for attr in attrs {
        let Some(is_rjson) = is_serde_or_rjson(attr) else { continue };
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename_all") {
                let s: LitStr = meta.value()?.parse()?;
                out.rename_all = RenameAll::from_str(&s.value());
            } else if meta.path.is_ident("deny_unknown_fields") {
                out.deny_unknown_fields = true;
            } else if meta.path.is_ident("default") {
                out.default = true;
            } else if meta.path.is_ident("tag") {
                let s: LitStr = meta.value()?.parse()?;
                out.tag = Some(s.value());
            } else if meta.path.is_ident("content") {
                let s: LitStr = meta.value()?.parse()?;
                out.content = Some(s.value());
            } else if meta.path.is_ident("untagged") {
                out.untagged = true;
            } else if meta.path.is_ident("transparent") {
                out.transparent = true;
            } else if meta.path.is_ident("rename_all_fields") {
                // Serde 1.0.152+ alias for rename_all on enum variant fields.
                // We apply the same rule as rename_all.
                let s: LitStr = meta.value()?.parse()?;
                out.rename_all = RenameAll::from_str(&s.value());
            } else if matches!(meta.path.get_ident().map(|i| i.to_string()).as_deref(),
                Some("bound" | "crate" | "remote" | "from" | "try_from" | "into"
                   | "expecting" | "variant_identifier" | "field_identifier")) {
                // Serde-internal attrs that don't map to jzon codegen. Consume
                // any value token so syn's parser doesn't choke.
                if meta.input.peek(syn::Token![=]) { let _: LitStr = meta.value()?.parse()?; }
            } else if matches!(meta.path.get_ident().map(|i| i.to_string()).as_deref(),
                Some("serialize_with" | "deserialize_with" | "with")) {
                // Container-level with/serialize_with/deserialize_with are not valid serde attrs
                // at the container level either; consume value and ignore silently.
                if meta.input.peek(syn::Token![=]) { let _: LitStr = meta.value()?.parse()?; }
            } else if is_rjson {
                // #[serde(...)] unknowns are silently ignored — serde owns that
                // namespace and will validate them. #[rjson(...)] unknowns are
                // a typo or unsupported feature in jzon's own namespace: fail loudly.
                return Err(meta.error(format!(
                    "unknown rjson container attribute `{}`",
                    meta.path.get_ident().map_or_else(|| "?".into(), |i| i.to_string())
                )));
            }
            Ok(())
        })?;
    }
    Ok(out)
}

pub fn parse_field_attrs(attrs: &[Attribute]) -> Result<FieldAttrs> {
    let mut out = FieldAttrs::default();
    for attr in attrs {
        let Some(is_rjson) = is_serde_or_rjson(attr) else { continue };
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename") {
                let s: LitStr = meta.value()?.parse()?;
                out.rename = Some(s.value());
            } else if meta.path.is_ident("alias") {
                let s: LitStr = meta.value()?.parse()?;
                out.aliases.push(s.value());
            } else if meta.path.is_ident("skip") {
                out.skip = true;
            } else if meta.path.is_ident("skip_serializing") {
                out.skip_serializing = true;
            } else if meta.path.is_ident("skip_deserializing") {
                out.skip_deserializing = true;
            } else if meta.path.is_ident("skip_serializing_if") {
                let s: LitStr = meta.value()?.parse()?;
                let path: ExprPath = s.parse()?;
                out.skip_serializing_if = Some(path);
            } else if meta.path.is_ident("default") {
                if meta.input.peek(syn::Token![=]) {
                    let s: LitStr = meta.value()?.parse()?;
                    let path: ExprPath = s.parse()?;
                    out.default = FieldDefault::Path(path);
                } else {
                    out.default = FieldDefault::Default;
                }
            } else if meta.path.is_ident("flatten") {
                out.flatten = true;
            } else if meta.path.is_ident("other") {
                out.other = true;
            } else if meta.path.is_ident("borrow") {
                // jzon zero-copies &'de str natively; this attr is a no-op for us.
                if meta.input.peek(syn::Token![=]) { let _: LitStr = meta.value()?.parse()?; }
            } else if matches!(meta.path.get_ident().map(|i| i.to_string()).as_deref(),
                Some("bound" | "getter")) {
                if meta.input.peek(syn::Token![=]) { let _: LitStr = meta.value()?.parse()?; }
            } else if meta.path.is_ident("serialize_with") && is_rjson {
                let s: LitStr = meta.value()?.parse()?;
                let path: ExprPath = s.parse()?;
                out.serialize_with = Some(path);
            } else if meta.path.is_ident("deserialize_with") && is_rjson {
                let s: LitStr = meta.value()?.parse()?;
                let path: ExprPath = s.parse()?;
                out.deserialize_with = Some(path);
            } else if matches!(meta.path.get_ident().map(|i| i.to_string()).as_deref(),
                Some("serialize_with" | "deserialize_with" | "with")) {
                // This is the serde namespace — these can't bridge to jzon's Vec<u8>/Scanner API.
                return Err(Error::new_spanned(
                    meta.path,
                    "jzon does not support #[serde(serialize_with/deserialize_with/with)] — \
                     use jzon_serde (Mode B) or jzon_compat (Mode C) for serde-compatible custom \
                     ser/de functions; for a jzon-native escape hatch, use \
                     #[rjson(serialize_with = \"path\")] where path: fn(&T, &mut Vec<u8>) \
                     or #[rjson(deserialize_with = \"path\")] where path: \
                     fn(&mut jzon::Scanner) -> Result<T, jzon::Error>",
                ));
            } else if is_rjson {
                return Err(meta.error(format!(
                    "unknown rjson field attribute `{}`",
                    meta.path.get_ident().map_or_else(|| "?".into(), |i| i.to_string())
                )));
            }
            Ok(())
        })?;
    }
    Ok(out)
}
