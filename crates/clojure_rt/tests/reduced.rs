//! `Reduced` wrap/unwrap + `IDeref` plumbing.

use clojure_rt::{drop_value, init, rt, Value};

#[test]
fn reduced_wraps_and_is_detected() {
    init();
    let r = rt::reduced(Value::int(7));
    assert!(rt::is_reduced(r));
    drop_value(r);
}

#[test]
fn unreduced_unwraps_a_reduced() {
    init();
    let r = rt::reduced(Value::int(7));
    let v = rt::unreduced(r);
    assert_eq!(v.as_int(), Some(7));
    // unreduced consumed `r` and returned a fresh value-only Value.
    drop_value(v);
}

#[test]
fn unreduced_passes_through_non_reduced() {
    init();
    let v = rt::unreduced(Value::int(42));
    assert_eq!(v.as_int(), Some(42));
}

#[test]
fn deref_protocol_returns_inner() {
    init();
    let r = rt::reduced(Value::int(99));
    let v = rt::deref(r);
    assert_eq!(v.as_int(), Some(99));
    drop_value(v);
    drop_value(r);
}

#[test]
fn is_reduced_false_for_primitives_and_other_heap_types() {
    init();
    assert!(!rt::is_reduced(Value::int(0)));
    assert!(!rt::is_reduced(Value::NIL));
    let v = rt::vector(&[Value::int(1)]);
    assert!(!rt::is_reduced(v));
    drop_value(v);
}
