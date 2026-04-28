//! `register_type!` — declares a heap type, generates the destructor,
//! the `OnceCell<TypeId>`, the inventory submission, and an `alloc(...)`
//! constructor.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    parse2, Field, Fields, GenericArgument, ItemStruct, PathArguments, Type, TypePath,
};

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

    // Per-field destructor snippets. We recognize three Value-bearing
    // shapes and emit a `drop_value` for each contained `Value` *before*
    // `ptr::drop_in_place(body)` runs the type's natural destructor:
    //
    //   - `Value`            — single decref.
    //   - `Box<[Value]>`     — loop over the slice (Box::drop afterwards
    //                          frees the slice memory).
    //   - `Vec<Value>`       — loop over the elements (Vec::drop frees
    //                          the buffer afterwards). Used by transients
    //                          for mutable in-place storage.
    //
    // Any other field type (i32, AtomicI32, OnceLock<...>, etc.) is left
    // for `drop_in_place` alone.
    let drops: Vec<TokenStream> = match &item.fields {
        Fields::Named(named) => {
            named.named.iter().filter_map(field_drop_snippet).collect()
        }
        _ => Vec::new(),
    };

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
            #[inline]
            pub fn alloc(#ctor_args) -> ::clojure_rt::Value {
                let id = *#id_cell.get()
                    .expect(concat!(stringify!(#name), ": clojure_rt::init() not called"));
                unsafe {
                    // Direct call into RCImmix's concrete fast path —
                    // bypasses the `dyn GcAllocator` vtable so LLVM can
                    // inline through to the bump-pointer hot path.
                    let h = ::clojure_rt::gc::rcimmix::RCIMMIX
                        .alloc_inline(::core::alloc::Layout::new::<#name>(), id);
                    let body = h.add(1) as *mut #name;
                    ::core::ptr::write(body, #name { #ctor_inits });
                    ::clojure_rt::Value::from_heap(h)
                }
            }

            /// Borrow the body of a `Self`-tagged Value.
            ///
            /// # Safety
            /// - `v` must be a live `Value` of this type.
            /// - The returned reference must not outlive any copy of
            ///   `v` reaching zero refcount.
            ///
            /// Debug builds tag-assert; release builds skip the check
            /// for zero overhead on the dispatch fast path.
            #[inline]
            #[allow(dead_code)]
            pub unsafe fn body<'a>(v: ::clojure_rt::Value) -> &'a Self {
                debug_assert_eq!(
                    v.tag,
                    *#id_cell.get().expect(
                        concat!(stringify!(#name), ": clojure_rt::init() not called")
                    ),
                    concat!(stringify!(#name), "::body: wrong tag"),
                );
                let h = v.as_heap().expect(
                    concat!(stringify!(#name), "::body: not a heap Value"),
                );
                unsafe { &*(h.add(1) as *const Self) }
            }

            /// Mutably borrow the body of a `Self`-tagged Value. Used
            /// by transients for in-place mutation.
            ///
            /// # Safety
            /// In addition to the contract of `body`, the caller must
            /// guarantee that no other `&Self` / `&mut Self` borrow is
            /// outstanding for `v`'s body at the same time. Transient
            /// types enforce this by document-level "single-thread
            /// single-owner" contract; persistent types should not use
            /// `body_mut`.
            #[inline]
            #[allow(dead_code)]
            pub unsafe fn body_mut<'a>(v: ::clojure_rt::Value) -> &'a mut Self {
                debug_assert_eq!(
                    v.tag,
                    *#id_cell.get().expect(
                        concat!(stringify!(#name), ": clojure_rt::init() not called")
                    ),
                    concat!(stringify!(#name), "::body_mut: wrong tag"),
                );
                let h = v.as_heap().expect(
                    concat!(stringify!(#name), "::body_mut: not a heap Value"),
                );
                unsafe { &mut *(h.add(1) as *mut Self) }
            }
        }
    }
}

fn field_drop_snippet(f: &Field) -> Option<TokenStream> {
    let id = f.ident.as_ref().unwrap();
    if is_value_type(&f.ty) {
        return Some(quote! {
            ::clojure_rt::drop_value((*body).#id);
        });
    }
    if is_value_box_slice(&f.ty) || is_value_vec(&f.ty) {
        return Some(quote! {
            for v in (*body).#id.iter() {
                ::clojure_rt::drop_value(*v);
            }
        });
    }
    None
}

fn is_value_type(ty: &Type) -> bool {
    if let Type::Path(p) = ty {
        if let Some(seg) = p.path.segments.last() {
            return seg.ident == "Value";
        }
    }
    false
}

/// Recognize `Box<[Value]>` (the only Box shape we destructure here —
/// other `Box<T>` fields go through `drop_in_place` only).
fn is_value_box_slice(ty: &Type) -> bool {
    let Type::Path(TypePath { path, .. }) = ty else { return false };
    let Some(seg) = path.segments.last() else { return false };
    if seg.ident != "Box" {
        return false;
    }
    let PathArguments::AngleBracketed(args) = &seg.arguments else { return false };
    let Some(GenericArgument::Type(Type::Slice(slice))) = args.args.first() else {
        return false;
    };
    is_value_type(&slice.elem)
}

/// Recognize `Vec<Value>`. Used by transients for mutable Value-bearing
/// storage that grows in place.
fn is_value_vec(ty: &Type) -> bool {
    let Type::Path(TypePath { path, .. }) = ty else { return false };
    let Some(seg) = path.segments.last() else { return false };
    if seg.ident != "Vec" {
        return false;
    }
    let PathArguments::AngleBracketed(args) = &seg.arguments else { return false };
    let Some(GenericArgument::Type(inner_ty)) = args.args.first() else { return false };
    is_value_type(inner_ty)
}
