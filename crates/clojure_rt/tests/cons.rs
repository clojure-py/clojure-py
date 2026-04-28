//! `Cons` cell tests + `rt::cons` shape selection.

use clojure_rt::{drop_value, init, rt, Value};
use clojure_rt::types::cons::Cons;

fn drop_all(vs: &[Value]) { for &v in vs { drop_value(v); } }

#[test]
fn cons_first_and_rest() {
    init();
    let c = Cons::new(Value::int(1), Value::NIL);
    assert_eq!(rt::first(c).as_int(), Some(1));
    // rest of (1) is empty list, not nil — matches JVM Cons.more() semantics.
    let r = rt::rest(c);
    assert!(rt::seq(r).is_nil());
    drop_value(r);
    drop_value(c);
}

#[test]
fn cons_walks_two_elements() {
    init();
    let inner = Cons::new(Value::int(2), Value::NIL);
    let outer = Cons::new(Value::int(1), inner);
    drop_value(inner);

    let f = rt::first(outer);
    assert_eq!(f.as_int(), Some(1));
    drop_value(f);

    let r = rt::next(outer);
    let f2 = rt::first(r);
    assert_eq!(f2.as_int(), Some(2));
    drop_value(f2);

    let r2 = rt::next(r);
    assert!(r2.is_nil());
    drop_value(r);
    drop_value(outer);
}

#[test]
fn cons_onto_vector_via_rt_cons() {
    // rt::cons of an element onto a non-list seqable (vector) should
    // produce a Cons (not a PersistentList — that would mis-count).
    init();
    let v = rt::vector(&[Value::int(2), Value::int(3)]);
    let c = rt::cons(Value::int(1), v);
    let cons_id = clojure_rt::types::cons::CONS_TYPE_ID
        .get().copied().unwrap_or(0);
    assert_eq!(c.tag, cons_id, "rt::cons over vector should yield Cons");

    // Walk the seq.
    let mut collected: Vec<i64> = Vec::new();
    let mut cur = c;
    clojure_rt::rc::dup(cur);
    while !cur.is_nil() {
        let f = rt::first(cur);
        collected.push(f.as_int().unwrap());
        drop_value(f);
        let n = rt::next(cur);
        drop_value(cur);
        cur = n;
    }
    assert_eq!(collected, vec![1, 2, 3]);
    drop_value(c);
    drop_value(v);
}

#[test]
fn cons_onto_list_yields_list() {
    // rt::cons of an element onto a PersistentList should yield a
    // PersistentList (count tracked).
    init();
    let l = rt::list(&[Value::int(2), Value::int(3)]);
    let c = rt::cons(Value::int(1), l);
    let plist_id = clojure_rt::types::list::PERSISTENTLIST_TYPE_ID
        .get().copied().unwrap_or(0);
    assert_eq!(c.tag, plist_id);
    assert_eq!(rt::count(c).as_int(), Some(3));
    drop_all(&[c, l]);
}

#[test]
fn cons_onto_nil_yields_singleton_list() {
    init();
    let c = rt::cons(Value::int(1), Value::NIL);
    let plist_id = clojure_rt::types::list::PERSISTENTLIST_TYPE_ID
        .get().copied().unwrap_or(0);
    assert_eq!(c.tag, plist_id);
    assert_eq!(rt::count(c).as_int(), Some(1));
    drop_value(c);
}

#[test]
fn cons_hash_matches_equivalent_list() {
    // (hash (cons 1 (cons 2 (cons 3 nil)))) should == (hash '(1 2 3))
    // since both are sequential and yield the same elements.
    init();
    let c = Cons::new(Value::int(1),
        Cons::new(Value::int(2),
            Cons::new(Value::int(3), Value::NIL)));
    // Build the equivalent list via rt::cons (which uses PersistentList for nil tail).
    let l = rt::list(&[Value::int(1), Value::int(2), Value::int(3)]);
    // Hashes should match because both have the same element sequence.
    assert_eq!(rt::hash(c).as_int(), rt::hash(l).as_int());
    drop_all(&[c, l]);
}

#[test]
fn cons_equiv_with_same_shape() {
    init();
    let a = Cons::new(Value::int(1), Cons::new(Value::int(2), Value::NIL));
    let b = Cons::new(Value::int(1), Cons::new(Value::int(2), Value::NIL));
    assert!(rt::equiv(a, b).as_bool().unwrap_or(false));
    drop_all(&[a, b]);
}
