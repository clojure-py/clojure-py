//! Proc-macros for the clojure-py runtime substrate.

use proc_macro::TokenStream;

mod arity;
mod register_type;
mod protocol;
mod implements;
mod dispatch_macro;

#[proc_macro]
pub fn register_type(input: TokenStream) -> TokenStream {
    register_type::expand(input.into()).into()
}

#[proc_macro]
pub fn protocol(input: TokenStream) -> TokenStream {
    protocol::expand(input.into()).into()
}

#[proc_macro]
pub fn implements(input: TokenStream) -> TokenStream {
    implements::expand(input.into()).into()
}

#[proc_macro]
pub fn dispatch(input: TokenStream) -> TokenStream {
    dispatch_macro::expand(input.into()).into()
}
