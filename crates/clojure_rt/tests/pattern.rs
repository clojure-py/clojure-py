//! `PatternObj` tests — round-trip through the heap, equality on
//! source string, hash determinism, error path on malformed
//! regex, and a basic match smoke-test through the Rust regex API.

use clojure_rt::{drop_value, init, rt, Value};
use clojure_rt::types::pattern::PatternObj;

fn drop_all(vs: &[Value]) { for &v in vs { drop_value(v); } }

#[test]
fn pattern_round_trip() {
    init();
    let p = rt::pattern_from_str(r"\d+");
    assert!(!p.is_exception());
    let src = unsafe { PatternObj::source(p) };
    assert_eq!(src, r"\d+");
    drop_value(p);
}

#[test]
fn equal_source_means_equiv() {
    init();
    let a = rt::pattern_from_str(r"\d+");
    let b = rt::pattern_from_str(r"\d+");
    assert!(rt::equiv(a, b).as_bool().unwrap_or(false));
    drop_all(&[a, b]);
}

#[test]
fn distinct_sources_are_not_equiv() {
    init();
    let a = rt::pattern_from_str(r"\d+");
    let b = rt::pattern_from_str(r"\w+");
    assert_eq!(rt::equiv(a, b).as_bool(), Some(false));
    drop_all(&[a, b]);
}

#[test]
fn hash_consistent_for_equal_patterns() {
    init();
    let a = rt::pattern_from_str(r"^foo.*bar$");
    let b = rt::pattern_from_str(r"^foo.*bar$");
    assert_eq!(rt::hash(a).as_int(), rt::hash(b).as_int());
    drop_all(&[a, b]);
}

#[test]
fn malformed_pattern_returns_exception() {
    init();
    // Unclosed character class — regex compile failure.
    let v = rt::pattern_from_str(r"[abc");
    assert!(v.is_exception());
    drop_value(v);
}

#[test]
fn regex_actually_matches() {
    // Smoke test that the underlying engine works — we rely on
    // it for the future re-find / re-matches helpers.
    init();
    let p = rt::pattern_from_str(r"^\d+$");
    let re = unsafe { PatternObj::as_regex(p) };
    assert!(re.is_match("12345"));
    assert!(!re.is_match("abc"));
    drop_value(p);
}
