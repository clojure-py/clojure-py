//! `PersistentHashSet` direct tests — round-trip, conj/disj,
//! contains, set-as-fn, equiv, hash, seq, with-meta.

use clojure_rt::{drop_value, init, rt, Value};
use clojure_rt::types::hash_set::{empty_hash_set, PersistentHashSet};

fn drop_all(vs: &[Value]) { for &v in vs { drop_value(v); } }

#[test]
fn empty_hash_set_count_zero() {
    init();
    let s = PersistentHashSet::from_items(&[]);
    assert_eq!(rt::count(s).as_int(), Some(0));
    drop_value(s);
}

#[test]
fn empty_hash_set_singleton_returns_empty() {
    init();
    let a = empty_hash_set();
    let b = empty_hash_set();
    assert_eq!(rt::count(a).as_int(), Some(0));
    assert!(rt::equiv(a, b).as_bool().unwrap_or(false));
    drop_all(&[a, b]);
}

#[test]
fn from_items_collapses_duplicates() {
    init();
    let s = PersistentHashSet::from_items(&[
        Value::int(1), Value::int(2), Value::int(1), Value::int(3), Value::int(2),
    ]);
    assert_eq!(rt::count(s).as_int(), Some(3));
    drop_value(s);
}

#[test]
fn conj_adds_element() {
    init();
    let s0 = PersistentHashSet::from_items(&[]);
    let s1 = rt::conj(s0, Value::int(1));
    let s2 = rt::conj(s1, Value::int(2));
    assert_eq!(rt::count(s2).as_int(), Some(2));
    // Original unchanged.
    assert_eq!(rt::count(s0).as_int(), Some(0));
    drop_all(&[s0, s1, s2]);
}

#[test]
fn conj_existing_is_identity_in_count() {
    init();
    let s = PersistentHashSet::from_items(&[Value::int(1), Value::int(2)]);
    let s2 = rt::conj(s, Value::int(1));
    assert_eq!(rt::count(s2).as_int(), Some(2));
    drop_all(&[s, s2]);
}

#[test]
fn disj_removes_existing() {
    init();
    let s = PersistentHashSet::from_items(&[Value::int(1), Value::int(2), Value::int(3)]);
    let s2 = rt::disj(s, Value::int(2));
    assert_eq!(rt::count(s2).as_int(), Some(2));
    assert_eq!(rt::contains_key(s2, Value::int(2)).as_bool(), Some(false));
    assert_eq!(rt::contains_key(s2, Value::int(1)).as_bool(), Some(true));
    drop_all(&[s, s2]);
}

#[test]
fn disj_missing_is_no_op() {
    init();
    let s = PersistentHashSet::from_items(&[Value::int(1), Value::int(2)]);
    let s2 = rt::disj(s, Value::int(99));
    assert_eq!(rt::count(s2).as_int(), Some(2));
    drop_all(&[s, s2]);
}

#[test]
fn contains_via_lookup() {
    init();
    let s = PersistentHashSet::from_items(&[Value::int(1), Value::int(2)]);
    // `contains?` (associative path) on a set delegates to the underlying map.
    assert_eq!(rt::contains_key(s, Value::int(1)).as_bool(), Some(true));
    assert_eq!(rt::contains_key(s, Value::int(99)).as_bool(), Some(false));
    // `(get s x)` returns x if present, nil otherwise.
    assert_eq!(rt::get(s, Value::int(1)).as_int(), Some(1));
    assert!(rt::get(s, Value::int(99)).is_nil());
    let dflt = Value::int(-1);
    assert_eq!(rt::get_default(s, Value::int(99), dflt).as_int(), Some(-1));
    drop_value(s);
}

#[test]
fn set_as_fn_returns_member_or_nil() {
    init();
    let s = PersistentHashSet::from_items(&[Value::int(1), Value::int(2)]);
    let r1 = rt::invoke(s, &[Value::int(1)]);
    assert_eq!(r1.as_int(), Some(1));
    let r2 = rt::invoke(s, &[Value::int(99)]);
    assert!(r2.is_nil());
    let dflt = Value::int(-1);
    let r3 = rt::invoke(s, &[Value::int(99), dflt]);
    assert_eq!(r3.as_int(), Some(-1));
    drop_all(&[r1, r2, r3, s]);
}

#[test]
fn equiv_same_elements_different_order() {
    init();
    let a = PersistentHashSet::from_items(&[Value::int(1), Value::int(2), Value::int(3)]);
    let b = PersistentHashSet::from_items(&[Value::int(3), Value::int(1), Value::int(2)]);
    assert!(rt::equiv(a, b).as_bool().unwrap_or(false));
    drop_all(&[a, b]);
}

#[test]
fn equiv_different_sets_false() {
    init();
    let a = PersistentHashSet::from_items(&[Value::int(1), Value::int(2)]);
    let b = PersistentHashSet::from_items(&[Value::int(1), Value::int(3)]);
    assert_eq!(rt::equiv(a, b).as_bool(), Some(false));
    drop_all(&[a, b]);
}

#[test]
fn hash_same_for_equal_sets() {
    init();
    let a = PersistentHashSet::from_items(&[Value::int(1), Value::int(2), Value::int(3)]);
    let b = PersistentHashSet::from_items(&[Value::int(3), Value::int(2), Value::int(1)]);
    assert_eq!(rt::hash(a).as_int(), rt::hash(b).as_int());
    drop_all(&[a, b]);
}

#[test]
fn seq_walks_all_elements() {
    init();
    let mut s = PersistentHashSet::from_items(&[]);
    for i in 0..50i64 {
        let n = rt::conj(s, Value::int(i));
        drop_value(s);
        s = n;
    }
    let mut cur = rt::seq(s);
    let mut collected: Vec<i64> = Vec::new();
    while !cur.is_nil() {
        let x = rt::first(cur);
        collected.push(x.as_int().unwrap());
        drop_value(x);
        let n = rt::next(cur);
        drop_value(cur);
        cur = n;
    }
    drop_value(cur);
    drop_value(s);
    collected.sort();
    let expected: Vec<i64> = (0..50).collect();
    assert_eq!(collected, expected);
}

#[test]
fn empty_set_seq_is_nil() {
    init();
    let s = PersistentHashSet::from_items(&[]);
    let q = rt::seq(s);
    assert!(q.is_nil());
    drop_value(s);
}

#[test]
fn empty_collection_returns_canonical_empty_set() {
    init();
    let s = PersistentHashSet::from_items(&[Value::int(1), Value::int(2)]);
    let e = rt::empty(s);
    assert_eq!(rt::count(e).as_int(), Some(0));
    drop_all(&[s, e]);
}

#[test]
fn with_meta_preserves_elements() {
    init();
    let s = PersistentHashSet::from_items(&[Value::int(1), Value::int(2)]);
    let meta = rt::array_map(&[rt::keyword(None, "tag"), Value::int(99)]);
    let s2 = rt::with_meta(s, meta);
    assert!(rt::equiv(s, s2).as_bool().unwrap_or(false));
    drop_all(&[s, s2, meta]);
}

#[test]
fn rt_hash_set_helper() {
    init();
    let ka = rt::keyword(None, "a");
    let kb = rt::keyword(None, "b");
    let s = rt::hash_set(&[ka, kb, ka]);
    assert_eq!(rt::count(s).as_int(), Some(2));
    assert_eq!(rt::contains_key(s, ka).as_bool(), Some(true));
    assert_eq!(rt::contains_key(s, kb).as_bool(), Some(true));
    drop_all(&[s, ka, kb]);
}

#[test]
fn many_entries_drives_underlying_trie() {
    init();
    // 200 distinct integer items force the underlying HAMT through
    // multiple levels — exercises the seq walker against a deep trie.
    let mut s = PersistentHashSet::from_items(&[]);
    for i in 0..200i64 {
        let n = rt::conj(s, Value::int(i));
        drop_value(s);
        s = n;
    }
    assert_eq!(rt::count(s).as_int(), Some(200));
    for i in 0..200i64 {
        assert_eq!(rt::contains_key(s, Value::int(i)).as_bool(), Some(true), "missing {i}");
    }
    drop_value(s);
}
