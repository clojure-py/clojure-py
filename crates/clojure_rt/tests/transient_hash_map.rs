//! `TransientHashMap` — round-trip + invalidation + batch-build
//! cross-checked against persistent op-by-op.

use clojure_rt::{drop_value, init, rt, Value};
use clojure_rt::types::hash_map::PersistentHashMap;

fn drop_all(vs: &[Value]) { for &v in vs { drop_value(v); } }

#[test]
fn empty_round_trip_via_transient() {
    init();
    let m = PersistentHashMap::from_kvs(&[]);
    let t = rt::transient(m);
    let p = rt::persistent_(t);
    drop_value(t);
    assert!(rt::equiv(m, p).as_bool().unwrap_or(false));
    drop_all(&[m, p]);
}

#[test]
fn assoc_bang_then_persistent_matches_persistent_assoc() {
    init();
    let ka = rt::keyword(None, "a");
    let kb = rt::keyword(None, "b");
    let t = rt::transient(PersistentHashMap::from_kvs(&[]));
    let t = rt::assoc_bang(t, ka, Value::int(1));
    let t = rt::assoc_bang(t, kb, Value::int(2));
    let m = rt::persistent_(t);
    drop_value(t);

    let expected = PersistentHashMap::from_kvs(&[ka, Value::int(1), kb, Value::int(2)]);
    assert!(rt::equiv(m, expected).as_bool().unwrap_or(false));
    drop_all(&[m, expected, ka, kb]);
}

#[test]
fn assoc_bang_replaces_existing() {
    init();
    let ka = rt::keyword(None, "a");
    let t = rt::transient(PersistentHashMap::from_kvs(&[ka, Value::int(1)]));
    let t = rt::assoc_bang(t, ka, Value::int(99));
    let m = rt::persistent_(t);
    drop_value(t);
    assert_eq!(rt::get(m, ka).as_int(), Some(99));
    assert_eq!(rt::count(m).as_int(), Some(1));
    drop_all(&[m, ka]);
}

#[test]
fn dissoc_bang_removes_key() {
    init();
    let ka = rt::keyword(None, "a");
    let kb = rt::keyword(None, "b");
    let t = rt::transient(PersistentHashMap::from_kvs(&[
        ka, Value::int(1), kb, Value::int(2),
    ]));
    let t = rt::dissoc_bang(t, ka);
    let m = rt::persistent_(t);
    drop_value(t);
    assert_eq!(rt::count(m).as_int(), Some(1));
    assert!(rt::get(m, ka).is_nil());
    assert_eq!(rt::get(m, kb).as_int(), Some(2));
    drop_all(&[m, ka, kb]);
}

#[test]
fn batch_assoc_bang_through_trie_levels() {
    init();
    let mut t = rt::transient(PersistentHashMap::from_kvs(&[]));
    for i in 0..200i64 {
        let nt = rt::assoc_bang(t, Value::int(i), Value::int(i * 10));
        drop_value(t);
        t = nt;
    }
    let m = rt::persistent_(t);
    drop_value(t);
    assert_eq!(rt::count(m).as_int(), Some(200));
    for i in 0..200i64 {
        let r = rt::get(m, Value::int(i));
        assert_eq!(r.as_int(), Some(i * 10), "lookup {i}");
        drop_value(r);
    }
    drop_value(m);
}

#[test]
fn persistent_invalidates_further_mutation() {
    init();
    let ka = rt::keyword(None, "a");
    let t = rt::transient(PersistentHashMap::from_kvs(&[]));
    let t = rt::assoc_bang(t, ka, Value::int(1));
    let _m = rt::persistent_(t);
    let r = rt::assoc_bang(t, ka, Value::int(2));
    assert!(r.is_exception(), "expected invalid-transient exception");
    drop_value(r);
    drop_all(&[t, _m, ka]);
}

#[test]
fn count_during_transient_session() {
    init();
    let ka = rt::keyword(None, "a");
    let kb = rt::keyword(None, "b");
    let t = rt::transient(PersistentHashMap::from_kvs(&[]));
    assert_eq!(rt::count(t).as_int(), Some(0));
    let t = rt::assoc_bang(t, ka, Value::int(1));
    assert_eq!(rt::count(t).as_int(), Some(1));
    let t = rt::assoc_bang(t, kb, Value::int(2));
    assert_eq!(rt::count(t).as_int(), Some(2));
    let _ = rt::persistent_(t);
    drop_all(&[t, ka, kb]);
}

#[test]
fn lookup_during_transient_session() {
    init();
    let ka = rt::keyword(None, "a");
    let t = rt::transient(PersistentHashMap::from_kvs(&[ka, Value::int(7)]));
    assert_eq!(rt::get(t, ka).as_int(), Some(7));
    let _ = rt::persistent_(t);
    drop_all(&[t, ka]);
}

#[test]
fn does_not_mutate_source_persistent() {
    init();
    let ka = rt::keyword(None, "a");
    let kb = rt::keyword(None, "b");
    let m = PersistentHashMap::from_kvs(&[ka, Value::int(1)]);
    let t = rt::transient(m);
    let t = rt::assoc_bang(t, kb, Value::int(2));
    let _new = rt::persistent_(t);
    // Original unchanged.
    assert_eq!(rt::count(m).as_int(), Some(1));
    assert!(rt::get(m, kb).is_nil());
    drop_all(&[t, m, _new, ka, kb]);
}

#[test]
fn dissoc_then_reassoc_shape_is_correct() {
    init();
    let mut t = rt::transient(PersistentHashMap::from_kvs(&[]));
    // assoc 50 entries.
    for i in 0..50i64 {
        let nt = rt::assoc_bang(t, Value::int(i), Value::int(i));
        drop_value(t);
        t = nt;
    }
    // dissoc every other.
    for i in (0..50i64).step_by(2) {
        let nt = rt::dissoc_bang(t, Value::int(i));
        drop_value(t);
        t = nt;
    }
    // reassoc the dissoc'd ones with new values.
    for i in (0..50i64).step_by(2) {
        let nt = rt::assoc_bang(t, Value::int(i), Value::int(i * 100));
        drop_value(t);
        t = nt;
    }
    let m = rt::persistent_(t);
    drop_value(t);
    assert_eq!(rt::count(m).as_int(), Some(50));
    for i in 0..50i64 {
        let r = rt::get(m, Value::int(i));
        let want = if i % 2 == 0 { i * 100 } else { i };
        assert_eq!(r.as_int(), Some(want), "lookup {i}");
        drop_value(r);
    }
    drop_value(m);
}
