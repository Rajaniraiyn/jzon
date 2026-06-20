//! Code-generation for `#[derive(ToJson)]`.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Error, Fields, Result};

use crate::attrs::{self, FieldAttrs};
use crate::rename;

pub fn expand(input: &DeriveInput) -> Result<TokenStream> {
    match &input.data {
        Data::Struct(_) => expand_struct(input),
        Data::Enum(_) => expand_enum(input),
        Data::Union(_) => Err(Error::new_spanned(input, "ToJson does not support unions")),
    }
}

fn expand_struct(input: &DeriveInput) -> Result<TokenStream> {
    let ident = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let container = attrs::parse_container_attrs(&input.attrs)?;

    let fields = match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(f) => {
                if container.transparent {
                    let active: Vec<_> = f.named.iter().filter(|field| {
                        let fa = attrs::parse_field_attrs(&field.attrs).unwrap_or_default();
                        !fa.skip && !fa.skip_serializing
                    }).collect();
                    if active.len() != 1 {
                        return Err(Error::new_spanned(
                            ident,
                            "#[serde(transparent)] requires exactly one non-skipped field",
                        ));
                    }
                    let single = active[0].ident.as_ref().unwrap();
                    return Ok(quote! {
                        #[automatically_derived]
                        impl #impl_generics ::jzon::ToJson for #ident #ty_generics #where_clause {
                            #[inline(always)]
                            fn json_write(&self, w: &mut ::std::vec::Vec<u8>) {
                                ::jzon::ToJson::json_write(&self.#single, w);
                            }
                            #[inline(always)]
                            fn json_write_sink<S: ::jzon::JsonSink>(&self, w: &mut S) {
                                ::jzon::ToJson::json_write_sink(&self.#single, w);
                            }
                            #[inline(always)]
                            fn json_size_hint(&self) -> usize {
                                ::jzon::ToJson::json_size_hint(&self.#single)
                            }
                        }
                    });
                }
                &f.named
            }
            Fields::Unit => {
                return Ok(quote! {
                    #[automatically_derived]
                    impl #impl_generics ::jzon::ToJson for #ident #ty_generics #where_clause {
                        #[inline(always)]
                        fn json_write(&self, w: &mut ::std::vec::Vec<u8>) {
                            self.json_write_sink(&mut ::jzon::VecSink(w));
                        }
                        #[inline(always)]
                        fn json_write_sink<S: ::jzon::JsonSink>(&self, w: &mut S) {
                            w.extend(b"{}");
                        }
                        #[inline(always)]
                        fn json_size_hint(&self) -> usize { 2 }
                    }
                });
            }
            Fields::Unnamed(f) => {
                let n = f.unnamed.len();
                if n == 0 {
                    return Ok(quote! {
                        #[automatically_derived]
                        impl #impl_generics ::jzon::ToJson for #ident #ty_generics #where_clause {
                            #[inline(always)]
                            fn json_write(&self, w: &mut ::std::vec::Vec<u8>) {
                                self.json_write_sink(&mut ::jzon::VecSink(w));
                            }
                            #[inline(always)]
                            fn json_write_sink<S: ::jzon::JsonSink>(&self, w: &mut S) {
                                w.extend(b"[]");
                            }
                            #[inline(always)]
                            fn json_size_hint(&self) -> usize { 2 }
                        }
                    });
                }
                if n == 1 {
                    return Ok(quote! {
                        #[automatically_derived]
                        impl #impl_generics ::jzon::ToJson for #ident #ty_generics #where_clause {
                            #[inline(always)]
                            fn json_write(&self, w: &mut ::std::vec::Vec<u8>) {
                                ::jzon::ToJson::json_write(&self.0, w);
                            }
                            #[inline(always)]
                            fn json_write_sink<S: ::jzon::JsonSink>(&self, w: &mut S) {
                                ::jzon::ToJson::json_write_sink(&self.0, w);
                            }
                            #[inline(always)]
                            fn json_size_hint(&self) -> usize {
                                ::jzon::ToJson::json_size_hint(&self.0)
                            }
                        }
                    });
                }
                let indices: Vec<syn::Index> = (0..n).map(syn::Index::from).collect();
                let first_idx = &indices[0];
                let rest_writes: Vec<TokenStream> = indices[1..].iter().map(|i| {
                    quote! {
                        w.push(b',');
                        ::jzon::ToJson::json_write_sink(&self.#i, w);
                    }
                }).collect();
                let hint_parts: Vec<TokenStream> = indices.iter().map(|i| {
                    quote! { ::jzon::ToJson::json_size_hint(&self.#i) }
                }).collect();
                return Ok(quote! {
                    #[automatically_derived]
                    impl #impl_generics ::jzon::ToJson for #ident #ty_generics #where_clause {
                        #[inline]
                        fn json_write(&self, w: &mut ::std::vec::Vec<u8>) {
                            self.json_write_sink(&mut ::jzon::VecSink(w));
                        }
                        #[inline]
                        fn json_write_sink<S: ::jzon::JsonSink>(&self, w: &mut S) {
                            w.push(b'[');
                            ::jzon::ToJson::json_write_sink(&self.#first_idx, w);
                            #(#rest_writes)*
                            w.push(b']');
                        }
                        #[inline]
                        fn json_size_hint(&self) -> usize {
                            2usize + #(#hint_parts)+* + (#n - 1)
                        }
                    }
                });
            }
        },
        _ => unreachable!(),
    };

    // Reasonable default when `serialize_with` hides the field type from codegen.
    const SERIALIZE_WITH_HINT: usize = 16;

    struct FieldInfo<'a> {
        fname: &'a syn::Ident,
        json_key: String,
        write_value: TokenStream,
        predicate: Option<syn::ExprPath>,
        serialize_with: bool,
    }

    let mut field_infos: Vec<FieldInfo<'_>> = Vec::new();

    for field in fields {
        let fname = field.ident.as_ref().unwrap();
        let fattrs: FieldAttrs = attrs::parse_field_attrs(&field.attrs)?;

        if fattrs.skip || fattrs.skip_serializing {
            continue;
        }

        if fattrs.flatten {
            return Err(Error::new_spanned(
                &field.ident,
                "#[serde(flatten)] is not yet supported by jzon ToJson; use jzon_serde (Mode B)",
            ));
        }

        let json_key = if let Some(r) = &fattrs.rename {
            r.clone()
        } else if let Some(rule) = container.rename_all {
            rename::apply(&fname.to_string(), rule)
        } else {
            fname.to_string()
        };

        let write_value: TokenStream = if let Some(path) = &fattrs.serialize_with {
            quote! {
                {
                    let mut __tmp = ::std::vec::Vec::new();
                    #path(&self.#fname, &mut __tmp);
                    w.extend(&__tmp);
                }
            }
        } else {
            quote! { ::jzon::ToJson::json_write_sink(&self.#fname, w); }
        };

        field_infos.push(FieldInfo {
            fname,
            json_key,
            write_value,
            predicate: fattrs.skip_serializing_if,
            serialize_with: fattrs.serialize_with.is_some(),
        });
    }

    let serializable_field_count = field_infos.len();
    let has_conditional = field_infos.iter().any(|f| f.predicate.is_some());

    let mut writes: Vec<TokenStream> = Vec::new();
    let open_brace;
    let size_hint_impl;

    if has_conditional {
        open_brace = quote! {
            let mut first = true;
            w.push(b'{');
        };

        let mut hint_parts: Vec<TokenStream> = Vec::new();

        for (idx, fi) in field_infos.iter().enumerate() {
            let fname = fi.fname;
            let key_literal = format!("\"{}\":", fi.json_key);
            let key_lit_bytes = proc_macro2::Literal::byte_string(key_literal.as_bytes());
            let const_name = proc_macro2::Ident::new(
                &format!("_K{}", idx),
                proc_macro2::Span::call_site(),
            );
            let key_overhead = fi.json_key.len() + 3;
            let write_value = &fi.write_value;
            let value_hint: TokenStream = if fi.serialize_with {
                quote! { #SERIALIZE_WITH_HINT }
            } else {
                quote! { ::jzon::ToJson::json_size_hint(&self.#fname) }
            };

            let emit_field = quote! {
                if !first {
                    w.push(b',');
                } else {
                    first = false;
                }
                const #const_name: &[u8] = #key_lit_bytes;
                w.extend(#const_name);
                #write_value
            };

            if let Some(predicate) = &fi.predicate {
                writes.push(quote! {
                    if !#predicate(&self.#fname) {
                        #emit_field
                    }
                });
                hint_parts.push(quote! {
                    if !#predicate(&self.#fname) {
                        hint += count + #key_overhead + #value_hint;
                        count += 1;
                    }
                });
            } else {
                writes.push(emit_field);
                hint_parts.push(quote! {
                    hint += count + #key_overhead + #value_hint;
                    count += 1;
                });
            }
        }

        size_hint_impl = quote! {
            #[inline]
            fn json_size_hint(&self) -> usize {
                let mut hint = 2usize;
                let mut count = 0usize;
                #(#hint_parts)*
                hint
            }
        };
    } else {
        let mut first = true;
        let mut open_brace_fused = false;
        let mut compile_time_key_overhead: usize = 2; // `{` + `}`
        let mut runtime_hint_parts: Vec<TokenStream> = Vec::new();
        let mut always_present_count = 0usize;

        for (idx, fi) in field_infos.iter().enumerate() {
            let fname = fi.fname;
            let json_key = &fi.json_key;
            let write_value = &fi.write_value;
            let const_name = proc_macro2::Ident::new(
                &format!("_K{}", idx),
                proc_macro2::Span::call_site(),
            );
            let value_hint: TokenStream = if fi.serialize_with {
                quote! { #SERIALIZE_WITH_HINT }
            } else {
                quote! { ::jzon::ToJson::json_size_hint(&self.#fname) }
            };

            if first {
                first = false;
                let fused_key = format!("{{\"{}\":", json_key);
                let fused_lit = proc_macro2::Literal::byte_string(fused_key.as_bytes());
                writes.push(quote! {
                    {
                        const #const_name: &[u8] = #fused_lit;
                        w.extend(#const_name);
                        #write_value
                    }
                });
                open_brace_fused = true;
                compile_time_key_overhead += json_key.len() + 3;
                always_present_count += 1;
                runtime_hint_parts.push(value_hint);
            } else {
                let fused_key = format!(",\"{}\":", json_key);
                let fused_lit = proc_macro2::Literal::byte_string(fused_key.as_bytes());
                writes.push(quote! {
                    {
                        const #const_name: &[u8] = #fused_lit;
                        w.extend(#const_name);
                        #write_value
                    }
                });
                compile_time_key_overhead += json_key.len() + 4;
                always_present_count += 1;
                runtime_hint_parts.push(value_hint);
            }
        }

        open_brace = if open_brace_fused {
            quote! {}
        } else {
            quote! { w.push(b'{'); }
        };

        size_hint_impl = if always_present_count == 0 {
            quote! {
                #[inline]
                fn json_size_hint(&self) -> usize { 2 }
            }
        } else {
            quote! {
                #[inline]
                fn json_size_hint(&self) -> usize {
                    const KEY_OVERHEAD: usize = #compile_time_key_overhead;
                    KEY_OVERHEAD #(+ #runtime_hint_parts)*
                }
            }
        };
    }

    let inline_attr = match serializable_field_count {
        0..=4  => quote! { #[inline(always)] },
        5..=16 => quote! { #[inline] },
        _      => quote! {},
    };

    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics ::jzon::ToJson for #ident #ty_generics #where_clause {
            #inline_attr
            fn json_write(&self, w: &mut ::std::vec::Vec<u8>) {
                self.json_write_sink(&mut ::jzon::VecSink(w));
            }

            #inline_attr
            fn json_write_sink<S: ::jzon::JsonSink>(&self, w: &mut S) {
                #open_brace
                #(#writes)*
                w.push(b'}');
            }

            #size_hint_impl
        }
    })
}

fn expand_enum(input: &DeriveInput) -> Result<TokenStream> {
    let ident = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let container = attrs::parse_container_attrs(&input.attrs)?;

    let variants = match &input.data {
        Data::Enum(e) => &e.variants,
        _ => unreachable!(),
    };

    let tag = container.tag.as_deref();
    let content = container.content.as_deref();
    let untagged = container.untagged;

    let arms: Vec<TokenStream> = variants
        .iter()
        .map(|v| {
            let vident = &v.ident;
            let vattrs = attrs::parse_field_attrs(&v.attrs)?;

            let variant_name = if let Some(r) = &vattrs.rename {
                r.clone()
            } else if let Some(rule) = container.rename_all {
                rename::apply_variant(&vident.to_string(), rule)
            } else {
                vident.to_string()
            };

            match &v.fields {
                Fields::Unit => {
                    let quoted_name = format!("\"{}\"", variant_name);
                    let vname_lit = proc_macro2::Literal::byte_string(quoted_name.as_bytes());
                    Ok(quote! {
                        Self::#vident => w.extend(#vname_lit),
                    })
                }
                Fields::Named(f) => {
                    let field_writes = build_variant_field_writes(
                        f.named.iter(),
                        &container,
                        true,
                    )?;
                    let arm = if let Some(tag_key) = tag {
                        if let Some(content_key) = content {
                            let tag_payload = format!("{{\"{}\":\"{}\",\"{}\":", tag_key, variant_name, content_key);
                            let tag_lit = proc_macro2::Literal::byte_string(tag_payload.as_bytes());
                            let field_names: Vec<&syn::Ident> = f.named.iter().map(|f| f.ident.as_ref().unwrap()).collect();
                            quote! {
                                Self::#vident { #(#field_names),* } => {
                                    w.extend(#tag_lit);
                                    w.push(b'{');
                                    #(#field_writes)*
                                    w.extend(b"}}");
                                }
                            }
                        } else {
                            let tag_payload = format!("{{\"{}\":\"{}\",", tag_key, variant_name);
                            let tag_lit = proc_macro2::Literal::byte_string(tag_payload.as_bytes());
                            let field_names: Vec<&syn::Ident> = f.named.iter().map(|f| f.ident.as_ref().unwrap()).collect();
                            quote! {
                                Self::#vident { #(#field_names),* } => {
                                    w.extend(#tag_lit);
                                    #(#field_writes)*
                                    w.push(b'}');
                                }
                            }
                        }
                    } else if untagged {
                        let field_names: Vec<&syn::Ident> = f.named.iter().map(|f| f.ident.as_ref().unwrap()).collect();
                        quote! {
                            Self::#vident { #(#field_names),* } => {
                                w.push(b'{');
                                #(#field_writes)*
                                w.push(b'}');
                            }
                        }
                    } else {
                        let tag_payload = format!("{{\"{}\":{{", variant_name);
                        let tag_lit = proc_macro2::Literal::byte_string(tag_payload.as_bytes());
                        let field_names: Vec<&syn::Ident> = f.named.iter().map(|f| f.ident.as_ref().unwrap()).collect();
                        quote! {
                            Self::#vident { #(#field_names),* } => {
                                w.extend(#tag_lit);
                                #(#field_writes)*
                                w.extend(b"}}");
                            }
                        }
                    };
                    Ok(arm)
                }
                Fields::Unnamed(_) => Err(Error::new_spanned(
                    vident,
                    "ToJson does not support tuple enum variants",
                )),
            }
        })
        .collect::<Result<_>>()?;

    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics ::jzon::ToJson for #ident #ty_generics #where_clause {
            fn json_write(&self, w: &mut ::std::vec::Vec<u8>) {
                self.json_write_sink(&mut ::jzon::VecSink(w));
            }

            fn json_write_sink<S: ::jzon::JsonSink>(&self, w: &mut S) {
                match self {
                    #(#arms)*
                }
            }
        }
    })
}

fn build_variant_field_writes<'a>(
    fields: impl Iterator<Item = &'a syn::Field>,
    container: &attrs::ContainerAttrs,
    _is_named: bool,
) -> Result<Vec<TokenStream>> {
    let mut writes = Vec::new();
    let mut first = true;
    let mut field_idx = 0usize;
    for field in fields {
        let fname = field.ident.as_ref().unwrap();
        let fattrs = attrs::parse_field_attrs(&field.attrs)?;
        if fattrs.skip || fattrs.skip_serializing { continue; }

        let json_key = if let Some(r) = &fattrs.rename {
            r.clone()
        } else if let Some(rule) = container.rename_all {
            rename::apply(&fname.to_string(), rule)
        } else {
            fname.to_string()
        };

        let fused_key = if first {
            first = false;
            format!("\"{}\":", json_key)
        } else {
            format!(",\"{}\":", json_key)
        };
        let fused_lit = proc_macro2::Literal::byte_string(fused_key.as_bytes());
        let const_name = proc_macro2::Ident::new(
            &format!("_VK{}", field_idx),
            proc_macro2::Span::call_site(),
        );
        field_idx += 1;

        writes.push(quote! {
            {
                const #const_name: &[u8] = #fused_lit;
                w.extend(#const_name);
                ::jzon::ToJson::json_write_sink(#fname, w);
            }
        });
    }
    Ok(writes)
}
