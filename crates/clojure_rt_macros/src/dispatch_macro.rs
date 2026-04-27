//! `dispatch!(Proto::method, &args)` — emits a per-call-site IC slot
//! (a `static IC: ICSlot`) and the tier-1 fast path with fall-through
//! to `clojure_rt::dispatch::slow_path`.
//!
//! The args expression *must* be a literal slice (`&[a, b, c]`) so that
//! the macro can count its elements at expand time. The element count
//! becomes the arity, which is used to mangle the path's last segment
//! to its arity-suffixed form (`Proto::method` + 2 elems →
//! `Proto::METHOD_2`). If the path already carries an explicit
//! `_<digits>` suffix it must match the slice length.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse::Parser, punctuated::Punctuated, Expr, ExprArray, ExprReference, Token};

use crate::arity::mangled_ident_at;

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

    let arity = match arity_from_slice_literal(&args_expr) {
        Ok(n) => n,
        Err(e) => return e.to_compile_error(),
    };

    let method_static = match mangle_path(&method, arity) {
        Ok(ts) => ts,
        Err(e) => return e.to_compile_error(),
    };

    quote! {{
        static IC: ::clojure_rt::dispatch::ic::ICSlot
            = ::clojure_rt::dispatch::ic::ICSlot::EMPTY;
        let __args: &[::clojure_rt::Value] = #args_expr;
        let __m: &::clojure_rt::protocol::ProtocolMethod = &#method_static;
        ::clojure_rt::dispatch::dispatch_fn(&IC, __m, __args)
    }}
}

/// Pull the element count out of a literal slice expression.
/// Accepts `&[a, b, c]` and `&[a; 0]` is rejected (the repeat form is
/// uncommon in our call sites and would smuggle a non-literal length).
fn arity_from_slice_literal(expr: &Expr) -> Result<usize, syn::Error> {
    if let Expr::Reference(ExprReference { expr: inner, .. }) = expr {
        if let Expr::Array(ExprArray { elems, .. }) = &**inner {
            return Ok(elems.len());
        }
    }
    Err(syn::Error::new_spanned(
        expr,
        "dispatch!: args must be a literal slice like `&[a, b, c]` so the macro can derive arity",
    ))
}

/// Mangle the last segment of `Proto::method` to its arity-suffixed,
/// uppercase form: `Proto::METHOD_<arity>`. Path may already carry an
/// explicit suffix; if so it must match.
fn mangle_path(method: &Expr, arity: usize) -> Result<TokenStream, syn::Error> {
    use quote::ToTokens;
    let Expr::Path(p) = method else {
        return Ok(method.to_token_stream());
    };
    let mut path = p.path.clone();
    let Some(last) = path.segments.last_mut() else {
        return Ok(path.to_token_stream());
    };
    let mangled = mangled_ident_at(&last.ident, arity, last.ident.span())?;
    let upper = syn::Ident::new(
        &mangled.to_string().to_uppercase(),
        last.ident.span(),
    );
    last.ident = upper;
    Ok(path.to_token_stream())
}
