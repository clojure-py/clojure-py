//! Unit tests for `PersistentVector`. Cross-references with vanilla
//! Clojure's behavior on the same operations. Property-based fuzz vs.
//! a `Vec<Value>` reference oracle lives in `proptest_vector.rs`.

use clojure_rt::{drop_value, init, rt, Value};

fn ints(xs: &[i64]) -> Vec<Value> { xs.iter().map(|&n| Value::int(n)).collect() }

fn drop_all(vs: &[Value]) { for &v in vs { drop_value(v); } }

#[test]
fn empty_vector_count_is_zero() {
    init();
    let v = rt::vector(&[]);
    assert_eq!(rt::count(v).as_int(), Some(0));
    drop_value(v);
}

#[test]
fn single_element_count_and_nth() {
    init();
    let v = rt::vector(&ints(&[42]));
    assert_eq!(rt::count(v).as_int(), Some(1));
    let r = rt::nth(v, Value::int(0));
    assert_eq!(r.as_int(), Some(42));
    drop_value(r);
    drop_value(v);
}

#[test]
fn nth_out_of_bounds_throws() {
    init();
    let v = rt::vector(&ints(&[1, 2, 3]));
    let r = rt::nth(v, Value::int(5));
    assert!(r.is_exception());
    drop_value(r);
    drop_value(v);
}

#[test]
fn nth_default_returns_default_for_oob() {
    init();
    let v = rt::vector(&ints(&[1, 2, 3]));
    let dflt = Value::int(-1);
    let r = rt::nth_default(v, Value::int(99), dflt);
    assert_eq!(r.as_int(), Some(-1));
    drop_value(v);
}

#[test]
fn cons_grows_into_trie_past_branching_factor() {
    init();
    // 100 elements forces multiple full tail promotions.
    let xs = ints(&(0..100).collect::<Vec<_>>());
    let v = rt::vector(&xs);
    assert_eq!(rt::count(v).as_int(), Some(100));
    for i in 0..100i64 {
        let r = rt::nth(v, Value::int(i));
        assert_eq!(r.as_int(), Some(i), "nth({i})");
        drop_value(r);
    }
    drop_value(v);
}

#[test]
fn cons_grows_through_root_growth() {
    // 2049 forces shift = 10 (two trie levels above the leaf-blocks),
    // exercising new_path + push_tail + root growth at least twice.
    init();
    let xs: Vec<i64> = (0..2049).collect();
    let v = rt::vector(&ints(&xs));
    assert_eq!(rt::count(v).as_int(), Some(2049));
    let last = rt::nth(v, Value::int(2048));
    assert_eq!(last.as_int(), Some(2048));
    drop_value(last);
    let mid = rt::nth(v, Value::int(1024));
    assert_eq!(mid.as_int(), Some(1024));
    drop_value(mid);
    drop_value(v);
}

#[test]
fn assoc_replaces_in_tail() {
    init();
    let v = rt::vector(&ints(&[10, 20, 30]));
    let v2 = rt::assoc(v, Value::int(1), Value::int(99));
    let r = rt::nth(v2, Value::int(1));
    assert_eq!(r.as_int(), Some(99));
    drop_value(r);
    // Original unchanged.
    let r0 = rt::nth(v, Value::int(1));
    assert_eq!(r0.as_int(), Some(20));
    drop_value(r0);
    drop_all(&[v, v2]);
}

#[test]
fn assoc_replaces_in_trie() {
    init();
    let xs = ints(&(0..100).collect::<Vec<_>>());
    let v = rt::vector(&xs);
    let v2 = rt::assoc(v, Value::int(42), Value::int(-1));
    let r = rt::nth(v2, Value::int(42));
    assert_eq!(r.as_int(), Some(-1));
    drop_value(r);
    let r0 = rt::nth(v, Value::int(42));
    assert_eq!(r0.as_int(), Some(42));
    drop_value(r0);
    drop_all(&[v, v2]);
}

#[test]
fn assoc_at_count_extends() {
    init();
    let v = rt::vector(&ints(&[1, 2, 3]));
    let v2 = rt::assoc(v, Value::int(3), Value::int(4));
    assert_eq!(rt::count(v2).as_int(), Some(4));
    let r = rt::nth(v2, Value::int(3));
    assert_eq!(r.as_int(), Some(4));
    drop_value(r);
    drop_all(&[v, v2]);
}

#[test]
fn assoc_out_of_range_throws() {
    init();
    let v = rt::vector(&ints(&[1, 2, 3]));
    let r = rt::assoc(v, Value::int(99), Value::int(0));
    assert!(r.is_exception());
    drop_value(r);
    drop_value(v);
}

#[test]
fn pop_from_tail() {
    init();
    let v = rt::vector(&ints(&[1, 2, 3]));
    let v2 = rt::pop(v);
    assert_eq!(rt::count(v2).as_int(), Some(2));
    let last = rt::peek(v2);
    assert_eq!(last.as_int(), Some(2));
    drop_value(last);
    drop_all(&[v, v2]);
}

#[test]
fn pop_promotes_trie_leaf_to_tail() {
    init();
    // 33 elements: 1 leaf-block in trie + 1-element tail.
    let xs = ints(&(0..33).collect::<Vec<_>>());
    let v = rt::vector(&xs);
    let v2 = rt::pop(v);
    assert_eq!(rt::count(v2).as_int(), Some(32));
    let last = rt::peek(v2);
    assert_eq!(last.as_int(), Some(31));
    drop_value(last);
    drop_all(&[v, v2]);
}

#[test]
fn pop_empty_throws() {
    init();
    let v = rt::vector(&[]);
    let r = rt::pop(v);
    assert!(r.is_exception());
    drop_all(&[r, v]);
}

#[test]
fn peek_empty_returns_nil() {
    init();
    let v = rt::vector(&[]);
    assert!(rt::peek(v).is_nil());
    drop_value(v);
}

#[test]
fn equiv_same_contents_is_true() {
    init();
    let a = rt::vector(&ints(&[1, 2, 3]));
    let b = rt::vector(&ints(&[1, 2, 3]));
    assert_eq!(rt::equiv(a, b).as_bool(), Some(true));
    drop_all(&[a, b]);
}

#[test]
fn equiv_differing_lengths_is_false() {
    init();
    let a = rt::vector(&ints(&[1, 2]));
    let b = rt::vector(&ints(&[1, 2, 3]));
    assert_eq!(rt::equiv(a, b).as_bool(), Some(false));
    drop_all(&[a, b]);
}

#[test]
fn hash_is_deterministic_and_stable() {
    init();
    let a = rt::vector(&ints(&[1, 2, 3]));
    let b = rt::vector(&ints(&[1, 2, 3]));
    let ha = rt::hash(a).as_int().unwrap();
    let hb = rt::hash(b).as_int().unwrap();
    assert_eq!(ha, hb);
    // Cached path returns identical value.
    assert_eq!(rt::hash(a).as_int().unwrap(), ha);
    drop_all(&[a, b]);
}

#[test]
fn lookup_by_int_key_works() {
    init();
    let v = rt::vector(&ints(&[10, 20, 30]));
    assert_eq!(rt::get(v, Value::int(1)).as_int(), Some(20));
    let dflt = Value::int(-99);
    let r = rt::get_default(v, Value::int(99), dflt);
    assert_eq!(r.as_int(), Some(-99));
    drop_value(v);
}

#[test]
fn contains_key_int_in_range() {
    init();
    let v = rt::vector(&ints(&[10, 20, 30]));
    assert_eq!(rt::contains_key(v, Value::int(0)).as_bool(), Some(true));
    assert_eq!(rt::contains_key(v, Value::int(2)).as_bool(), Some(true));
    assert_eq!(rt::contains_key(v, Value::int(3)).as_bool(), Some(false));
    assert_eq!(rt::contains_key(v, Value::int(-1)).as_bool(), Some(false));
    drop_value(v);
}

#[test]
fn seq_walks_in_order() {
    init();
    let v = rt::vector(&ints(&[10, 20, 30]));
    let mut s = rt::seq(v);
    let mut collected = vec![];
    while !s.is_nil() {
        let f = rt::first(s);
        collected.push(f.as_int().unwrap());
        drop_value(f);
        let n = rt::next(s);
        drop_value(s);
        s = n;
    }
    drop_value(s);
    drop_value(v);
    assert_eq!(collected, vec![10, 20, 30]);
}

#[test]
fn seq_of_empty_is_nil() {
    init();
    let v = rt::vector(&[]);
    assert!(rt::seq(v).is_nil());
    drop_value(v);
}

#[test]
fn rseq_walks_in_reverse() {
    init();
    let v = rt::vector(&ints(&[10, 20, 30]));
    let mut s = rt::rseq(v);
    let mut collected = vec![];
    while !s.is_nil() {
        let f = rt::first(s);
        collected.push(f.as_int().unwrap());
        drop_value(f);
        let n = rt::next(s);
        drop_value(s);
        s = n;
    }
    drop_value(s);
    drop_value(v);
    assert_eq!(collected, vec![30, 20, 10]);
}

#[test]
fn rseq_empty_is_nil() {
    init();
    let v = rt::vector(&[]);
    assert!(rt::rseq(v).is_nil());
    drop_value(v);
}

#[test]
fn with_meta_replaces_metadata_and_preserves_contents() {
    init();
    let v = rt::vector(&ints(&[1, 2, 3]));
    let k = rt::keyword(None, "tag");
    let m = rt::vector(&[k, Value::int(7)]);
    let v2 = rt::with_meta(v, m);
    assert!(rt::equiv(v, v2).as_bool().unwrap_or(false));
    let m2 = rt::meta(v2);
    assert!(rt::equiv(m, m2).as_bool().unwrap_or(false));
    drop_all(&[v, v2, m, m2, k]);
}
