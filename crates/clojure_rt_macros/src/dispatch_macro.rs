//! `dispatch!(Proto::method, &args)` — emits a per-call-site IC slot
//! (a `static IC: ICSlot`) and the tier-1 fast path with fall-through
//! to `clojure_rt::dispatch::slow_path`.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse::Parser, punctuated::Punctuated, Expr, Token};

pub fn expand(input: TokenStream) -> TokenStream {
    let parser = Punctuated::<Expr, Token![,]>::parse_terminated;
    let parsed = match parser.parse2(input) {
        Ok(p) => p,
        Err(e) => return e.to_compile_error(),
    };
    let mut iter = parsed.into_iter();
    let method = match iter.next() {
        Some(e) => e,
        None => return syn::Error::new(proc_macro2::Span::call_site(),
            "dispatch!: expected method path").to_compile_error(),
    };
    let args_expr = match iter.next() {
        Some(e) => e,
        None => return syn::Error::new(proc_macro2::Span::call_site(),
            "dispatch!: expected args slice").to_compile_error(),
    };

    // For Proto::method, the static is Proto::METHOD (uppercase). The
    // user is expected to write `Proto::method` in the macro input;
    // we preserve the path's last segment but uppercase it.
    let method_static = uppercase_last_segment(&method);

    quote! {{
        static IC: ::clojure_rt::dispatch::ic::ICSlot
            = ::clojure_rt::dispatch::ic::ICSlot::EMPTY;
        let __args: &[::clojure_rt::Value] = #args_expr;
        let __m: &::clojure_rt::protocol::ProtocolMethod = &#method_static;
        ::clojure_rt::dispatch::dispatch_fn(&IC, __m, __args)
    }}
}

fn uppercase_last_segment(method: &Expr) -> TokenStream {
    use quote::ToTokens;
    if let Expr::Path(p) = method {
        let mut path = p.path.clone();
        if let Some(last) = path.segments.last_mut() {
            let upper = last.ident.to_string().to_uppercase();
            last.ident = syn::Ident::new(&upper, last.ident.span());
        }
        return path.to_token_stream();
    }
    method.to_token_stream()
}
