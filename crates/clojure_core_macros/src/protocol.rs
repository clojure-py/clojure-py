use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use syn::parse::Parse;
use syn::{ItemTrait, LitBool, LitStr, Token, TraitItem, TraitItemFn};

pub struct ProtocolArgs {
    pub name: String,
    pub via_metadata: bool,
}

impl Parse for ProtocolArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut name: Option<String> = None;
        let mut via_metadata: bool = false;
        let punct: syn::punctuated::Punctuated<syn::MetaNameValue, Token![,]> =
            input.parse_terminated(syn::MetaNameValue::parse, Token![,])?;
        for nv in punct {
            let key = nv
                .path
                .get_ident()
                .map(|i| i.to_string())
                .unwrap_or_default();
            match key.as_str() {
                "name" => {
                    let s: LitStr = syn::parse2(nv.value.to_token_stream())?;
                    name = Some(s.value());
                }
                "extend_via_metadata" => {
                    let b: LitBool = syn::parse2(nv.value.to_token_stream())?;
                    via_metadata = b.value;
                }
                other => {
                    return Err(syn::Error::new_spanned(
                        nv,
                        format!("unknown protocol arg: {other}"),
                    ));
                }
            }
        }
        let name = name.ok_or_else(|| {
            syn::Error::new(input.span(), "protocol requires name = \"...\"")
        })?;
        Ok(Self { name, via_metadata })
    }
}

pub struct MethodInfo {
    pub ident: syn::Ident,
    /// `None` = variadic (trait method named `invoke_variadic`).
    /// `Some(n)` = fixed arity `invoke{n}`.
    pub arity: Option<usize>,
}

pub fn method_infos(item: &ItemTrait) -> Vec<MethodInfo> {
    item.items
        .iter()
        .filter_map(|ti| {
            if let TraitItem::Fn(TraitItemFn { sig, .. }) = ti {
                let name = sig.ident.to_string();
                let arity = if name == "invoke_variadic" {
                    None
                } else if let Some(rest) = name.strip_prefix("invoke") {
                    rest.parse::<usize>().ok()
                } else {
                    // Method doesn't match invoke{N} or invoke_variadic — still a trait
                    // method but with no protocol-dispatch arity. We still surface it so
                    // codegen (Task 15) can decide what to do. For non-IFn-shaped
                    // protocols, arity doesn't apply; Task 15 uses the ident directly.
                    None
                };
                Some(MethodInfo {
                    ident: sig.ident.clone(),
                    arity,
                })
            } else {
                None
            }
        })
        .collect()
}

/// Stub — Task 15 fills this in with real codegen.
pub fn expand(_args: ProtocolArgs, item: ItemTrait) -> TokenStream {
    quote! { #item }
}
