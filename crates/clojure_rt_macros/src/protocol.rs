//! `protocol!` — declares a protocol with one or more methods.
//! Generates a module with `ProtocolMethod` statics and `OnceCell<u32>`
//! method-id cells, plus an inventory submission.
//!
//! Input syntax (subset of trait syntax — semicolon, no body):
//!
//! ```ignore
//! protocol! {
//!     pub trait ISeq {
//!         fn first(this: Value) -> Value;
//!         fn rest(this: Value) -> Value;
//!     }
//! }
//! ```
//!
//! A method may optionally carry a *default body*; this is wired as the
//! protocol method's fallback (slow_path uses it when no per-type impl
//! is registered):
//!
//! ```ignore
//! protocol! {
//!     pub trait Counted {
//!         fn count(this: Value) -> Value {
//!             // default body — runs for any unregistered type
//!             ::clojure_rt::error::resolution_failure(&COUNT, this.tag)
//!         }
//!     }
//! }
//! ```

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{parse2, FnArg, ItemTrait, Pat, TraitItem, TraitItemFn};

pub fn expand(input: TokenStream) -> TokenStream {
    let item: ItemTrait = match parse2(input) {
        Ok(t) => t,
        Err(e) => return e.to_compile_error(),
    };
    let proto_name = &item.ident;
    let vis = &item.vis;
    let mod_name = format_ident!("{}", proto_name);
    let table_name = format_ident!("{}_METHODS", proto_name.to_string().to_uppercase());

    let methods: Vec<&TraitItemFn> = item.items.iter().filter_map(|i| {
        if let TraitItem::Fn(m) = i { Some(m) } else { None }
    }).collect();

    let static_decls = methods.iter().map(|m| {
        let mname = &m.sig.ident;
        let id_cell = format_ident!("{}_METHOD_ID", mname.to_string().to_uppercase());
        let method_static = format_ident!("{}", mname.to_string().to_uppercase());
        let mname_str = format!("{}/{}", proto_name, mname);

        match &m.default {
            None => quote! {
                pub static #id_cell: ::once_cell::sync::OnceCell<u32>
                    = ::once_cell::sync::OnceCell::new();
                pub static #method_static: ::clojure_rt::protocol::ProtocolMethod =
                    ::clojure_rt::protocol::ProtocolMethod::new(#mname_str);
            },
            Some(block) => {
                let fallback_fn = format_ident!("__cljrt_fallback_{}_{}", proto_name, mname);
                let n_expected = m.sig.inputs.len();
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
                quote! {
                    pub static #id_cell: ::once_cell::sync::OnceCell<u32>
                        = ::once_cell::sync::OnceCell::new();

                    #[allow(non_snake_case)]
                    unsafe extern "C" fn #fallback_fn(
                        args: *const ::clojure_rt::Value,
                        n: usize,
                    ) -> ::clojure_rt::Value {
                        debug_assert_eq!(n, #n_expected);
                        #(#arg_binds)*
                        #block
                    }

                    pub static #method_static: ::clojure_rt::protocol::ProtocolMethod =
                        ::clojure_rt::protocol::ProtocolMethod::with_fallback(
                            #mname_str,
                            #fallback_fn,
                        );
                }
            }
        }
    });

    let table_entries = methods.iter().map(|m| {
        let mname = &m.sig.ident;
        let id_cell = format_ident!("{}_METHOD_ID", mname.to_string().to_uppercase());
        let method_static = format_ident!("{}", mname.to_string().to_uppercase());
        let mname_str = format!("{}", mname);
        quote! {
            ::clojure_rt::registry::StaticProtocolMethodEntry {
                name: #mname_str,
                method: &#mod_name::#method_static,
                method_id_cell: &#mod_name::#id_cell,
            }
        }
    });

    let proto_str = format!("{}", proto_name);

    quote! {
        #[allow(non_snake_case)]
        #vis mod #mod_name {
            #![allow(non_upper_case_globals)]
            #(#static_decls)*
        }

        static #table_name: &[::clojure_rt::registry::StaticProtocolMethodEntry] = &[
            #(#table_entries),*
        ];

        ::clojure_rt::__inventory_submit_protocol! {
            ::clojure_rt::registry::StaticProtocolRegistration {
                name: #proto_str,
                methods: #table_name,
            }
        }
    }
}
