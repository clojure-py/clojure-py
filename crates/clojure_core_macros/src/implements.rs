use proc_macro2::TokenStream;
use quote::{format_ident, quote, ToTokens};
use syn::parse::Parse;
use syn::{ImplItem, ImplItemFn, ItemImpl, LitStr, Token};

pub struct ImplementsArgs {
    pub protocol_ident: syn::Ident,
    pub py_type: Option<String>,
    pub default: Option<syn::Path>,
}

impl Parse for ImplementsArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let protocol_ident: syn::Ident = input.parse()?;
        let mut py_type: Option<String> = None;
        let mut default: Option<syn::Path> = None;
        if input.peek(Token![,]) {
            let _: Token![,] = input.parse()?;
            let punct: syn::punctuated::Punctuated<syn::MetaNameValue, Token![,]> =
                input.parse_terminated(syn::MetaNameValue::parse, Token![,])?;
            for nv in punct {
                let key = nv
                    .path
                    .get_ident()
                    .map(|i| i.to_string())
                    .unwrap_or_default();
                match key.as_str() {
                    "py_type" => {
                        let s: LitStr = syn::parse2(nv.value.to_token_stream())?;
                        py_type = Some(s.value());
                    }
                    "default" => {
                        let p: syn::Path = syn::parse2(nv.value.to_token_stream())?;
                        default = Some(p);
                    }
                    other => {
                        return Err(syn::Error::new_spanned(
                            nv,
                            format!("unknown implements arg: {other}"),
                        ));
                    }
                }
            }
        }
        Ok(Self {
            protocol_ident,
            py_type,
            default,
        })
    }
}

fn simple_name_for(ty: &syn::Type) -> String {
    let s = quote! { #ty }.to_string();
    s.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect()
}

/// For a method whose name is `invokeN` (N in 0..=20), return Some(N).
/// For `invoke_variadic`, return None (signals variadic).
/// For any other name, return Some(method.inputs.len() - 2) (skip self + py).
fn method_arity(f: &ImplItemFn) -> Option<usize> {
    let name = f.sig.ident.to_string();
    if name == "invoke_variadic" {
        return None;
    }
    if let Some(rest) = name.strip_prefix("invoke") {
        if let Ok(n) = rest.parse::<usize>() {
            return Some(n);
        }
    }
    Some(f.sig.inputs.len().saturating_sub(2))
}

pub fn expand(args: ImplementsArgs, item_impl: ItemImpl) -> TokenStream {
    let self_ty = &item_impl.self_ty;
    let proto_ident = &args.protocol_ident;
    let install_fn_ident = format_ident!(
        "__install_impls_{}_{}",
        proto_ident,
        simple_name_for(self_ty)
    );

    // Collect the impl's concrete methods.
    let methods: Vec<&ImplItemFn> = item_impl
        .items
        .iter()
        .filter_map(|ii| {
            if let ImplItem::Fn(f) = ii { Some(f) } else { None }
        })
        .collect();

    // Per-method wrapper-builder snippets. Each puts one entry into `impls` dict.
    let method_builders: Vec<TokenStream> = methods.iter().map(|f| {
        let ident = &f.sig.ident;
        let key = ident.to_string();
        let arity = method_arity(f);
        match arity {
            None => {
                // variadic: wrapper receives args = (self, *rest). Pass rest as a PyTuple.
                quote! {
                    {
                        let f = ::pyo3::types::PyCFunction::new_closure(
                            py,
                            None,
                            None,
                            |args: &::pyo3::Bound<'_, ::pyo3::types::PyTuple>, _kw: ::std::option::Option<&::pyo3::Bound<'_, ::pyo3::types::PyDict>>| -> ::pyo3::PyResult<::pyo3::Py<::pyo3::types::PyAny>> {
                                let py = args.py();
                                let self_any = args.get_item(0)?;
                                let self_bound = self_any.cast::<#self_ty>()?;
                                let this: ::pyo3::Py<#self_ty> = self_bound.clone().unbind();
                                let rest_items: ::std::vec::Vec<::pyo3::Py<::pyo3::types::PyAny>> =
                                    (1..args.len()).map(|i| -> ::pyo3::PyResult<_> {
                                        Ok(args.get_item(i)?.unbind())
                                    }).collect::<::pyo3::PyResult<_>>()?;
                                let rest_tup = ::pyo3::types::PyTuple::new(py, &rest_items)?;
                                <#self_ty as #proto_ident>::#ident(this, py, rest_tup)
                                    .and_then(|v| ::pyo3::IntoPyObjectExt::into_py_any(v, py))
                            },
                        )?;
                        impls.set_item(#key, f)?;
                    }
                }
            }
            Some(n) => {
                // Fixed arity: expects args of length n+1 (self + n positional args).
                let arg_idents: Vec<syn::Ident> = (0..n).map(|i| format_ident!("a{}", i)).collect();
                let arg_extractions: Vec<TokenStream> = (0..n).map(|i| {
                    let ai = format_ident!("a{}", i);
                    let idx = i + 1;
                    quote! { let #ai: ::pyo3::Py<::pyo3::types::PyAny> = args.get_item(#idx)?.unbind(); }
                }).collect();
                quote! {
                    {
                        let f = ::pyo3::types::PyCFunction::new_closure(
                            py,
                            None,
                            None,
                            |args: &::pyo3::Bound<'_, ::pyo3::types::PyTuple>, _kw: ::std::option::Option<&::pyo3::Bound<'_, ::pyo3::types::PyDict>>| -> ::pyo3::PyResult<::pyo3::Py<::pyo3::types::PyAny>> {
                                let py = args.py();
                                let self_any = args.get_item(0)?;
                                let self_bound = self_any.cast::<#self_ty>()?;
                                let this: ::pyo3::Py<#self_ty> = self_bound.clone().unbind();
                                #(#arg_extractions)*
                                <#self_ty as #proto_ident>::#ident(this, py #(, #arg_idents)*)
                                    .and_then(|v| ::pyo3::IntoPyObjectExt::into_py_any(v, py))
                            },
                        )?;
                        impls.set_item(#key, f)?;
                    }
                }
            }
        }
    }).collect();

    // Resolve target type: either a Rust-side pyclass, or a Python built-in by path.
    let target_ty_expr = match &args.py_type {
        Some(path) => {
            let (mod_name, cls_name) = path
                .rsplit_once('.')
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .unwrap_or_else(|| ("builtins".to_string(), path.clone()));
            quote! {
                let ty_mod = py.import(#mod_name)?;
                let ty = ty_mod.getattr(#cls_name)?.cast_into::<::pyo3::types::PyType>()?;
            }
        }
        None => quote! {
            let ty = py.get_type::<#self_ty>();
        },
    };

    // Store the original impl (unchanged) + the install fn + the inventory submission.
    quote! {
        #item_impl

        #[allow(non_snake_case)]
        fn #install_fn_ident(
            py: ::pyo3::Python<'_>,
            m: &::pyo3::Bound<'_, ::pyo3::types::PyModule>,
        ) -> ::pyo3::PyResult<()> {
            use ::pyo3::prelude::*;
            let impls = ::pyo3::types::PyDict::new(py);
            #target_ty_expr
            #(#method_builders)*
            let proto_any = m.getattr(stringify!(#proto_ident))?;
            let proto: &::pyo3::Bound<'_, crate::Protocol> = proto_any.cast()?;
            proto.get().extend_type(py, ty, impls)?;
            Ok(())
        }

        ::inventory::submit! {
            crate::registry::ExtendRegistration {
                install: #install_fn_ident,
            }
        }
    }
}
