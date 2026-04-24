use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use syn::parse::Parse;
use syn::{ItemTrait, LitBool, LitStr, Token, TraitItem, TraitItemFn};

pub struct ProtocolArgs {
    pub name: String,
    pub via_metadata: bool,
    /// Phase 3 opt-in: when true, the macro binds the ProtocolFn at the
    /// method name (e.g. "count") instead of the ProtocolMethod. The
    /// ProtocolMethod is still constructed (dropped on the floor — no one
    /// references it after binding); Phase 4 will remove it entirely.
    /// Defaults false to preserve Phase 2 behavior.
    pub emit_fn_primary: bool,
}

impl Parse for ProtocolArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut name: Option<String> = None;
        let mut via_metadata: bool = false;
        let mut emit_fn_primary: bool = false;
        let punct: syn::punctuated::Punctuated<syn::MetaNameValue, Token![,]> =
            input.parse_terminated(syn::MetaNameValue::parse, Token![,])?;
        for nv in punct {
            let key = nv
                .path
                .get_ident()
                .map(|i| i.to_string())
                .unwrap_or_default();
            match key.as_str() {
                "name" => {
                    let s: LitStr = syn::parse2(nv.value.to_token_stream())?;
                    name = Some(s.value());
                }
                "extend_via_metadata" => {
                    let b: LitBool = syn::parse2(nv.value.to_token_stream())?;
                    via_metadata = b.value;
                }
                "emit_fn_primary" => {
                    let b: LitBool = syn::parse2(nv.value.to_token_stream())?;
                    emit_fn_primary = b.value;
                }
                other => {
                    return Err(syn::Error::new_spanned(
                        nv,
                        format!("unknown protocol arg: {other}"),
                    ));
                }
            }
        }
        let name = name.ok_or_else(|| {
            syn::Error::new(input.span(), "protocol requires name = \"...\"")
        })?;
        Ok(Self { name, via_metadata, emit_fn_primary })
    }
}

pub struct MethodInfo {
    pub ident: syn::Ident,
    /// `None` = variadic (trait method named `invoke_variadic`).
    /// `Some(n)` = fixed arity `invoke{n}`.
    pub arity: Option<usize>,
}

pub fn method_infos(item: &ItemTrait) -> Vec<MethodInfo> {
    item.items
        .iter()
        .filter_map(|ti| {
            if let TraitItem::Fn(TraitItemFn { sig, .. }) = ti {
                let name = sig.ident.to_string();
                let arity = if name == "invoke_variadic" {
                    None
                } else if let Some(rest) = name.strip_prefix("invoke") {
                    rest.parse::<usize>().ok()
                } else {
                    // Method doesn't match invoke{N} or invoke_variadic — still a trait
                    // method but with no protocol-dispatch arity. We still surface it so
                    // codegen (Task 15) can decide what to do. For non-IFn-shaped
                    // protocols, arity doesn't apply; Task 15 uses the ident directly.
                    None
                };
                Some(MethodInfo {
                    ident: sig.ident.clone(),
                    arity,
                })
            } else {
                None
            }
        })
        .collect()
}

pub fn expand(args: ProtocolArgs, item: ItemTrait) -> TokenStream {
    let trait_ident = &item.ident;
    let methods = method_infos(&item);

    let (ns_lit, name_part_lit): (Option<String>, String) = match args.name.split_once('/') {
        Some((n, m)) => (Some(n.to_string()), m.to_string()),
        None => (None, args.name.clone()),
    };

    let ns_expr: proc_macro2::TokenStream = match ns_lit {
        Some(n) => quote! { Some(::std::sync::Arc::from(#n)) },
        None => quote! { None },
    };

    let via_md = args.via_metadata;
    let emit_fn_primary = args.emit_fn_primary;

    let method_key_strings: Vec<String> = methods.iter().map(|m| m.ident.to_string()).collect();

    let register_fn_ident = quote::format_ident!("__register_proto_{}", trait_ident);

    // Per-method ProtocolMethod bindings emitted as a repetition. When
    // `emit_fn_primary` is true, the PM is still constructed (Protocol's
    // cache still needs impls for satisfies? and fall-through dispatch)
    // but NOT exposed at the module-level method name — the ProtocolFn
    // takes that slot instead.
    let method_bindings: Vec<proc_macro2::TokenStream> = method_key_strings.iter().map(|mname| {
        if emit_fn_primary {
            quote! {
                {
                    // No module-level binding for the PM — the ProtocolFn
                    // binding below takes the `mname` slot. PM is dropped.
                }
            }
        } else {
            quote! {
                {
                    let pm = crate::ProtocolMethod {
                        protocol: proto_py.clone_ref(py),
                        key: ::std::sync::Arc::from(#mname),
                    };
                    let pm_py = ::pyo3::Py::new(py, pm)?;
                    m.add(#mname, pm_py)?;
                }
            }
        }
    }).collect();

    // Per-method ProtocolFn creation + registry insertion. Primary module
    // binding depends on `emit_fn_primary`:
    //   - false (Phase 2 default): ProtocolFn exposed at "_pfn:<name>" only;
    //     the method name stays bound to ProtocolMethod.
    //   - true (Phase 3 opt-in): ProtocolFn exposed at the method name.
    //     No _pfn: alias emitted.
    let proto_name_str = trait_ident.to_string();
    let protocol_fn_bindings: Vec<proc_macro2::TokenStream> =
        method_key_strings.iter().map(|mname| {
            let expose_at_primary = if emit_fn_primary {
                quote! { m.add(#mname, pfn_py)?; }
            } else {
                quote! {
                    let hidden_name = ::std::format!("_pfn:{}", #mname);
                    m.add(hidden_name.as_str(), pfn_py)?;
                }
            };
            quote! {
                {
                    let pfn = crate::protocol_fn::ProtocolFn::new_py(
                        ::std::string::String::from(#mname),
                        ::std::string::String::from(#proto_name_str),
                        #via_md,
                    );
                    let pfn_py = ::pyo3::Py::new(py, pfn)?;
                    crate::protocol_fn::register_protocol_fn(
                        #proto_name_str,
                        #mname,
                        pfn_py.clone_ref(py),
                    );
                    #expose_at_primary
                }
            }
        }).collect();

    let method_key_push = method_key_strings.iter().map(|mname| {
        quote! { v.push(::std::sync::Arc::from(#mname)); }
    });

    quote! {
        #item

        #[allow(non_snake_case)]
        fn #register_fn_ident(
            py: ::pyo3::Python<'_>,
            m: &::pyo3::Bound<'_, ::pyo3::types::PyModule>,
        ) -> ::pyo3::PyResult<()> {
            use ::pyo3::prelude::*;
            let sym = crate::Symbol::new(
                #ns_expr,
                ::std::sync::Arc::from(#name_part_lit),
            );
            let sym_py = ::pyo3::Py::new(py, sym)?;
            let proto = crate::Protocol {
                name: sym_py,
                method_keys: {
                    let mut v: ::smallvec::SmallVec<[::std::sync::Arc<str>; 8]> =
                        ::smallvec::SmallVec::new();
                    #(#method_key_push)*
                    v
                },
                cache: ::std::sync::Arc::new(crate::MethodCache::new()),
                fallback: ::parking_lot::RwLock::new(None),
                via_metadata: #via_md,
            };
            let proto_py = ::pyo3::Py::new(py, proto)?;
            m.add(stringify!(#trait_ident), proto_py.clone_ref(py))?;
            // Also register in the global Protocol registry so ProtocolFn
            // dispatch can fall through on miss — see protocol_fn.rs.
            crate::protocol_fn::register_old_protocol(
                #proto_name_str,
                proto_py.clone_ref(py),
            );

            // Old path: per-method ProtocolMethod bindings.
            #(#method_bindings)*

            // New path: per-method ProtocolFn creation + registry insertion.
            // Exposed under "_pfn:<name>" during Phase 2; Phase 3 flips the
            // primary #(#method_bindings)* names to ProtocolFn.
            #(#protocol_fn_bindings)*

            Ok(())
        }

        ::inventory::submit! {
            crate::registry::ProtocolRegistration {
                build_and_register: #register_fn_ident,
            }
        }
    }
}
