//! `TransientArrayMap` — round-trip + invalidation + idiomatic
//! batch-build.

use clojure_rt::{drop_value, init, rt, Value};

fn drop_all(vs: &[Value]) { for &v in vs { drop_value(v); } }

#[test]
fn empty_round_trip_via_transient() {
    init();
    let m = rt::array_map(&[]);
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
    let t = rt::transient(rt::array_map(&[]));
    let t1 = rt::assoc_bang(t, ka, Value::int(1));
    let t2 = rt::assoc_bang(t1, kb, Value::int(2));
    let m = rt::persistent_(t2);
    drop_value(t); drop_value(t1); drop_value(t2);

    let expected = rt::array_map(&[ka, Value::int(1), kb, Value::int(2)]);
    assert!(rt::equiv(m, expected).as_bool().unwrap_or(false));
    drop_all(&[m, expected, ka, kb]);
}

#[test]
fn assoc_bang_replaces_existing() {
    init();
    let ka = rt::keyword(None, "a");
    let t = rt::transient(rt::array_map(&[ka, Value::int(1)]));
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
    let t = rt::transient(rt::array_map(&[ka, Value::int(1), kb, Value::int(2)]));
    let t = rt::dissoc_bang(t, ka);
    let m = rt::persistent_(t);
    drop_value(t);
    assert_eq!(rt::count(m).as_int(), Some(1));
    assert!(rt::get(m, ka).is_nil());
    assert_eq!(rt::get(m, kb).as_int(), Some(2));
    drop_all(&[m, ka, kb]);
}

#[test]
fn persistent_invalidates_further_mutation() {
    init();
    let ka = rt::keyword(None, "a");
    let t = rt::transient(rt::array_map(&[]));
    let t = rt::assoc_bang(t, ka, Value::int(1));
    let _m = rt::persistent_(t);
    let r = rt::assoc_bang(t, ka, Value::int(2));
    assert!(r.is_exception(), "expected invalid-transient exception");
    drop_value(r);
    drop_all(&[t, _m, ka]);
}

#[test]
fn count_during_transient_session_works() {
    init();
    let ka = rt::keyword(None, "a");
    let kb = rt::keyword(None, "b");
    let t = rt::transient(rt::array_map(&[]));
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
    let t = rt::transient(rt::array_map(&[ka, Value::int(7)]));
    assert_eq!(rt::get(t, ka).as_int(), Some(7));
    let _ = rt::persistent_(t);
    drop_all(&[t, ka]);
}
