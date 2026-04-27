//! `implements!` — implements a protocol for a type. Generates one
//! `unsafe extern "C" fn` per method body and emits an inventory entry
//! per method that init wires into the type's PerTypeTable.
//!
//! **Empty body → marker registration.** An impl block with no `fn`
//! items (e.g. `implements! { impl ISequential for ConsObj {} }`)
//! emits a single registration against the protocol's synthetic
//! `MARKER` method (see `protocol!` for the generation), with a no-op
//! fn pointer. This makes `clojure_rt::protocol::satisfies(&Proto::MARKER, v)`
//! answer correctly without users needing to write a sentinel method.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{parse2, FnArg, ImplItem, ImplItemFn, ItemImpl, Pat, Type, TypePath};

pub fn expand(input: TokenStream) -> TokenStream {
    let item: ItemImpl = match parse2(input) {
        Ok(i) => i,
        Err(e) => return e.to_compile_error(),
    };
    let proto_path = match item.trait_.as_ref() {
        Some((_, path, _)) => path,
        None => return syn::Error::new_spanned(&item, "implements!: must be `impl PROTO for TYPE`")
            .to_compile_error(),
    };
    let proto_name = match proto_path.segments.last() {
        Some(s) => s.ident.clone(),
        None => return quote! {},
    };
    let type_path = match &*item.self_ty {
        Type::Path(TypePath { path, .. }) => path,
        _ => return syn::Error::new_spanned(&item.self_ty, "implements!: only path types").to_compile_error(),
    };
    let type_name = match type_path.segments.last() {
        Some(s) => s.ident.clone(),
        None => return quote! {},
    };
    let type_id_cell = format_ident!("{}_TYPE_ID", type_name.to_string().to_uppercase());

    let fns: Vec<&ImplItemFn> = item.items.iter().filter_map(|i| match i {
        ImplItem::Fn(m) => Some(m),
        _ => None,
    }).collect();

    if fns.is_empty() {
        // Marker impl: register a no-op against the protocol's MARKER
        // method. The protocol! macro guarantees MARKER exists for any
        // zero-method protocol; for protocols that *do* declare methods,
        // an empty implements! block is a user error caught here.
        return emit_marker(&proto_name, &type_name, &type_id_cell);
    }

    let method_outputs = fns.iter().map(|m| emit_method(&proto_name, &type_name, &type_id_cell, m));

    quote! {
        #(#method_outputs)*
    }
}

fn emit_marker(
    proto: &syn::Ident,
    type_name: &syn::Ident,
    type_id_cell: &syn::Ident,
) -> TokenStream {
    let extern_fn = format_ident!("__cljrt_marker_{}_{}", proto, type_name);
    quote! {
        #[allow(non_snake_case)]
        unsafe extern "C" fn #extern_fn(
            _args: *const ::clojure_rt::Value,
            _n: usize,
        ) -> ::clojure_rt::Value {
            // Marker impl — never called by user code; presence in the
            // type's per-type table is what `satisfies?` detects.
            ::clojure_rt::Value::NIL
        }

        ::clojure_rt::__inventory_submit_impl! {
            ::clojure_rt::registry::StaticImplRegistration {
                type_cell: &#type_id_cell,
                method_id_cell: &#proto::MARKER_METHOD_ID,
                method_version: &#proto::MARKER.version,
                fn_ptr: #extern_fn as *const (),
            }
        }
    }
}

fn emit_method(
    proto: &syn::Ident,
    type_name: &syn::Ident,
    type_id_cell: &syn::Ident,
    m: &ImplItemFn,
) -> TokenStream {
    let mname = &m.sig.ident;
    let body  = &m.block;
    let extern_fn = format_ident!("__cljrt_impl_{}_{}_{}", proto, type_name, mname);
    let mid_cell = format_ident!("{}_METHOD_ID", mname.to_string().to_uppercase());
    let method_static = format_ident!("{}", mname.to_string().to_uppercase());

    // Bind named args from `args[i]`. Names come from the user's fn sig.
    let arg_binds = m.sig.inputs.iter().enumerate().filter_map(|(i, a)| {
        if let FnArg::Typed(pat) = a {
            if let Pat::Ident(id) = &*pat.pat {
                let ident = &id.ident;
                let ty = &pat.ty;
                return Some(quote! {
                    let #ident: #ty = unsafe { *args.add(#i) };
                });
            }
        }
        None
    });

    let n_expected = m.sig.inputs.len();

    quote! {
        #[allow(non_snake_case)]
        unsafe extern "C" fn #extern_fn(
            args: *const ::clojure_rt::Value,
            n: usize,
        ) -> ::clojure_rt::Value {
            debug_assert_eq!(n, #n_expected);
            #(#arg_binds)*
            #body
        }

        ::clojure_rt::__inventory_submit_impl! {
            ::clojure_rt::registry::StaticImplRegistration {
                type_cell: &#type_id_cell,
                method_id_cell: &#proto::#mid_cell,
                method_version: &#proto::#method_static.version,
                fn_ptr: #extern_fn as *const (),
            }
        }
    }
}
