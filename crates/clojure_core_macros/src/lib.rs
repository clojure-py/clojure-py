use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn protocol(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item  // passthrough for now
}

#[proc_macro_attribute]
pub fn implements(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item  // passthrough for now
}
