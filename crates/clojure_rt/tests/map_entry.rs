//! `MapEntry` — behaves as a 2-vector for nth/count/hash, exposes
//! key/val via IMapEntry, equivs with same-shape entries and 2-vecs.

use clojure_rt::{drop_value, init, rt, Value};

#[test]
fn key_and_val_extract_correctly() {
    init();
    let kw = rt::keyword(None, "tag");
    let e = clojure_rt::types::map_entry::MapEntry::new(kw, Value::int(42));
    let k = rt::key(e);
    let v = rt::val(e);
    assert!(rt::equiv(k, kw).as_bool().unwrap_or(false));
    assert_eq!(v.as_int(), Some(42));
    drop_value(k); drop_value(v); drop_value(e);
}

#[test]
fn map_entry_count_is_two() {
    init();
    let e = clojure_rt::types::map_entry::MapEntry::new(Value::int(1), Value::int(2));
    assert_eq!(rt::count(e).as_int(), Some(2));
    drop_value(e);
}

#[test]
fn map_entry_nth_routes_to_key_then_val() {
    init();
    let e = clojure_rt::types::map_entry::MapEntry::new(Value::int(7), Value::int(8));
    let n0 = rt::nth(e, Value::int(0));
    let n1 = rt::nth(e, Value::int(1));
    assert_eq!(n0.as_int(), Some(7));
    assert_eq!(n1.as_int(), Some(8));
    drop_value(n0); drop_value(n1); drop_value(e);
}

#[test]
fn map_entry_nth_oob_returns_default_or_throws() {
    init();
    let e = clojure_rt::types::map_entry::MapEntry::new(Value::int(7), Value::int(8));
    let r = rt::nth(e, Value::int(2));
    assert!(r.is_exception());
    drop_value(r);
    let dflt = Value::int(-1);
    let r2 = rt::nth_default(e, Value::int(2), dflt);
    assert_eq!(r2.as_int(), Some(-1));
    drop_value(e);
}

#[test]
fn map_entry_hash_matches_two_vector_hash() {
    init();
    let e = clojure_rt::types::map_entry::MapEntry::new(Value::int(7), Value::int(8));
    let v = rt::vector(&[Value::int(7), Value::int(8)]);
    assert_eq!(rt::hash(e).as_int(), rt::hash(v).as_int());
    drop_value(e); drop_value(v);
}

#[test]
fn map_entry_equiv_with_same_shape() {
    init();
    let a = clojure_rt::types::map_entry::MapEntry::new(Value::int(1), Value::int(2));
    let b = clojure_rt::types::map_entry::MapEntry::new(Value::int(1), Value::int(2));
    assert!(rt::equiv(a, b).as_bool().unwrap_or(false));
    drop_value(a); drop_value(b);
}

#[test]
fn map_entry_equiv_with_2_vector() {
    init();
    let e = clojure_rt::types::map_entry::MapEntry::new(Value::int(1), Value::int(2));
    let v = rt::vector(&[Value::int(1), Value::int(2)]);
    assert!(rt::equiv(e, v).as_bool().unwrap_or(false));
    drop_value(e); drop_value(v);
}

#[test]
fn map_entry_with_meta_preserves_kv() {
    init();
    let e = clojure_rt::types::map_entry::MapEntry::new(Value::int(1), Value::int(2));
    let m = rt::array_map(&[]);
    let e2 = rt::with_meta(e, m);
    assert!(rt::equiv(e, e2).as_bool().unwrap_or(false));
    let m2 = rt::meta(e2);
    assert!(rt::equiv(m, m2).as_bool().unwrap_or(false));
    drop_value(e); drop_value(e2); drop_value(m); drop_value(m2);
}
