use proc_macro::TokenStream;
use syn::{parse_macro_input, ItemImpl, ItemTrait};

mod implements;
mod protocol;

#[proc_macro_attribute]
pub fn protocol(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as protocol::ProtocolArgs);
    let item_trait = parse_macro_input!(item as ItemTrait);
    protocol::expand(args, item_trait).into()
}

#[proc_macro_attribute]
pub fn implements(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as implements::ImplementsArgs);
    let item_impl = parse_macro_input!(item as ItemImpl);
    implements::expand(args, item_impl).into()
}
