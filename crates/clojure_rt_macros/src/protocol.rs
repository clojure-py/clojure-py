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

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{parse2, ItemTrait, TraitItem, TraitItemFn};

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
        quote! {
            pub static #id_cell: ::once_cell::sync::OnceCell<u32>
                = ::once_cell::sync::OnceCell::new();
            pub static #method_static: ::clojure_rt::protocol::ProtocolMethod =
                ::clojure_rt::protocol::ProtocolMethod::new(#mname_str);
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
