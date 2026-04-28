//! `TransientVector` — round-trip + invalidation + batch-conj!.

use clojure_rt::{drop_value, init, rt, Value};

fn ints(xs: &[i64]) -> Vec<Value> { xs.iter().map(|&n| Value::int(n)).collect() }
fn drop_all(vs: &[Value]) { for &v in vs { drop_value(v); } }

#[test]
fn empty_round_trip_via_transient() {
    init();
    let v = rt::vector(&[]);
    let t = rt::transient(v);
    let p = rt::persistent_(t);
    drop_value(t);
    assert!(rt::equiv(v, p).as_bool().unwrap_or(false));
    drop_all(&[v, p]);
}

#[test]
fn conj_bang_then_persistent_matches_iterative_conj() {
    init();
    let mut t = rt::transient(rt::vector(&[]));
    for i in 0..50i64 {
        let nt = rt::conj_bang(t, Value::int(i));
        drop_value(t);
        t = nt;
    }
    let v = rt::persistent_(t);
    drop_value(t);

    let expected = rt::vector(&ints(&(0..50).collect::<Vec<_>>()));
    assert!(rt::equiv(v, expected).as_bool().unwrap_or(false));
    drop_all(&[v, expected]);
}

#[test]
fn conj_bang_promotes_tail_into_trie_correctly() {
    // 100 elements forces multiple tail promotions.
    init();
    let mut t = rt::transient(rt::vector(&[]));
    for i in 0..100i64 {
        let nt = rt::conj_bang(t, Value::int(i));
        drop_value(t);
        t = nt;
    }
    let v = rt::persistent_(t);
    drop_value(t);
    assert_eq!(rt::count(v).as_int(), Some(100));
    for i in 0..100i64 {
        let r = rt::nth(v, Value::int(i));
        assert_eq!(r.as_int(), Some(i), "nth({i})");
        drop_value(r);
    }
    drop_value(v);
}

#[test]
fn assoc_bang_replaces_in_tail() {
    init();
    let v = rt::vector(&ints(&[10, 20, 30]));
    let t = rt::transient(v);
    let t = rt::assoc_bang(t, Value::int(1), Value::int(99));
    let v2 = rt::persistent_(t);
    drop_value(t);
    let r = rt::nth(v2, Value::int(1));
    assert_eq!(r.as_int(), Some(99));
    drop_value(r);
    drop_all(&[v, v2]);
}

#[test]
fn assoc_bang_replaces_in_trie() {
    init();
    let xs = ints(&(0..100).collect::<Vec<_>>());
    let v = rt::vector(&xs);
    let t = rt::transient(v);
    let t = rt::assoc_bang(t, Value::int(42), Value::int(-1));
    let v2 = rt::persistent_(t);
    drop_value(t);
    let r = rt::nth(v2, Value::int(42));
    assert_eq!(r.as_int(), Some(-1));
    drop_value(r);
    drop_all(&[v, v2]);
}

#[test]
fn pop_bang_removes_last() {
    init();
    let v = rt::vector(&ints(&[1, 2, 3]));
    let t = rt::transient(v);
    let t = rt::pop_bang(t);
    let v2 = rt::persistent_(t);
    drop_value(t);
    assert_eq!(rt::count(v2).as_int(), Some(2));
    drop_all(&[v, v2]);
}

#[test]
fn pop_bang_promotes_trie_leaf_when_tail_empties() {
    init();
    // 33 elements: 1 leaf-block in trie + 1-element tail.
    let xs = ints(&(0..33).collect::<Vec<_>>());
    let v = rt::vector(&xs);
    let t = rt::transient(v);
    let t = rt::pop_bang(t);
    let v2 = rt::persistent_(t);
    drop_value(t);
    assert_eq!(rt::count(v2).as_int(), Some(32));
    drop_all(&[v, v2]);
}

#[test]
fn persistent_invalidates_further_mutation() {
    init();
    let t = rt::transient(rt::vector(&[]));
    let t = rt::conj_bang(t, Value::int(1));
    let _v = rt::persistent_(t);
    let r = rt::conj_bang(t, Value::int(2));
    assert!(r.is_exception());
    drop_value(r);
    drop_all(&[t, _v]);
}

#[test]
fn nth_during_transient_session() {
    init();
    let v = rt::vector(&ints(&[10, 20, 30]));
    let t = rt::transient(v);
    let r = rt::nth(t, Value::int(1));
    assert_eq!(r.as_int(), Some(20));
    drop_value(r);
    let _ = rt::persistent_(t);
    drop_all(&[t, v]);
}

#[test]
fn does_not_mutate_source_persistent() {
    init();
    let v = rt::vector(&ints(&[1, 2, 3]));
    let t = rt::transient(v);
    let t = rt::conj_bang(t, Value::int(99));
    let _v2 = rt::persistent_(t);
    // Original unchanged.
    assert_eq!(rt::count(v).as_int(), Some(3));
    drop_all(&[t, v, _v2]);
}

#[test]
fn many_assoc_bang_in_trie_share_no_extra_allocations() {
    // Build a 200-element vector. The first assoc! at a trie position
    // will path-copy that leaf-block via Arc::make_mut (the trie was
    // shared with the source persistent). Subsequent assoc!s at
    // *different* leaf-blocks each path-copy *that* leaf-block but
    // not the rest. Assoc!s at the *same* leaf-block reuse it
    // in-place. This test just verifies correctness over many
    // mutations across all trie regions; the in-place perf shape is
    // observed indirectly via no use-after-free / no dup-drop bugs
    // showing up under stress.
    init();
    let xs = ints(&(0..200).collect::<Vec<_>>());
    let v = rt::vector(&xs);
    let mut t = rt::transient(v);

    // Touch every index, replacing with i*10.
    for i in 0..200i64 {
        let nt = rt::assoc_bang(t, Value::int(i), Value::int(i * 10));
        drop_value(t);
        t = nt;
    }
    let v2 = rt::persistent_(t);
    drop_value(t);

    assert_eq!(rt::count(v2).as_int(), Some(200));
    for i in 0..200i64 {
        let r = rt::nth(v2, Value::int(i));
        assert_eq!(r.as_int(), Some(i * 10), "nth({i}) after batch assoc!");
        drop_value(r);
    }
    // Source untouched.
    for i in 0..200i64 {
        let r = rt::nth(v, Value::int(i));
        assert_eq!(r.as_int(), Some(i));
        drop_value(r);
    }
    drop_all(&[v, v2]);
}
