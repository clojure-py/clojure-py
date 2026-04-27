//! Proc-macros for the clojure-py runtime substrate.

use proc_macro::TokenStream;

mod register_type;
mod protocol;

#[proc_macro]
pub fn register_type(input: TokenStream) -> TokenStream {
    register_type::expand(input.into()).into()
}

#[proc_macro]
pub fn protocol(input: TokenStream) -> TokenStream {
    protocol::expand(input.into()).into()
}
