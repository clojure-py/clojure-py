//! Shared name-mangling: protocol method statics are always
//! arity-suffixed (`<NAME>_<arity>`). A method written without an
//! explicit suffix gets one auto-appended; a method written *with* a
//! `_<digits>` suffix must have that count match the input arity.
//!
//! The same rules apply across `protocol!`, `implements!`, and
//! `dispatch!` so that all three macros refer to the same Rust
//! identifier when they emit per-method symbols.

use proc_macro2::Span;
use quote::format_ident;
use syn::Ident;

pub struct ArityName {
    /// The identifier the macro should emit (e.g. `count_1`, `lookup_2`).
    /// Already includes the arity suffix.
    pub mangled_ident: Ident,
    /// String form of `mangled_ident`. Used in `name:` registry fields
    /// and as the `Proto/method` slug for diagnostics.
    pub mangled_str: String,
}

/// Parse a method ident written by the user against the actual arity
/// (parameter count). Returns the canonical arity-suffixed form.
///
/// `count`     + arity 1 → `count_1`
/// `count_1`   + arity 1 → `count_1`
/// `lookup_2`  + arity 2 → `lookup_2`
/// `lookup_3`  + arity 3 → `lookup_3`
/// `lookup_2`  + arity 3 → ERROR (suffix mismatches arity)
pub fn parse_arity_name(ident: &Ident, arity: usize) -> Result<ArityName, syn::Error> {
    let s = ident.to_string();
    if let Some((stem, declared_arity)) = split_trailing_arity(&s) {
        if declared_arity != arity {
            return Err(syn::Error::new(
                ident.span(),
                format!(
                    "method `{}` has explicit arity suffix `_{}` but takes {} parameter(s) — \
                     suffix must match parameter count",
                    s, declared_arity, arity
                ),
            ));
        }
        let _ = stem;
        Ok(ArityName {
            mangled_ident: ident.clone(),
            mangled_str: s,
        })
    } else {
        let mangled = format!("{}_{}", s, arity);
        Ok(ArityName {
            mangled_ident: format_ident!("{}", mangled),
            mangled_str: mangled,
        })
    }
}

/// Like `parse_arity_name` but returns just the mangled ident form,
/// rebuilt at the requested span — for cases where we synthesize a
/// path segment from a different origin span (e.g. `dispatch!` mangling
/// the user's path).
pub fn mangled_ident_at(ident: &Ident, arity: usize, span: Span) -> Result<Ident, syn::Error> {
    let parsed = parse_arity_name(ident, arity)?;
    Ok(Ident::new(&parsed.mangled_ident.to_string(), span))
}

/// If `s` ends with `_<digits>`, split into (stem, digits-as-usize).
/// Returns None if there's no trailing `_<digits>` segment.
fn split_trailing_arity(s: &str) -> Option<(&str, usize)> {
    let bytes = s.as_bytes();
    let mut i = bytes.len();
    // Trailing digits.
    while i > 0 && bytes[i - 1].is_ascii_digit() {
        i -= 1;
    }
    if i == bytes.len() || i == 0 {
        return None;
    }
    if bytes[i - 1] != b'_' {
        return None;
    }
    let digits = &s[i..];
    let stem = &s[..i - 1];
    digits.parse::<usize>().ok().map(|n| (stem, n))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proc_macro2::Span;

    fn ident(s: &str) -> Ident { Ident::new(s, Span::call_site()) }

    #[test]
    fn no_suffix_appends() {
        let p = parse_arity_name(&ident("count"), 1).unwrap();
        assert_eq!(p.mangled_str, "count_1");
        let p = parse_arity_name(&ident("equiv"), 2).unwrap();
        assert_eq!(p.mangled_str, "equiv_2");
    }

    #[test]
    fn matching_suffix_kept() {
        let p = parse_arity_name(&ident("lookup_2"), 2).unwrap();
        assert_eq!(p.mangled_str, "lookup_2");
        let p = parse_arity_name(&ident("nth_3"), 3).unwrap();
        assert_eq!(p.mangled_str, "nth_3");
    }

    #[test]
    fn mismatching_suffix_errors() {
        assert!(parse_arity_name(&ident("lookup_2"), 3).is_err());
    }

    #[test]
    fn nondigit_underscore_treated_as_no_suffix() {
        // `nth_default` → stem doesn't end with _<digits>, so suffix is appended.
        let p = parse_arity_name(&ident("nth_default"), 3).unwrap();
        assert_eq!(p.mangled_str, "nth_default_3");
    }
}
