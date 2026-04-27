//! Integration tests for `KeywordObj` — interning, Named, equiv, hash.

use clojure_rt::types::string::StringObj;
use clojure_rt::{drop_value, init, rt};

#[test]
fn keyword_interning_returns_same_value() {
    init();
    let a = rt::keyword(None, "foo");
    let b = rt::keyword(None, "foo");
    // Strong-ref interning: identical name+ns ⇒ same heap pointer.
    assert_eq!(a.payload, b.payload, "interned keywords must share identity");
    drop_value(a);
    drop_value(b);
}

#[test]
fn keyword_with_namespace_interning() {
    init();
    let a = rt::keyword(Some("ns"), "foo");
    let b = rt::keyword(Some("ns"), "foo");
    assert_eq!(a.payload, b.payload);
    drop_value(a);
    drop_value(b);
}

#[test]
fn keyword_different_names_are_distinct() {
    init();
    let a = rt::keyword(None, "foo");
    let b = rt::keyword(None, "bar");
    assert_ne!(a.payload, b.payload);
    drop_value(a);
    drop_value(b);
}

#[test]
fn keyword_named_returns_underlying_strings() {
    init();
    let kw = rt::keyword(Some("ns"), "name");
    let ns = rt::namespace(kw);
    let nm = rt::name(kw);
    assert_eq!(unsafe { StringObj::as_str_unchecked(ns) }, "ns");
    assert_eq!(unsafe { StringObj::as_str_unchecked(nm) }, "name");
    drop_value(ns);
    drop_value(nm);
    drop_value(kw);
}

#[test]
fn keyword_no_ns_named_returns_nil() {
    init();
    let kw = rt::keyword(None, "foo");
    assert!(rt::namespace(kw).is_nil());
    drop_value(kw);
}

#[test]
fn keyword_equiv_same_is_true() {
    init();
    let a = rt::keyword(None, "foo");
    let b = rt::keyword(None, "foo");
    assert_eq!(rt::equiv(a, b).as_bool(), Some(true));
    drop_value(a);
    drop_value(b);
}

#[test]
fn keyword_equiv_with_symbol_is_false() {
    init();
    let kw  = rt::keyword(None, "foo");
    let sym = rt::symbol(None, "foo");
    // :foo and 'foo are NOT equal under Clojure's =. Different types.
    assert_eq!(rt::equiv(kw, sym).as_bool(), Some(false));
    drop_value(kw);
    drop_value(sym);
}

#[test]
fn keyword_hash_matches_sym_hash_plus_golden_ratio() {
    init();
    let kw  = rt::keyword(None, "foo");
    let sym = rt::symbol(None, "foo");
    let kh = rt::hash(kw).as_int().unwrap() as i32;
    let sh = rt::hash(sym).as_int().unwrap() as i32;
    let golden = 0x9e3779b9_u32 as i32;
    assert_eq!(kh, sh.wrapping_add(golden));
    drop_value(kw);
    drop_value(sym);
}

#[test]
fn keyword_hash_with_ns_matches_sym_hash() {
    init();
    let kw  = rt::keyword(Some("clj"), "core");
    let sym = rt::symbol(Some("clj"), "core");
    let kh = rt::hash(kw).as_int().unwrap() as i32;
    let sh = rt::hash(sym).as_int().unwrap() as i32;
    let golden = 0x9e3779b9_u32 as i32;
    assert_eq!(kh, sh.wrapping_add(golden));
    drop_value(kw);
    drop_value(sym);
}
