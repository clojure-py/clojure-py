//! Integration tests for `SymbolObj` — construction, Named, equiv,
//! hash, and the meta lifecycle (IMeta + IObj).

use clojure_rt::types::string::StringObj;
use clojure_rt::{drop_value, init, rt};

#[test]
fn symbol_without_namespace() {
    init();
    let s = rt::symbol(None, "foo");
    assert!(rt::namespace(s).is_nil());
    let nm = rt::name(s);
    assert_eq!(unsafe { StringObj::as_str_unchecked(nm) }, "foo");
    drop_value(nm);
    drop_value(s);
}

#[test]
fn symbol_with_namespace() {
    init();
    let s = rt::symbol(Some("clj"), "core");
    let ns = rt::namespace(s);
    let nm = rt::name(s);
    assert_eq!(unsafe { StringObj::as_str_unchecked(ns) }, "clj");
    assert_eq!(unsafe { StringObj::as_str_unchecked(nm) }, "core");
    drop_value(ns);
    drop_value(nm);
    drop_value(s);
}

#[test]
fn symbol_equiv_same_name_no_ns() {
    init();
    let a = rt::symbol(None, "foo");
    let b = rt::symbol(None, "foo");
    assert_eq!(rt::equiv(a, b).as_bool(), Some(true));
    drop_value(a);
    drop_value(b);
}

#[test]
fn symbol_equiv_same_name_same_ns() {
    init();
    let a = rt::symbol(Some("ns"), "foo");
    let b = rt::symbol(Some("ns"), "foo");
    assert_eq!(rt::equiv(a, b).as_bool(), Some(true));
    drop_value(a);
    drop_value(b);
}

#[test]
fn symbol_equiv_differing_ns() {
    init();
    let a = rt::symbol(Some("ns1"), "foo");
    let b = rt::symbol(Some("ns2"), "foo");
    assert_eq!(rt::equiv(a, b).as_bool(), Some(false));
    drop_value(a);
    drop_value(b);
}

#[test]
fn symbol_equiv_nil_vs_string_ns_is_false() {
    init();
    let a = rt::symbol(None, "foo");
    let b = rt::symbol(Some("ns"), "foo");
    assert_eq!(rt::equiv(a, b).as_bool(), Some(false));
    drop_value(a);
    drop_value(b);
}

#[test]
fn symbol_equiv_differing_name_is_false() {
    init();
    let a = rt::symbol(None, "foo");
    let b = rt::symbol(None, "bar");
    assert_eq!(rt::equiv(a, b).as_bool(), Some(false));
    drop_value(a);
    drop_value(b);
}

#[test]
fn symbol_hash_is_deterministic_and_stable() {
    init();
    let a = rt::symbol(Some("ns"), "foo");
    let b = rt::symbol(Some("ns"), "foo");
    let h1 = rt::hash(a).as_int();
    let h2 = rt::hash(b).as_int();
    let h1_again = rt::hash(a).as_int();
    assert_eq!(h1, h2, "equal symbols must hash equal");
    assert_eq!(h1, h1_again, "hash must be cached deterministically");
    drop_value(a);
    drop_value(b);
}

#[test]
fn symbol_hash_differs_with_ns() {
    init();
    let a = rt::symbol(None, "foo");
    let b = rt::symbol(Some("ns"), "foo");
    assert_ne!(rt::hash(a).as_int(), rt::hash(b).as_int());
    drop_value(a);
    drop_value(b);
}

#[test]
fn symbol_meta_defaults_to_nil() {
    init();
    let s = rt::symbol(None, "foo");
    assert!(rt::meta(s).is_nil());
    drop_value(s);
}

#[test]
fn with_meta_replaces_metadata_and_preserves_identity() {
    init();
    let s = rt::symbol(Some("ns"), "foo");
    let m = rt::str_new("some-meta");

    let s_with_m = rt::with_meta(s, m);

    // Meta is now reachable from the new symbol.
    let read = rt::meta(s_with_m);
    assert_eq!(rt::equiv(read, m).as_bool(), Some(true));
    drop_value(read);

    // Equiv still holds: same name+ns → equal regardless of meta.
    assert_eq!(rt::equiv(s, s_with_m).as_bool(), Some(true));
    // Hash also unchanged.
    assert_eq!(rt::hash(s).as_int(), rt::hash(s_with_m).as_int());

    drop_value(m);
    drop_value(s);
    drop_value(s_with_m);
}

#[test]
fn with_meta_does_not_mutate_original() {
    init();
    let s = rt::symbol(None, "foo");
    let m = rt::str_new("meta");
    let s_with_m = rt::with_meta(s, m);

    // Original `s` is untouched.
    assert!(rt::meta(s).is_nil());
    assert!(!rt::meta(s_with_m).is_nil());

    drop_value(rt::meta(s_with_m));   // released
    drop_value(m);
    drop_value(s);
    drop_value(s_with_m);
}
