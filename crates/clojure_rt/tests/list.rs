//! Integration tests for the seq abstraction + PersistentList
//! (EmptyList + PersistentList). Exercises every protocol the type
//! implements.

use clojure_rt::types::list::empty_list;
use clojure_rt::{drop_value, exception, init, rt, Value};

// ---- construction + basic shape -------------------------------------------

#[test]
fn empty_list_is_empty() {
    init();
    let e = empty_list();
    assert_eq!(rt::count(e).as_int(), Some(0));
    drop_value(e);
}

#[test]
fn list_constructs_from_slice() {
    init();
    let items = [Value::int(1), Value::int(2), Value::int(3)];
    let l = rt::list(&items);
    assert_eq!(rt::count(l).as_int(), Some(3));
    drop_value(l);
}

// ---- ISeq/ISeqable/INext ---------------------------------------------------

#[test]
fn first_of_list_is_head() {
    init();
    let l = rt::list(&[Value::int(1), Value::int(2)]);
    assert_eq!(rt::first(l).as_int(), Some(1));
    drop_value(l);
}

#[test]
fn rest_of_list_is_tail() {
    init();
    let l = rt::list(&[Value::int(1), Value::int(2), Value::int(3)]);
    let r = rt::rest(l);
    assert_eq!(rt::count(r).as_int(), Some(2));
    assert_eq!(rt::first(r).as_int(), Some(2));
    drop_value(r);
    drop_value(l);
}

#[test]
fn rest_of_singleton_is_empty_list() {
    init();
    let l = rt::list(&[Value::int(1)]);
    let r = rt::rest(l);
    assert_eq!(rt::count(r).as_int(), Some(0));
    // It's the empty list, not nil.
    assert!(!r.is_nil());
    drop_value(r);
    drop_value(l);
}

#[test]
fn next_of_singleton_is_nil() {
    init();
    let l = rt::list(&[Value::int(1)]);
    let n = rt::next(l);
    assert!(n.is_nil());
    drop_value(l);
}

#[test]
fn next_of_two_element_list_is_one_element_list() {
    init();
    let l = rt::list(&[Value::int(1), Value::int(2)]);
    let n = rt::next(l);
    assert!(!n.is_nil());
    assert_eq!(rt::count(n).as_int(), Some(1));
    drop_value(n);
    drop_value(l);
}

#[test]
fn seq_of_empty_list_is_nil() {
    init();
    let e = empty_list();
    assert!(rt::seq(e).is_nil());
    drop_value(e);
}

#[test]
fn seq_of_nonempty_list_is_self() {
    init();
    let l = rt::list(&[Value::int(1)]);
    let s = rt::seq(l);
    // Same heap address — same Value.
    assert_eq!(s.payload, l.payload);
    drop_value(s);
    drop_value(l);
}

// ---- nil-as-empty-seq ------------------------------------------------------

#[test]
fn first_nil_is_nil() {
    init();
    assert!(rt::first(Value::NIL).is_nil());
}

#[test]
fn rest_nil_is_empty_list() {
    init();
    let r = rt::rest(Value::NIL);
    assert!(!r.is_nil());
    assert_eq!(rt::count(r).as_int(), Some(0));
    drop_value(r);
}

#[test]
fn next_nil_is_nil() {
    init();
    assert!(rt::next(Value::NIL).is_nil());
}

#[test]
fn seq_nil_is_nil() {
    init();
    assert!(rt::seq(Value::NIL).is_nil());
}

// ---- ICollection / IEmptyableCollection ------------------------------------

#[test]
fn conj_prepends() {
    init();
    let l = rt::list(&[Value::int(2), Value::int(3)]);
    let l2 = rt::conj(l, Value::int(1));
    assert_eq!(rt::count(l2).as_int(), Some(3));
    assert_eq!(rt::first(l2).as_int(), Some(1));
    drop_value(l2);
    drop_value(l);
}

#[test]
fn empty_returns_empty_list() {
    init();
    let l = rt::list(&[Value::int(1), Value::int(2)]);
    let e = rt::empty(l);
    assert_eq!(rt::count(e).as_int(), Some(0));
    drop_value(e);
    drop_value(l);
}

// ---- IStack ----------------------------------------------------------------

#[test]
fn peek_returns_first() {
    init();
    let l = rt::list(&[Value::int(1), Value::int(2)]);
    assert_eq!(rt::peek(l).as_int(), Some(1));
    drop_value(l);
}

#[test]
fn pop_returns_rest() {
    init();
    let l = rt::list(&[Value::int(1), Value::int(2), Value::int(3)]);
    let p = rt::pop(l);
    assert_eq!(rt::count(p).as_int(), Some(2));
    assert_eq!(rt::first(p).as_int(), Some(2));
    drop_value(p);
    drop_value(l);
}

#[test]
fn pop_empty_returns_exception() {
    init();
    let e = empty_list();
    let r = rt::pop(e);
    assert!(r.is_exception());
    let msg = exception::message(r).expect("exception payload");
    assert!(msg.contains("empty"), "message should mention empty: {msg}");
    drop_value(r);
    drop_value(e);
}

#[test]
fn peek_empty_is_nil() {
    init();
    let e = empty_list();
    assert!(rt::peek(e).is_nil());
    drop_value(e);
}

// ---- IHash / IEquiv contract ----------------------------------------------

#[test]
fn equal_lists_are_equiv_and_hash_equal() {
    init();
    let a = rt::list(&[Value::int(1), Value::int(2), Value::int(3)]);
    let b = rt::list(&[Value::int(1), Value::int(2), Value::int(3)]);
    assert_eq!(rt::equiv(a, b).as_bool(), Some(true));
    assert_eq!(rt::hash(a).as_int(), rt::hash(b).as_int());
    drop_value(a);
    drop_value(b);
}

#[test]
fn different_lists_are_not_equiv() {
    init();
    let a = rt::list(&[Value::int(1), Value::int(2)]);
    let b = rt::list(&[Value::int(1), Value::int(3)]);
    assert_eq!(rt::equiv(a, b).as_bool(), Some(false));
    drop_value(a);
    drop_value(b);
}

#[test]
fn empty_lists_are_equiv() {
    init();
    let a = empty_list();
    let b = empty_list();
    assert_eq!(rt::equiv(a, b).as_bool(), Some(true));
    drop_value(a);
    drop_value(b);
}

#[test]
fn empty_list_not_equiv_to_singleton() {
    init();
    let e = empty_list();
    let l = rt::list(&[Value::int(1)]);
    assert_eq!(rt::equiv(e, l).as_bool(), Some(false));
    drop_value(e);
    drop_value(l);
}

// ---- IMeta / IWithMeta -----------------------------------------------------

#[test]
fn fresh_list_meta_is_nil() {
    init();
    let l = rt::list(&[Value::int(1)]);
    assert!(rt::meta(l).is_nil());
    drop_value(l);
}

#[test]
fn with_meta_replaces_meta_on_cons() {
    init();
    let l = rt::list(&[Value::int(1), Value::int(2)]);
    let m = rt::str_new("source-form-info");
    let l2 = rt::with_meta(l, m);

    let read = rt::meta(l2);
    assert_eq!(rt::equiv(read, m).as_bool(), Some(true));
    drop_value(read);

    // Equal value semantics — meta doesn't break equiv.
    assert_eq!(rt::equiv(l, l2).as_bool(), Some(true));
    assert_eq!(rt::hash(l).as_int(), rt::hash(l2).as_int());

    drop_value(m);
    drop_value(l);
    drop_value(l2);
}

#[test]
fn with_meta_on_empty_list_allocates_fresh() {
    init();
    let e = empty_list();
    let m = rt::str_new("meta");
    let e2 = rt::with_meta(e, m);

    // Different heap pointers — fresh allocation.
    assert_ne!(e.payload, e2.payload);
    // Still equal in value sense.
    assert_eq!(rt::equiv(e, e2).as_bool(), Some(true));

    drop_value(m);
    drop_value(e);
    drop_value(e2);
}

// ---- ISequential ----------------------------------------------------------

#[test]
fn empty_list_is_sequential() {
    init();
    assert!(rt::sequential(empty_list()));
}

#[test]
fn cons_is_sequential() {
    init();
    let l = rt::list(&[Value::int(1)]);
    assert!(rt::sequential(l));
    drop_value(l);
}

#[test]
fn nil_is_not_sequential() {
    init();
    assert!(!rt::sequential(Value::NIL));
}

#[test]
fn int_is_not_sequential() {
    init();
    assert!(!rt::sequential(Value::int(7)));
}

#[test]
fn string_is_not_sequential() {
    init();
    let s = rt::str_new("hello");
    assert!(!rt::sequential(s));
    drop_value(s);
}

// ---- cons fn (rt::cons does seq coercion) ---------------------------------

#[test]
fn cons_onto_nil_yields_singleton() {
    init();
    let l = rt::cons(Value::int(1), Value::NIL);
    assert_eq!(rt::count(l).as_int(), Some(1));
    assert_eq!(rt::first(l).as_int(), Some(1));
    drop_value(l);
}

#[test]
fn cons_onto_list_prepends() {
    init();
    let tail = rt::list(&[Value::int(2), Value::int(3)]);
    let l = rt::cons(Value::int(1), tail);
    assert_eq!(rt::count(l).as_int(), Some(3));
    drop_value(l);
    drop_value(tail);
}
