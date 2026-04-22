use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use syn::parse::Parse;
use syn::{ItemImpl, LitStr, Token};

pub struct ImplementsArgs {
    pub protocol_ident: syn::Ident,
    pub py_type: Option<String>,
    pub default: Option<syn::Path>,
}

impl Parse for ImplementsArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let protocol_ident: syn::Ident = input.parse()?;
        let mut py_type: Option<String> = None;
        let mut default: Option<syn::Path> = None;
        if input.peek(Token![,]) {
            let _: Token![,] = input.parse()?;
            let punct: syn::punctuated::Punctuated<syn::MetaNameValue, Token![,]> =
                input.parse_terminated(syn::MetaNameValue::parse, Token![,])?;
            for nv in punct {
                let key = nv
                    .path
                    .get_ident()
                    .map(|i| i.to_string())
                    .unwrap_or_default();
                match key.as_str() {
                    "py_type" => {
                        let s: LitStr = syn::parse2(nv.value.to_token_stream())?;
                        py_type = Some(s.value());
                    }
                    "default" => {
                        let p: syn::Path = syn::parse2(nv.value.to_token_stream())?;
                        default = Some(p);
                    }
                    other => {
                        return Err(syn::Error::new_spanned(
                            nv,
                            format!("unknown implements arg: {other}"),
                        ));
                    }
                }
            }
        }
        Ok(Self {
            protocol_ident,
            py_type,
            default,
        })
    }
}

/// Stub — Task 19 replaces with real codegen.
pub fn expand(_args: ImplementsArgs, item_impl: ItemImpl) -> TokenStream {
    quote! { #item_impl }
}
