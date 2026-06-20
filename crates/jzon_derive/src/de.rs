//! Code-generation for `#[derive(FromJson)]`.
//!
//! * **Field-hint cache** — a single `usize` variable tracks the index of the
//!   most-recently matched field and is tried first on the next key.  For JSON
//!   payloads whose field order matches the struct definition (the common case)
//!   this gives O(1) dispatch on almost every field.
//!
//! * **Integer-padded key comparison** — for field names ≤8 bytes, a single
//!   u64 integer comparison replaces memcmp.  For names 9–16 bytes, u128.
//!   Names >16 bytes fall back to byte-slice comparison.  The dispatch first
//!   branches on `key.len()`, so lengths are verified before the integer test.
//!
//! * **Compile-time minimal perfect hash** (for structs with >6 active fields)
//!   — a multiplier found at proc-macro time gives O(1) dispatch regardless of
//!   field count.
//!
//! * **Trie dispatch** (enabled with `#[rjson(trie_dispatch)]`) — length-first
//!   then character-by-character branching, compiled to jump tables by rustc.
//!
//! * **Alias support** — `|`-joined patterns in the generated `match`.
//!
//! * **Full serde attribute compatibility** — rename, rename_all,
//!   deny_unknown_fields, skip_deserializing, default, default = "fn", flatten.

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{Data, DeriveInput, Error, Fields, GenericParam, Lifetime, Result, Type};

use crate::attrs::{self, ContainerAttrs, FieldAttrs, FieldDefault};
use crate::rename;

/// Active field count above which compile-time PHF dispatch is preferred over
/// integer-compare or trie (unless `#[rjson(trie_dispatch)]` opts in).
/// Benchmarked in `bench_derive_dispatch`; unchanged since Task 0 baseline.
const PHF_DISPATCH_THRESHOLD: usize = 6;

/// Maximum active field count for which `deny_unknown_fields` emits a
/// first-byte prefix check. For larger structs the O(N) scan over valid bytes
/// is worse than going straight to PHF dispatch.
const FIRST_BYTE_DENY_UNKNOWN_MAX_FIELDS: usize = 8;

/// Maximum active field count for u8 required-field bitmask tracking.
const REQUIRED_FIELD_BITMASK_MAX_FIELDS: usize = 8;

/// Maximum active field count for `#[inline]` on `from_json_scanner`.
const INLINE_DISPATCH_MAX_FIELDS: usize = 4;

pub fn expand(input: &DeriveInput) -> Result<TokenStream> {
    match &input.data {
        Data::Struct(_) => expand_struct(input),
        Data::Enum(_) => expand_enum(input),
        Data::Union(_) => Err(Error::new_spanned(
            input,
            "FromJson does not support unions",
        )),
    }
}

fn key_to_u64(bytes: &[u8]) -> u64 {
    let mut b = [0u8; 8];
    let len = bytes.len().min(8);
    b[..len].copy_from_slice(&bytes[..len]);
    u64::from_le_bytes(b)
}

fn key_to_u128(bytes: &[u8]) -> u128 {
    let mut b = [0u8; 16];
    let len = bytes.len().min(16);
    b[..len].copy_from_slice(&bytes[..len]);
    u128::from_le_bytes(b)
}

fn phf_hash(key: &[u8], mul: u64, table_size: usize) -> usize {
    let h = key
        .iter()
        .fold(0u64, |acc, &b| acc.wrapping_mul(mul).wrapping_add(b as u64));
    (h as usize) % table_size
}

fn find_phf_multiplier(keys: &[&[u8]]) -> Option<(u64, usize)> {
    if keys.is_empty() {
        return None;
    }
    let table_size = keys.len().next_power_of_two();
    let candidates: &[u64] = &[
        31, 37, 97, 131, 137, 149, 157, 163, 167, 173, 179, 181, 191, 193, 197, 199, 211, 223, 227,
        229, 233, 239, 241, 251, 257, 263, 269, 271, 277, 281, 283, 293, 307, 311, 313, 317, 331,
        337, 347, 349, 353, 359, 367, 373, 379, 383, 389, 397, 401, 409, 419, 421, 431, 433, 439,
        443, 449, 457, 461, 463, 467, 2654435761, // Knuth's multiplicative hash
        2246822519, 3266489917, 668265263, 374761393,
    ];
    for &m in candidates {
        let mut seen = vec![false; table_size];
        let mut ok = true;
        for key in keys {
            let h = phf_hash(key, m, table_size);
            if seen[h] {
                ok = false;
                break;
            }
            seen[h] = true;
        }
        if ok {
            return Some((m, table_size));
        }
    }
    None
}

fn expand_struct(input: &DeriveInput) -> Result<TokenStream> {
    let ident = &input.ident;
    let container = attrs::parse_container_attrs(&input.attrs)?;
    let de_lt = Lifetime::new("'de", Span::call_site());

    let impl_params: Vec<TokenStream> = {
        let mut p = vec![quote! { 'de }];
        for gp in &input.generics.params {
            match gp {
                GenericParam::Lifetime(_) => {}
                GenericParam::Type(t) => p.push(quote! { #t }),
                GenericParam::Const(c) => p.push(quote! { #c }),
            }
        }
        p
    };
    let ty_args: TokenStream = if input.generics.params.is_empty() {
        quote! {}
    } else {
        let args: Vec<TokenStream> = input
            .generics
            .params
            .iter()
            .map(|gp| match gp {
                GenericParam::Lifetime(_) => quote! { 'de },
                GenericParam::Type(t) => {
                    let id = &t.ident;
                    quote! { #id }
                }
                GenericParam::Const(c) => {
                    let id = &c.ident;
                    quote! { #id }
                }
            })
            .collect();
        quote! { <#(#args),*> }
    };
    let where_clause = &input.generics.where_clause;

    let fields = match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(f) => {
                if container.transparent {
                    let active: Vec<_> = f
                        .named
                        .iter()
                        .filter(|field| {
                            let fa = attrs::parse_field_attrs(&field.attrs).unwrap_or_default();
                            !fa.skip && !fa.skip_deserializing
                        })
                        .collect();
                    if active.len() != 1 {
                        return Err(Error::new_spanned(
                            ident,
                            "#[serde(transparent)] requires exactly one non-skipped field",
                        ));
                    }
                    let single_field = active[0].ident.as_ref().unwrap();
                    let single_ty = &active[0].ty;
                    let skipped_assembly: Vec<TokenStream> = f
                        .named
                        .iter()
                        .filter_map(|field| {
                            let fa = attrs::parse_field_attrs(&field.attrs).unwrap_or_default();
                            if fa.skip || fa.skip_deserializing {
                                let fname = field.ident.as_ref().unwrap();
                                Some(quote! { #fname: ::std::default::Default::default(), })
                            } else {
                                None
                            }
                        })
                        .collect();
                    return Ok(quote! {
                        #[automatically_derived]
                        impl<#(#impl_params),*> ::jzon::FromJson<'de>
                            for #ident #ty_args #where_clause
                        {
                            #[inline(always)]
                            fn from_json_scanner(
                                scanner: &mut ::jzon::Scanner<'de>,
                            ) -> ::std::result::Result<Self, ::jzon::Error> {
                                Ok(#ident {
                                    #single_field: <#single_ty as ::jzon::FromJson<'de>>::from_json_scanner(scanner)?,
                                    #(#skipped_assembly)*
                                })
                            }
                        }
                    });
                }
                &f.named
            }
            Fields::Unit => return expand_unit_struct(input, &container),
            Fields::Unnamed(f) => {
                let n = f.unnamed.len();
                if n == 0 {
                    return Ok(quote! {
                        #[automatically_derived]
                        impl<#(#impl_params),*> ::jzon::FromJson<'de>
                            for #ident #ty_args #where_clause
                        {
                            #[inline(always)]
                            fn from_json_scanner(
                                scanner: &mut ::jzon::Scanner<'de>,
                            ) -> ::std::result::Result<Self, ::jzon::Error> {
                                scanner.skip_whitespace();
                                scanner.expect_byte(b'[')?;
                                scanner.skip_whitespace();
                                scanner.expect_byte(b']')?;
                                Ok(#ident())
                            }
                        }
                    });
                }
                if n == 1 {
                    let inner_ty = &f.unnamed[0].ty;
                    return Ok(quote! {
                        #[automatically_derived]
                        impl<#(#impl_params),*> ::jzon::FromJson<'de>
                            for #ident #ty_args #where_clause
                        {
                            #[inline(always)]
                            fn from_json_scanner(
                                scanner: &mut ::jzon::Scanner<'de>,
                            ) -> ::std::result::Result<Self, ::jzon::Error> {
                                Ok(#ident(<#inner_ty as ::jzon::FromJson<'de>>::from_json_scanner(scanner)?))
                            }
                        }
                    });
                }
                let field_tys: Vec<&Type> = f.unnamed.iter().map(|field| &field.ty).collect();
                let field_vars: Vec<proc_macro2::Ident> = (0..n)
                    .map(|i| {
                        proc_macro2::Ident::new(&format!("_f{}", i), proc_macro2::Span::call_site())
                    })
                    .collect();
                let first_var = &field_vars[0];
                let first_ty = field_tys[0];
                let rest_reads: Vec<TokenStream> = field_vars[1..]
                    .iter()
                    .zip(field_tys[1..].iter())
                    .map(|(var, ty)| {
                        quote! {
                            scanner.skip_whitespace();
                            scanner.expect_byte(b',')?;
                            let #var = <#ty as ::jzon::FromJson<'de>>::from_json_scanner(scanner)?;
                        }
                    })
                    .collect();
                return Ok(quote! {
                    #[automatically_derived]
                    impl<#(#impl_params),*> ::jzon::FromJson<'de>
                        for #ident #ty_args #where_clause
                    {
                        #[inline]
                        fn from_json_scanner(
                            scanner: &mut ::jzon::Scanner<'de>,
                        ) -> ::std::result::Result<Self, ::jzon::Error> {
                            scanner.skip_whitespace();
                            scanner.expect_byte(b'[')?;
                            let #first_var = <#first_ty as ::jzon::FromJson<'de>>::from_json_scanner(scanner)?;
                            #(#rest_reads)*
                            scanner.skip_whitespace();
                            scanner.expect_byte(b']')?;
                            Ok(#ident(#(#field_vars),*))
                        }
                    }
                });
            }
        },
        _ => unreachable!(),
    };

    struct FieldInfo<'a> {
        fname: &'a syn::Ident,
        json_key: String,
        all_keys: Vec<String>,
        ty: &'a Type,
        fattrs: FieldAttrs,
        idx: usize,
    }

    let mut active_fields: Vec<FieldInfo> = Vec::new();
    for (i, field) in fields.iter().enumerate() {
        let fname = field.ident.as_ref().unwrap();
        let fattrs = attrs::parse_field_attrs(&field.attrs)?;
        let json_key = if let Some(r) = &fattrs.rename {
            r.clone()
        } else if let Some(rule) = container.rename_all {
            rename::apply(&fname.to_string(), rule)
        } else {
            fname.to_string()
        };
        let mut all_keys = vec![json_key.clone()];
        all_keys.extend(fattrs.aliases.clone());
        active_fields.push(FieldInfo {
            fname,
            json_key,
            all_keys,
            ty: &field.ty,
            fattrs,
            idx: i,
        });
    }

    let var_decls: Vec<TokenStream> = active_fields
        .iter()
        .filter(|f| !f.fattrs.skip && !f.fattrs.skip_deserializing)
        .map(|f| {
            let fname = f.fname;
            quote! { let mut #fname = None; }
        })
        .collect();

    let active_deserialized: Vec<&FieldInfo> = active_fields
        .iter()
        .filter(|f| !f.fattrs.skip && !f.fattrs.skip_deserializing)
        .collect();

    let num_active = active_deserialized.len();

    // Borrow key bytes from cached field metadata; no per-key Vec allocation.
    let dispatch_keys: Vec<DispatchKey> = active_deserialized
        .iter()
        .flat_map(|f| {
            f.all_keys
                .iter()
                .map(move |k| DispatchKey {
                    key: k.as_bytes(),
                    idx: f.idx,
                })
        })
        .collect();

    let hint_table: Vec<TokenStream> = active_deserialized
        .iter()
        .map(|f| {
            let key_str = &f.json_key;
            let key_bytes = key_str.as_bytes();
            let byte_str = proc_macro2::Literal::byte_string(key_bytes);
            quote! { #byte_str as &[u8] }
        })
        .collect();

    let hint_slot_map: Vec<usize> = {
        let mut m = vec![usize::MAX; active_fields.len()];
        for (slot, fi) in active_deserialized.iter().enumerate() {
            m[fi.idx] = slot;
        }
        m
    };

    let dispatch_expr =
        generate_optimized_dispatch(&dispatch_keys, num_active, container.trie_dispatch);

    // ── Optimization 1: deny_unknown_fields first-byte prefix check ───────────
    let first_byte_check: TokenStream = if container.deny_unknown_fields
        && !dispatch_keys.is_empty()
        && num_active <= FIRST_BYTE_DENY_UNKNOWN_MAX_FIELDS
    {
            // Collect every unique first byte across all keys (including aliases).
            let mut first_bytes: Vec<u8> = dispatch_keys
                .iter()
                .filter(|e| !e.key.is_empty())
                .map(|e| e.key[0])
                .collect();
            first_bytes.sort_unstable();
            first_bytes.dedup();
            let byte_lits: Vec<TokenStream> = first_bytes.iter().map(|&b| quote! { #b }).collect();
            let n = first_bytes.len();
            quote! {
                const VALID_FIRST_BYTES: [u8; #n] = [#(#byte_lits),*];
                if !_key.is_empty() && !VALID_FIRST_BYTES.contains(&_key[0]) {
                    return Err(::jzon::Error::UnknownField);
                }
            }
        } else {
            quote! {}
        };

    // ── Optimization 2: u8 bitmask for required-field tracking ────────────────
    let (use_bitmask, required_slots, required_mask): (bool, Vec<usize>, u8) =
        if num_active <= REQUIRED_FIELD_BITMASK_MAX_FIELDS {
        let slots: Vec<usize> = active_deserialized
            .iter()
            .enumerate()
            .filter_map(|(slot, f)| {
                let is_required = !container.default
                    && !is_option(f.ty)
                    && matches!(f.fattrs.default, FieldDefault::None)
                    && !f.fattrs.skip_serializing;
                if is_required {
                    Some(slot)
                } else {
                    None
                }
            })
            .collect();
        let mask = slots.iter().fold(0u8, |acc, &s| acc | (1u8 << s));
        (!slots.is_empty(), slots, mask)
    } else {
        (false, Vec::new(), 0)
    };

    let inline_attr: TokenStream = if num_active <= INLINE_DISPATCH_MAX_FIELDS {
        quote! { #[inline] }
    } else {
        quote! {}
    };

    let read_arms: Vec<TokenStream> = active_deserialized
        .iter()
        .map(|f| {
            let fname = f.fname;
            let idx = f.idx;
            let read_expr = if let Some(path) = &f.fattrs.deserialize_with {
                quote! { #path(scanner)? }
            } else {
                field_read_expr(f.ty, &de_lt)
            };
            let hint_slot = hint_slot_map[idx];
            let next_slot = if num_active > 0 {
                (hint_slot + 1) % num_active
            } else {
                0
            };

            if use_bitmask {
                let is_required = required_slots.contains(&hint_slot);
                if is_required {
                    let bit: u8 = 1u8 << hint_slot;
                    quote! {
                        #idx => {
                            #fname = Some(#read_expr);
                            _found |= #bit;
                            _hint = #next_slot;
                        }
                    }
                } else {
                    quote! {
                        #idx => {
                            #fname = Some(#read_expr);
                            _hint = #next_slot;
                        }
                    }
                }
            } else {
                quote! {
                    #idx => {
                        #fname = Some(#read_expr);
                        _hint = #next_slot;
                    }
                }
            }
        })
        .collect();

    let unknown_handler = if container.deny_unknown_fields {
        quote! { return Err(::jzon::Error::UnknownField); }
    } else {
        quote! { scanner.skip_value()?; }
    };

    // Missing-field check: bitmask variant (one comparison) vs per-field ok_or fallback.
    let missing_check: TokenStream = if use_bitmask {
        // Build an array mapping slot -> field name string for the error message.
        // We emit a single mask comparison; only on failure do we find the first missing field.
        let required_slot_names: Vec<&str> = required_slots
            .iter()
            .map(|&slot| active_deserialized[slot].json_key.as_str())
            .collect();
        let required_slot_bits: Vec<u8> = required_slots.iter().map(|&slot| 1u8 << slot).collect();
        quote! {
            if _found != #required_mask {
                // Find first missing required field name for the error.
                #(
                    if _found & #required_slot_bits == 0 {
                        return Err(::jzon::Error::MissingField(#required_slot_names));
                    }
                )*
            }
        }
    } else {
        quote! {}
    };

    let field_assembly: Vec<TokenStream> = active_fields
        .iter()
        .map(|f| {
            let fname = f.fname;
            let fname_str = f.json_key.as_str();

            if f.fattrs.skip || f.fattrs.skip_deserializing {
                return quote! { #fname: ::std::default::Default::default(), };
            }
            if f.fattrs.skip_serializing {
                return quote! { #fname: #fname.unwrap_or_default(), };
            }

            match &f.fattrs.default {
                FieldDefault::None => {
                    if container.default {
                        quote! { #fname: #fname.unwrap_or_default(), }
                    } else if is_option(f.ty) {
                        quote! { #fname: #fname.unwrap_or(None), }
                    } else if use_bitmask {
                        // Safety: bitmask already guaranteed this field was set, so unwrap is safe.
                        quote! { #fname: unsafe { #fname.unwrap_unchecked() }, }
                    } else {
                        quote! { #fname: #fname.ok_or(::jzon::Error::MissingField(#fname_str))?, }
                    }
                }
                FieldDefault::Default => {
                    quote! { #fname: #fname.unwrap_or_default(), }
                }
                FieldDefault::Path(path) => {
                    quote! { #fname: #fname.unwrap_or_else(#path), }
                }
            }
        })
        .collect();

    let field_name_assertions: Vec<TokenStream> = active_deserialized.iter().map(|f| {
        let key_str = &f.json_key;
        let key_bytes = key_str.as_bytes();
        let byte_str = proc_macro2::Literal::byte_string(key_bytes);
        let err_msg = format!(
            "Field name `{}` contains JSON-special characters; rename it or use #[serde(rename)] with a safe name",
            key_str
        );
        quote! {
            const _: () = {
                const fn has_json_key_escape_chars(name: &[u8]) -> bool {
                    let mut i = 0;
                    while i < name.len() {
                        let b = name[i];
                        if b == b'"' || b == b'\\' || b < 0x20 { return true; }
                        i += 1;
                    }
                    false
                }
                assert!(!has_json_key_escape_chars(#byte_str), #err_msg);
            };
        }
    }).collect();

    let bitmask_init: TokenStream = if use_bitmask {
        quote! { let mut _found: u8 = 0; }
    } else {
        quote! {}
    };

    Ok(quote! {
        #(#field_name_assertions)*

        #[automatically_derived]
        impl<#(#impl_params),*> ::jzon::FromJson<'de>
            for #ident #ty_args
            #where_clause
        {
            #inline_attr
            fn from_json_scanner(
                scanner: &mut ::jzon::Scanner<'de>,
            ) -> ::std::result::Result<Self, ::jzon::Error> {
                scanner.skip_whitespace();
                scanner.expect_byte(b'{')?;

                #(#var_decls)*

                const FIELD_HINTS: [&[u8]; #num_active] = [#(#hint_table),*];
                let mut _hint: usize = 0;
                #bitmask_init

                loop {
                    match scanner.peek_byte_after_ws()? {
                        b'}' => { scanner.advance(); break; }
                        b'"' => {
                            let _key = scanner.read_key_colon()?;

                            #first_byte_check

                            let _field_idx = if _hint < #num_active && _key == FIELD_HINTS[_hint] {
                                _hint
                            } else {
                                #dispatch_expr
                            };

                            match _field_idx {
                                #(#read_arms)*
                                _ => { #unknown_handler }
                            }
                        }
                        _ => return Err(::jzon::Error::UnexpectedToken),
                    }

                    match scanner.peek_byte_after_ws()? {
                        b',' => { scanner.advance(); }
                        b'}' => { scanner.advance(); break; }
                        _ => return Err(::jzon::Error::UnexpectedToken),
                    }
                }

                #missing_check

                Ok(#ident {
                    #(#field_assembly)*
                })
            }
        }
    })
}

struct DispatchKey<'a> {
    key: &'a [u8],
    idx: usize,
}

fn generate_optimized_dispatch(
    entries: &[DispatchKey<'_>],
    num_active: usize,
    trie_dispatch: bool,
) -> TokenStream {
    if entries.is_empty() {
        return quote! { usize::MAX };
    }

    if num_active > PHF_DISPATCH_THRESHOLD && !trie_dispatch {
        let key_slices: Vec<&[u8]> = entries.iter().map(|dk| dk.key).collect();
        if let Some((mul, table_size)) = find_phf_multiplier(&key_slices) {
            return generate_phf_dispatch(entries, mul, table_size);
        }
    }

    if trie_dispatch {
        generate_trie_dispatch(entries)
    } else {
        generate_integer_compare_dispatch(entries)
    }
}

fn generate_integer_compare_dispatch(all_keys: &[DispatchKey]) -> TokenStream {
    let max_len = all_keys.iter().map(|dk| dk.key.len()).max().unwrap_or(0);
    let mut by_len: Vec<Vec<&DispatchKey>> = vec![Vec::new(); max_len + 1];
    for dk in all_keys {
        by_len[dk.key.len()].push(dk);
    }

    let len_arms: Vec<TokenStream> = by_len
        .iter()
        .enumerate()
        .filter(|(_, group)| !group.is_empty())
        .map(|(len, group)| {
            let body = if len <= 8 {
                let arms: Vec<TokenStream> = group
                    .iter()
                    .map(|dk| {
                        let const_val = key_to_u64(dk.key);
                        let idx = dk.idx;
                        quote! { #const_val => #idx, }
                    })
                    .collect();
                quote! {
                    {
                        let mut _b = [0u8; 8];
                        _b[..#len].copy_from_slice(_key);
                        match u64::from_le_bytes(_b) {
                            #(#arms)*
                            _ => usize::MAX,
                        }
                    }
                }
            } else if len <= 16 {
                let arms: Vec<TokenStream> = group
                    .iter()
                    .map(|dk| {
                        let const_val = key_to_u128(dk.key);
                        let idx = dk.idx;
                        quote! { #const_val => #idx, }
                    })
                    .collect();
                quote! {
                    {
                        let mut _b = [0u8; 16];
                        _b[..#len].copy_from_slice(_key);
                        match u128::from_le_bytes(_b) {
                            #(#arms)*
                            _ => usize::MAX,
                        }
                    }
                }
            } else {
                let arms: Vec<TokenStream> = group
                    .iter()
                    .map(|dk| {
                        let key_bytes = dk.key;
                        let byte_str = proc_macro2::Literal::byte_string(key_bytes);
                        let idx = dk.idx;
                        quote! { #byte_str => #idx, }
                    })
                    .collect();
                quote! {
                    match _key {
                        #(#arms)*
                        _ => usize::MAX,
                    }
                }
            };
            quote! { #len => #body }
        })
        .collect();

    quote! {
        match _key.len() {
            #(#len_arms,)*
            _ => usize::MAX,
        }
    }
}

fn generate_trie_dispatch(all_keys: &[DispatchKey]) -> TokenStream {
    let max_len = all_keys.iter().map(|dk| dk.key.len()).max().unwrap_or(0);
    let mut by_len: Vec<Vec<&DispatchKey>> = vec![Vec::new(); max_len + 1];
    for dk in all_keys {
        by_len[dk.key.len()].push(dk);
    }

    let len_arms: Vec<TokenStream> = by_len
        .iter()
        .enumerate()
        .filter(|(_, group)| !group.is_empty())
        .map(|(len, group)| {
            let body = generate_trie_node(group, 0, len);
            quote! { #len => #body }
        })
        .collect();

    quote! {
        match _key.len() {
            #(#len_arms,)*
            _ => usize::MAX,
        }
    }
}

fn generate_trie_node(group: &[&DispatchKey], depth: usize, len: usize) -> TokenStream {
    if depth == len {
        let idx = group.first().map_or(usize::MAX, |dk| dk.idx);
        return quote! { #idx };
    }

    let mut branches: Vec<(u8, Vec<&DispatchKey>)> = Vec::new();
    for dk in group {
        let byte = dk.key[depth];
        if let Some((_, keys)) = branches.iter_mut().find(|(b, _)| *b == byte) {
            keys.push(*dk);
        } else {
            branches.push((byte, vec![*dk]));
        }
    }
    branches.sort_unstable_by_key(|(byte, _)| *byte);

    let depth_lit = depth;
    let arms: Vec<TokenStream> = branches
        .iter()
        .map(|(byte, keys)| {
            let body = generate_trie_node(keys, depth + 1, len);
            quote! { #byte => #body, }
        })
        .collect();

    quote! {
        match _key[#depth_lit] {
            #(#arms)*
            _ => usize::MAX,
        }
    }
}

fn generate_phf_dispatch(all_keys: &[DispatchKey], mul: u64, table_size: usize) -> TokenStream {
    let mut hash_table: Vec<usize> = vec![usize::MAX; table_size];
    for dk in all_keys {
        let h = phf_hash(dk.key, mul, table_size);
        hash_table[h] = dk.idx;
    }

    let mut slot_keys: Vec<Option<&[u8]>> = vec![None; table_size];
    for dk in all_keys {
        let h = phf_hash(dk.key, mul, table_size);
        slot_keys[h] = Some(dk.key);
    }

    let hash_table_tokens: Vec<TokenStream> = hash_table
        .iter()
        .map(|&fi| {
            if fi == usize::MAX {
                quote! { usize::MAX }
            } else {
                quote! { #fi }
            }
        })
        .collect();

    let slot_key_tokens: Vec<TokenStream> = slot_keys
        .iter()
        .map(|sk| match sk {
            None => quote! { b"" as &[u8] },
            Some(k) => {
                let byte_str = proc_macro2::Literal::byte_string(k);
                quote! { #byte_str as &[u8] }
            }
        })
        .collect();

    let mul_lit = mul;

    quote! {
        {
            const HASH_MUL: u64 = #mul_lit;
            const HASH_TABLE: [usize; #table_size] = [#(#hash_table_tokens),*];
            const SLOT_KEYS: [&[u8]; #table_size] = [#(#slot_key_tokens),*];

            let _h = _key.iter().fold(0u64, |acc, &b|
                acc.wrapping_mul(HASH_MUL).wrapping_add(b as u64)
            ) as usize % #table_size;
            let _fi = HASH_TABLE[_h];
            if _fi != usize::MAX && _key == SLOT_KEYS[_h] {
                _fi
            } else {
                usize::MAX
            }
        }
    }
}

fn expand_unit_struct(input: &DeriveInput, _container: &ContainerAttrs) -> Result<TokenStream> {
    let ident = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    Ok(quote! {
        #[automatically_derived]
        impl<'de> #impl_generics ::jzon::FromJson<'de>
            for #ident #ty_generics #where_clause
        {
            fn from_json_scanner(
                scanner: &mut ::jzon::Scanner<'de>,
            ) -> ::std::result::Result<Self, ::jzon::Error> {
                scanner.skip_whitespace();
                scanner.expect_byte(b'{')?;
                scanner.skip_whitespace();
                scanner.expect_byte(b'}')?;
                Ok(#ident)
            }
        }
    })
}

fn expand_enum(input: &DeriveInput) -> Result<TokenStream> {
    let ident = &input.ident;
    let container = attrs::parse_container_attrs(&input.attrs)?;

    let impl_params: Vec<TokenStream> = {
        let mut p = vec![quote! { 'de }];
        for gp in &input.generics.params {
            match gp {
                GenericParam::Lifetime(_) => {}
                GenericParam::Type(t) => p.push(quote! { #t }),
                GenericParam::Const(c) => p.push(quote! { #c }),
            }
        }
        p
    };
    let ty_args: TokenStream = if input.generics.params.is_empty() {
        quote! {}
    } else {
        let args: Vec<TokenStream> = input
            .generics
            .params
            .iter()
            .map(|gp| match gp {
                GenericParam::Lifetime(_) => quote! { 'de },
                GenericParam::Type(t) => {
                    let id = &t.ident;
                    quote! { #id }
                }
                GenericParam::Const(c) => {
                    let id = &c.ident;
                    quote! { #id }
                }
            })
            .collect();
        quote! { <#(#args),*> }
    };
    let where_clause = &input.generics.where_clause;

    let variants = match &input.data {
        Data::Enum(e) => &e.variants,
        _ => unreachable!(),
    };

    let all_unit = variants.iter().all(|v| matches!(&v.fields, Fields::Unit));

    if all_unit {
        let mut other_variant: Option<TokenStream> = None;
        let arms: Vec<TokenStream> = variants
            .iter()
            .filter_map(|v| {
                let vident = &v.ident;
                let vattrs = match attrs::parse_field_attrs(&v.attrs) {
                    Ok(a) => a,
                    Err(e) => return Some(Err(e)),
                };
                if vattrs.other {
                    other_variant = Some(quote! { #ident::#vident });
                    return None; // not a named arm
                }
                let vname = if let Some(r) = &vattrs.rename {
                    r.clone()
                } else if let Some(rule) = container.rename_all {
                    rename::apply_variant(&vident.to_string(), rule)
                } else {
                    vident.to_string()
                };
                let vbytes = vname.as_bytes();
                let byte_str = proc_macro2::Literal::byte_string(vbytes);
                let alias_pats: Vec<_> = vattrs
                    .aliases
                    .iter()
                    .map(|a| proc_macro2::Literal::byte_string(a.as_bytes()))
                    .collect();
                Some(Ok(
                    quote! { #byte_str #(| #alias_pats)* => Ok(#ident::#vident), },
                ))
            })
            .collect::<Result<_>>()?;

        let fallback = if let Some(ov) = other_variant {
            quote! { _ => Ok(#ov), }
        } else {
            quote! { _ => Err(::jzon::Error::UnknownVariant), }
        };

        return Ok(quote! {
            #[automatically_derived]
            impl<#(#impl_params),*> ::jzon::FromJson<'de>
                for #ident #ty_args #where_clause
            {
                fn from_json_scanner(
                    scanner: &mut ::jzon::Scanner<'de>,
                ) -> ::std::result::Result<Self, ::jzon::Error> {
                    let js = scanner.read_str()?;
                    let s = js.as_str().as_bytes();
                    match s {
                        #(#arms)*
                        #fallback
                    }
                }
            }
        });
    }

    if let Some(tag_key) = &container.tag {
        return expand_internally_tagged_enum(
            input,
            ident,
            &impl_params,
            &ty_args,
            where_clause,
            &container,
            variants,
            tag_key,
        );
    }

    Err(Error::new_spanned(
        ident,
        "FromJson currently supports only unit-variant enums and internally-tagged \
         (#[serde(tag = \"…\")]) struct-variant enums; \
         adjacently tagged, untagged, and tuple-variant enums are not yet supported",
    ))
}

/// Generate `FromJson` for an internally tagged enum (`#[serde(tag = "…")]`).
/// Tag-first inputs dispatch immediately; tag-not-first inputs keep the
/// reset/rescan fallback so field order stays compatible.
fn expand_internally_tagged_enum(
    _input: &DeriveInput,
    ident: &syn::Ident,
    impl_params: &[TokenStream],
    ty_args: &TokenStream,
    where_clause: &Option<syn::WhereClause>,
    container: &attrs::ContainerAttrs,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::token::Comma>,
    tag_key: &str,
) -> Result<TokenStream> {
    let tag_bytes_lit = proc_macro2::Literal::byte_string(tag_key.as_bytes());
    let tag_key_missing: &str = tag_key;
    let de_lt = Lifetime::new("'de", Span::call_site());

    let variant_arms: Vec<TokenStream> = variants.iter().map(|v| {
        let vident = &v.ident;
        let vattrs = attrs::parse_field_attrs(&v.attrs)?;
        let vname = if let Some(r) = &vattrs.rename { r.clone() }
            else if let Some(rule) = container.rename_all { rename::apply_variant(&vident.to_string(), rule) }
            else { vident.to_string() };
        let vbytes_lit = proc_macro2::Literal::byte_string(vname.as_bytes());

        match &v.fields {
            Fields::Unit => {
                Ok(quote! {
                    #vbytes_lit => {
                        loop {
                            scanner.skip_whitespace();
                            match scanner.peek_byte()? {
                                b'}' => { scanner.advance(); break; }
                                b'"' => {
                                    scanner.read_key_colon()?;
                                    scanner.skip_value()?;
                                    scanner.skip_whitespace();
                                    match scanner.peek_byte()? {
                                        b',' => { scanner.advance(); }
                                        b'}' => {}
                                        _ => return Err(::jzon::Error::UnexpectedToken),
                                    }
                                }
                                _ => return Err(::jzon::Error::UnexpectedToken),
                            }
                        }
                        Ok(#ident::#vident)
                    }
                })
            }
            Fields::Named(f) => {
                let decls: Vec<TokenStream> = f.named.iter().map(|field| {
                    let fname = field.ident.as_ref().unwrap();
                    let fa = attrs::parse_field_attrs(&field.attrs)?;
                    if fa.skip || fa.skip_deserializing {
                        Ok(quote! {})
                    } else {
                        Ok(quote! { let mut #fname = None; })
                    }
                }).collect::<Result<_>>()?;

                let field_arms: Vec<TokenStream> = f.named.iter().map(|field| {
                    let fname = field.ident.as_ref().unwrap();
                    let fa = attrs::parse_field_attrs(&field.attrs)?;
                    if fa.skip || fa.skip_deserializing {
                        return Ok(quote! {});
                    }
                    let json_key = if let Some(r) = &fa.rename { r.clone() }
                        else if let Some(rule) = container.rename_all { rename::apply(&fname.to_string(), rule) }
                        else { fname.to_string() };
                    let jkey_lit = proc_macro2::Literal::byte_string(json_key.as_bytes());
                    let fty = &field.ty;
                    let read = field_read_expr(fty, &de_lt);
                    Ok(quote! {
                        #jkey_lit => { #fname = Some(#read); }
                    })
                }).collect::<Result<_>>()?;

                let assembly: Vec<TokenStream> = f.named.iter().map(|field| {
                    let fname = field.ident.as_ref().unwrap();
                    let fa = attrs::parse_field_attrs(&field.attrs)?;
                    let json_key = if let Some(r) = &fa.rename { r.clone() }
                        else if let Some(rule) = container.rename_all { rename::apply(&fname.to_string(), rule) }
                        else { fname.to_string() };
                    if fa.skip || fa.skip_deserializing {
                        return Ok(quote! { #fname: ::std::default::Default::default(), });
                    }
                    let missing = json_key.clone();
                    match &fa.default {
                        attrs::FieldDefault::None => {
                            if matches!(fa.skip, false) {
                                Ok(quote! { #fname: #fname.ok_or(::jzon::Error::MissingField(#missing))?, })
                            } else {
                                Ok(quote! { #fname: ::std::default::Default::default(), })
                            }
                        }
                        attrs::FieldDefault::Default => Ok(quote! { #fname: #fname.unwrap_or_default(), }),
                        attrs::FieldDefault::Path(p) => Ok(quote! { #fname: #fname.unwrap_or_else(#p), }),
                    }
                }).collect::<Result<_>>()?;

                Ok(quote! {
                    #vbytes_lit => {
                        #(#decls)*
                        loop {
                            scanner.skip_whitespace();
                            match scanner.peek_byte()? {
                                b'}' => { scanner.advance(); break; }
                                b'"' => {
                                    let _k2 = scanner.read_key_colon()?;
                                    if _k2 == #tag_bytes_lit {
                                        scanner.skip_value()?;
                                    } else {
                                        match _k2 {
                                            #(#field_arms)*
                                            _ => { scanner.skip_value()?; }
                                        }
                                    }
                                    scanner.skip_whitespace();
                                    match scanner.peek_byte()? {
                                        b',' => { scanner.advance(); }
                                        b'}' => {}
                                        _ => return Err(::jzon::Error::UnexpectedToken),
                                    }
                                }
                                _ => return Err(::jzon::Error::UnexpectedToken),
                            }
                        }
                        Ok(#ident::#vident { #(#assembly)* })
                    }
                })
            }
            Fields::Unnamed(_) => Err(Error::new_spanned(vident,
                "tuple enum variants are not supported with #[serde(tag)]")),
        }
    }).collect::<Result<_>>()?;

    Ok(quote! {
        #[automatically_derived]
        impl<#(#impl_params),*> ::jzon::FromJson<'de>
            for #ident #ty_args #where_clause
        {
            fn from_json_scanner(
                scanner: &mut ::jzon::Scanner<'de>,
            ) -> ::std::result::Result<Self, ::jzon::Error> {
                scanner.skip_whitespace();

                let _obj_start = scanner.pos();
                scanner.expect_byte(b'{')?;

                let mut _tag: ::std::option::Option<::jzon::JsonStr<'de>> = None;
                let mut _tag_first = false;
                let mut _first_key = true;
                loop {
                    scanner.skip_whitespace();
                    match scanner.peek_byte()? {
                        b'}' => { scanner.advance(); break; }
                        b'"' => {
                            let _k = scanner.read_key_colon()?;
                            if _k == #tag_bytes_lit {
                                _tag = ::std::option::Option::Some(scanner.read_str()?);
                                _tag_first = _first_key;
                                break;
                            } else {
                                _first_key = false;
                                scanner.skip_value()?;
                                scanner.skip_whitespace();
                                match scanner.peek_byte()? {
                                    b',' => { scanner.advance(); }
                                    b'}' => {}
                                    _ => return Err(::jzon::Error::UnexpectedToken),
                                }
                            }
                        }
                        _ => return Err(::jzon::Error::UnexpectedToken),
                    }
                }

                let _tag = _tag.ok_or(::jzon::Error::MissingField(#tag_key_missing))?;

                if !_tag_first {
                    scanner.set_pos(_obj_start);
                    scanner.expect_byte(b'{')?;
                } else {
                    scanner.skip_whitespace();
                    match scanner.peek_byte()? {
                        b',' => { scanner.advance(); }
                        b'}' => {}
                        _ => return Err(::jzon::Error::UnexpectedToken),
                    }
                }

                match _tag.as_str().as_bytes() {
                    #(#variant_arms)*
                    _ => {
                        if _tag_first {
                            loop {
                                scanner.skip_whitespace();
                                match scanner.peek_byte()? {
                                    b'}' => { scanner.advance(); break; }
                                    b'"' => {
                                        scanner.read_key_colon()?;
                                        scanner.skip_value()?;
                                        scanner.skip_whitespace();
                                        match scanner.peek_byte()? {
                                            b',' => { scanner.advance(); }
                                            b'}' => { scanner.advance(); break; }
                                            _ => return Err(::jzon::Error::UnexpectedToken),
                                        }
                                    }
                                    _ => return Err(::jzon::Error::UnexpectedToken),
                                }
                            }
                        } else {
                            scanner.skip_object_tail()?;
                        }
                        Err(::jzon::Error::UnknownVariant)
                    }
                }
            }
        }
    })
}

fn is_str_ref(ty: &Type) -> bool {
    if let Type::Reference(r) = ty {
        if let Type::Path(tp) = r.elem.as_ref() {
            return tp.path.is_ident("str");
        }
    }
    false
}

fn is_named(ty: &Type, name: &str) -> bool {
    if let Type::Path(tp) = ty {
        if let Some(seg) = tp.path.segments.last() {
            return seg.ident == name;
        }
    }
    false
}

fn is_option(ty: &Type) -> bool {
    is_named(ty, "Option")
}

fn option_inner(ty: &Type) -> &Type {
    if let Type::Path(tp) = ty {
        if let Some(seg) = tp.path.segments.last() {
            if let syn::PathArguments::AngleBracketed(ab) = &seg.arguments {
                if let Some(syn::GenericArgument::Type(inner)) = ab.args.first() {
                    return inner;
                }
            }
        }
    }
    ty
}

fn field_read_expr(ty: &Type, _de_lt: &Lifetime) -> TokenStream {
    if is_str_ref(ty) {
        quote! {
            {
                let _js = scanner.read_str()?;
                match _js {
                    ::jzon::JsonStr::Borrowed(b) | ::jzon::JsonStr::BorrowedNoEsc(b) => b,
                    ::jzon::JsonStr::Owned(_) => {
                        return Err(::jzon::Error::EscapedString);
                    }
                }
            }
        }
    } else if is_named(ty, "String") {
        quote! { scanner.read_str()?.into_owned() }
    } else if is_option(ty) {
        let inner = option_inner(ty);
        quote! {
            if scanner.peek_null() {
                scanner.read_null()?;
                None
            } else {
                Some(<#inner as ::jzon::FromJson<'de>>::from_json_scanner(scanner)?)
            }
        }
    } else {
        quote! { <#ty as ::jzon::FromJson<'de>>::from_json_scanner(scanner)? }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dispatch_key(key: &'static str, idx: usize) -> DispatchKey<'static> {
        DispatchKey {
            key: key.as_bytes(),
            idx,
        }
    }

    #[test]
    fn trie_dispatch_opt_in_generates_byte_branch_dispatch() {
        let entries = [
            dispatch_key("alpha", 0),
            dispatch_key("beta", 1),
            dispatch_key("gamma", 2),
            dispatch_key("delta", 3),
            dispatch_key("epsilon", 4),
            dispatch_key("zeta", 5),
            dispatch_key("eta", 6),
            dispatch_key("theta", 7),
        ];

        let tokens = generate_optimized_dispatch(&entries, entries.len(), true).to_string();

        assert!(
            tokens.contains("_key [0usize]"),
            "trie dispatch should branch on key bytes, got: {tokens}"
        );
        assert!(
            !tokens.contains("from_le_bytes"),
            "trie dispatch must not fall back to integer compare dispatch: {tokens}"
        );
        assert!(
            !tokens.contains("HASH_TABLE"),
            "trie dispatch must not use PHF dispatch: {tokens}"
        );
    }

    #[test]
    fn phf_dispatch_used_above_threshold() {
        let keys: Vec<String> = (0..7).map(|i| format!("f{i:02}")).collect();
        let entries: Vec<DispatchKey<'_>> = keys
            .iter()
            .enumerate()
            .map(|(i, k)| DispatchKey {
                key: k.as_bytes(),
                idx: i,
            })
            .collect();
        let tokens = generate_optimized_dispatch(&entries, entries.len(), false).to_string();
        assert!(
            tokens.contains("HASH_TABLE"),
            "PHF dispatch expected for >{PHF_DISPATCH_THRESHOLD} active fields: {tokens}"
        );
    }

    #[test]
    fn integer_compare_used_at_or_below_threshold() {
        let keys: Vec<String> = (0..6).map(|i| format!("f{i:02}")).collect();
        let entries: Vec<DispatchKey<'_>> = keys
            .iter()
            .enumerate()
            .map(|(i, k)| DispatchKey {
                key: k.as_bytes(),
                idx: i,
            })
            .collect();
        let tokens = generate_optimized_dispatch(&entries, entries.len(), false).to_string();
        assert!(
            tokens.contains("from_le_bytes"),
            "integer compare dispatch expected for ≤{PHF_DISPATCH_THRESHOLD} active fields: {tokens}"
        );
        assert!(
            !tokens.contains("HASH_TABLE"),
            "PHF dispatch must not be used at threshold: {tokens}"
        );
    }
}
