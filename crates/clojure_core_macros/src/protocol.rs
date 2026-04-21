use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use syn::parse::Parse;
use syn::{ItemTrait, LitBool, LitStr, Token, TraitItem, TraitItemFn};

pub struct ProtocolArgs {
    pub name: String,
    pub via_metadata: bool,
}

impl Parse for ProtocolArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut name: Option<String> = None;
        let mut via_metadata: bool = false;
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
        Ok(Self { name, via_metadata })
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

    let method_key_strings: Vec<String> = methods.iter().map(|m| m.ident.to_string()).collect();

    let register_fn_ident = quote::format_ident!("__register_proto_{}", trait_ident);

    // Per-method ProtocolMethod bindings emitted as a repetition.
    let method_bindings: Vec<proc_macro2::TokenStream> = method_key_strings.iter().map(|mname| {
        quote! {
            {
                let pm = ::clojure_core::ProtocolMethod {
                    protocol: proto_py.clone_ref(py),
                    key: ::std::sync::Arc::from(#mname),
                };
                let pm_py = ::pyo3::Py::new(py, pm)?;
                m.add(#mname, pm_py)?;
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
            let sym = ::clojure_core::Symbol::new(
                #ns_expr,
                ::std::sync::Arc::from(#name_part_lit),
            );
            let sym_py = ::pyo3::Py::new(py, sym)?;
            let proto = ::clojure_core::Protocol {
                name: sym_py,
                method_keys: {
                    let mut v: ::smallvec::SmallVec<[::std::sync::Arc<str>; 8]> =
                        ::smallvec::SmallVec::new();
                    #(#method_key_push)*
                    v
                },
                cache: ::std::sync::Arc::new(::clojure_core::MethodCache::new()),
                fallback: ::parking_lot::RwLock::new(None),
                via_metadata: #via_md,
            };
            let proto_py = ::pyo3::Py::new(py, proto)?;
            m.add(stringify!(#trait_ident), proto_py.clone_ref(py))?;

            #(#method_bindings)*

            Ok(())
        }

        ::inventory::submit! {
            ::clojure_core::registry::ProtocolRegistration {
                build_and_register: #register_fn_ident,
            }
        }
    }
}
