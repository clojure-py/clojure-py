//! `PersistentArrayMap` — assoc / dissoc / lookup / contains-key /
//! find / conj / seq / equiv / hash / with-meta plus edge cases
//! (empty map, dup keys, key-by-IEquiv-not-identity, nil-value
//! distinction).

use clojure_rt::{drop_value, init, rt, Value};

fn drop_all(vs: &[Value]) { for &v in vs { drop_value(v); } }

#[test]
fn empty_array_map_count_is_zero() {
    init();
    let m = rt::array_map(&[]);
    assert_eq!(rt::count(m).as_int(), Some(0));
    drop_value(m);
}

#[test]
fn assoc_adds_a_new_entry() {
    init();
    let k = rt::keyword(None, "a");
    let m = rt::array_map(&[]);
    let m2 = rt::assoc(m, k, Value::int(1));
    assert_eq!(rt::count(m2).as_int(), Some(1));
    let r = rt::get(m2, k);
    assert_eq!(r.as_int(), Some(1));
    drop_all(&[r, m, m2, k]);
}

#[test]
fn assoc_replaces_existing_value_keeping_position() {
    init();
    let ka = rt::keyword(None, "a");
    let kb = rt::keyword(None, "b");
    let m = rt::array_map(&[ka, Value::int(1), kb, Value::int(2)]);
    let m2 = rt::assoc(m, ka, Value::int(99));
    assert_eq!(rt::count(m2).as_int(), Some(2));
    assert_eq!(rt::get(m2, ka).as_int(), Some(99));
    assert_eq!(rt::get(m2, kb).as_int(), Some(2));
    drop_all(&[m, m2, ka, kb]);
}

#[test]
fn dissoc_removes_an_existing_entry() {
    init();
    let ka = rt::keyword(None, "a");
    let kb = rt::keyword(None, "b");
    let m = rt::array_map(&[ka, Value::int(1), kb, Value::int(2)]);
    let m2 = rt::dissoc(m, ka);
    assert_eq!(rt::count(m2).as_int(), Some(1));
    assert!(rt::get(m2, ka).is_nil());
    assert_eq!(rt::get(m2, kb).as_int(), Some(2));
    drop_all(&[m, m2, ka, kb]);
}

#[test]
fn dissoc_missing_key_returns_equal_map() {
    init();
    let ka = rt::keyword(None, "a");
    let kc = rt::keyword(None, "c");
    let m = rt::array_map(&[ka, Value::int(1)]);
    let m2 = rt::dissoc(m, kc);
    assert!(rt::equiv(m, m2).as_bool().unwrap_or(false));
    drop_all(&[m, m2, ka, kc]);
}

#[test]
fn contains_key_uses_iequiv_not_identity() {
    init();
    // Two distinct keyword constructions for the same name share the
    // interned identity, so this is really "by-name == by-equiv". A
    // stronger test would use vector-keys; this exercises the basic
    // path.
    let k1 = rt::keyword(None, "x");
    let k2 = rt::keyword(None, "x");
    let m = rt::array_map(&[k1, Value::int(7)]);
    assert_eq!(rt::contains_key(m, k2).as_bool(), Some(true));
    drop_all(&[m, k1, k2]);
}

#[test]
fn lookup_default_returns_default_on_miss() {
    init();
    let ka = rt::keyword(None, "a");
    let kc = rt::keyword(None, "c");
    let m = rt::array_map(&[ka, Value::int(1)]);
    let dflt = Value::int(-1);
    let r = rt::get_default(m, kc, dflt);
    assert_eq!(r.as_int(), Some(-1));
    drop_all(&[m, ka, kc]);
}

#[test]
fn lookup_distinguishes_present_nil_from_missing() {
    init();
    let ka = rt::keyword(None, "a");
    let m = rt::array_map(&[ka, Value::NIL]);
    // Present-but-nil: `get` returns nil, `contains-key` returns true.
    assert!(rt::get(m, ka).is_nil());
    assert_eq!(rt::contains_key(m, ka).as_bool(), Some(true));
    // get-with-default: returns the *stored* nil, not the default.
    let dflt = Value::int(-1);
    assert!(rt::get_default(m, ka, dflt).is_nil());
    drop_all(&[m, ka]);
}

#[test]
fn find_returns_a_real_map_entry() {
    init();
    let ka = rt::keyword(None, "a");
    let m = rt::array_map(&[ka, Value::int(7)]);
    let e = rt::find(m, ka);
    let k = rt::key(e);
    let v = rt::val(e);
    assert!(rt::equiv(k, ka).as_bool().unwrap_or(false));
    assert_eq!(v.as_int(), Some(7));
    drop_all(&[k, v, e, m, ka]);
}

#[test]
fn find_on_missing_returns_nil() {
    init();
    let ka = rt::keyword(None, "a");
    let kc = rt::keyword(None, "c");
    let m = rt::array_map(&[ka, Value::int(7)]);
    assert!(rt::find(m, kc).is_nil());
    drop_all(&[m, ka, kc]);
}

#[test]
fn conj_map_entry_extends_the_map() {
    init();
    let ka = rt::keyword(None, "a");
    let m = rt::array_map(&[]);
    let e = clojure_rt::types::map_entry::MapEntry::new(ka, Value::int(7));
    let m2 = rt::conj(m, e);
    assert_eq!(rt::count(m2).as_int(), Some(1));
    assert_eq!(rt::get(m2, ka).as_int(), Some(7));
    drop_all(&[m, m2, e, ka]);
}

#[test]
fn conj_two_vector_acts_as_map_entry() {
    init();
    let ka = rt::keyword(None, "a");
    let m = rt::array_map(&[]);
    let pair = rt::vector(&[ka, Value::int(8)]);
    let m2 = rt::conj(m, pair);
    assert_eq!(rt::get(m2, ka).as_int(), Some(8));
    drop_all(&[m, m2, pair, ka]);
}

#[test]
fn from_kvs_collapses_duplicate_keys_last_wins() {
    init();
    let ka = rt::keyword(None, "a");
    let m = rt::array_map(&[ka, Value::int(1), ka, Value::int(2)]);
    assert_eq!(rt::count(m).as_int(), Some(1));
    assert_eq!(rt::get(m, ka).as_int(), Some(2));
    drop_all(&[m, ka]);
}

#[test]
fn equiv_same_kvs_different_insertion_order() {
    init();
    let ka = rt::keyword(None, "a");
    let kb = rt::keyword(None, "b");
    let m1 = rt::array_map(&[ka, Value::int(1), kb, Value::int(2)]);
    let m2 = rt::array_map(&[kb, Value::int(2), ka, Value::int(1)]);
    assert!(rt::equiv(m1, m2).as_bool().unwrap_or(false));
    drop_all(&[m1, m2, ka, kb]);
}

#[test]
fn hash_same_kvs_different_insertion_order() {
    init();
    let ka = rt::keyword(None, "a");
    let kb = rt::keyword(None, "b");
    let m1 = rt::array_map(&[ka, Value::int(1), kb, Value::int(2)]);
    let m2 = rt::array_map(&[kb, Value::int(2), ka, Value::int(1)]);
    assert_eq!(rt::hash(m1).as_int(), rt::hash(m2).as_int());
    drop_all(&[m1, m2, ka, kb]);
}

#[test]
fn equiv_differing_values_is_false() {
    init();
    let ka = rt::keyword(None, "a");
    let m1 = rt::array_map(&[ka, Value::int(1)]);
    let m2 = rt::array_map(&[ka, Value::int(2)]);
    assert_eq!(rt::equiv(m1, m2).as_bool(), Some(false));
    drop_all(&[m1, m2, ka]);
}

#[test]
fn seq_walks_in_insertion_order() {
    init();
    let ka = rt::keyword(None, "a");
    let kb = rt::keyword(None, "b");
    let kc = rt::keyword(None, "c");
    let m = rt::array_map(&[ka, Value::int(1), kb, Value::int(2), kc, Value::int(3)]);
    let mut s = rt::seq(m);
    let mut keys = vec![];
    while !s.is_nil() {
        let e = rt::first(s);
        let k = rt::key(e);
        let v = rt::val(e);
        keys.push((k.as_int(), v.as_int().unwrap()));
        let _ = k;
        drop_value(k); drop_value(v); drop_value(e);
        let n = rt::next(s);
        drop_value(s);
        s = n;
    }
    drop_value(s);
    // Keys are keywords (heap), so as_int returns None. Just check
    // values arrived in order.
    let vs: Vec<i64> = keys.iter().map(|(_, v)| *v).collect();
    assert_eq!(vs, vec![1, 2, 3]);
    drop_all(&[m, ka, kb, kc]);
}

#[test]
fn seq_of_empty_map_is_nil() {
    init();
    let m = rt::array_map(&[]);
    assert!(rt::seq(m).is_nil());
    drop_value(m);
}

#[test]
fn with_meta_preserves_contents_and_replaces_meta() {
    init();
    let ka = rt::keyword(None, "a");
    let m = rt::array_map(&[ka, Value::int(1)]);
    let meta = rt::array_map(&[rt::keyword(None, "tag"), Value::int(99)]);
    let m2 = rt::with_meta(m, meta);
    assert!(rt::equiv(m, m2).as_bool().unwrap_or(false));
    let m2_meta = rt::meta(m2);
    assert!(rt::equiv(meta, m2_meta).as_bool().unwrap_or(false));
    drop_all(&[m, m2, meta, m2_meta, ka]);
}

#[test]
fn empty_collection_returns_canonical_empty_map() {
    init();
    let ka = rt::keyword(None, "a");
    let m = rt::array_map(&[ka, Value::int(1)]);
    let e = rt::empty(m);
    assert_eq!(rt::count(e).as_int(), Some(0));
    drop_all(&[m, e, ka]);
}
