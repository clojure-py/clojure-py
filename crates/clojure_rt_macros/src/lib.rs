//! Proc-macros for the clojure-py runtime substrate.

use proc_macro::TokenStream;

mod register_type;

#[proc_macro]
pub fn register_type(input: TokenStream) -> TokenStream {
    register_type::expand(input.into()).into()
}
