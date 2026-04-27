//! `register_type!` — declares a heap type, generates the destructor,
//! the `OnceCell<TypeId>`, the inventory submission, and an `alloc(...)`
//! constructor.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{parse2, Field, Fields, ItemStruct, Type};

pub fn expand(input: TokenStream) -> TokenStream {
    let item: ItemStruct = match parse2(input) {
        Ok(s) => s,
        Err(e) => return e.to_compile_error(),
    };

    if !matches!(item.fields, Fields::Named(_)) {
        return syn::Error::new_spanned(
            &item.ident,
            "register_type! requires a struct with named fields",
        ).to_compile_error();
    }

    let name = &item.ident;
    let vis  = &item.vis;
    let id_cell = format_ident!("{}_TYPE_ID", name.to_string().to_uppercase());
    let destruct_fn = format_ident!("__cljrt_destruct_{}", name);

    // Collect Value-typed fields (drop emits `gc::drop_value` for each).
    let value_fields: Vec<&Field> = match &item.fields {
        Fields::Named(named) => named.named.iter().filter(|f| is_value_type(&f.ty)).collect(),
        _ => Vec::new(),
    };
    let drops = value_fields.iter().map(|f| {
        let id = f.ident.as_ref().unwrap();
        quote! { ::clojure_rt::drop_value((*body).#id); }
    });

    let (ctor_args, ctor_inits) = match &item.fields {
        Fields::Named(named) => {
            let args = named.named.iter().map(|f| {
                let id = f.ident.as_ref().unwrap();
                let ty = &f.ty;
                quote! { #id: #ty }
            });
            let inits = named.named.iter().map(|f| {
                let id = f.ident.as_ref().unwrap();
                quote! { #id }
            });
            (quote! { #(#args),* }, quote! { #(#inits),* })
        }
        _ => (quote! {}, quote! {}),
    };

    quote! {
        #item

        #vis static #id_cell: ::once_cell::sync::OnceCell<::clojure_rt::TypeId>
            = ::once_cell::sync::OnceCell::new();

        #[allow(non_snake_case)]
        unsafe fn #destruct_fn(h: *mut ::clojure_rt::Header) {
            unsafe {
                let body = h.add(1) as *mut #name;
                #(#drops)*
                ::core::ptr::drop_in_place(body);
            }
        }

        ::clojure_rt::__inventory_submit_type! {
            ::clojure_rt::registry::StaticTypeRegistration {
                name: stringify!(#name),
                layout: ::core::alloc::Layout::new::<#name>(),
                destruct: #destruct_fn,
                id_cell: &#id_cell,
            }
        }

        impl #name {
            pub fn alloc(#ctor_args) -> ::clojure_rt::Value {
                let id = *#id_cell.get()
                    .expect(concat!(stringify!(#name), ": clojure_rt::init() not called"));
                unsafe {
                    let h = ::clojure_rt::gc::allocator()
                        .alloc(::core::alloc::Layout::new::<#name>(), id);
                    let body = h.add(1) as *mut #name;
                    ::core::ptr::write(body, #name { #ctor_inits });
                    ::clojure_rt::Value::from_heap(h)
                }
            }
        }
    }
}

fn is_value_type(ty: &Type) -> bool {
    if let Type::Path(p) = ty {
        if let Some(seg) = p.path.segments.last() {
            return seg.ident == "Value";
        }
    }
    false
}
