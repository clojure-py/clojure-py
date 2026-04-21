use proc_macro::TokenStream;
use syn::{parse_macro_input, ItemImpl, ItemTrait};

mod protocol;

#[proc_macro_attribute]
pub fn protocol(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as protocol::ProtocolArgs);
    let item_trait = parse_macro_input!(item as ItemTrait);
    protocol::expand(args, item_trait).into()
}

#[proc_macro_attribute]
pub fn implements(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // Placeholder — Task 18 parses attrs, Task 19 codegens.
    // For now, passthrough the impl block unchanged so any user code compiles.
    let _item_impl = parse_macro_input!(item as ItemImpl);
    quote::quote!(#_item_impl).into()
}
