//! Verifies the multi-arity protocol path: a protocol declares two
//! arities of the same stem (`pick_2`/`pick_3`); a type implements both;
//! call sites use the un-suffixed stem and the macro routes to the right
//! slot based on the literal slice length.

use clojure_rt::{init, register_type, protocol, implements, Value};

protocol! {
    pub trait IPicker {
        fn pick_2(this: Value, k: Value) -> Value;
        fn pick_3(this: Value, k: Value, fallback: Value) -> Value;
    }
}

register_type! {
    pub struct Box1 { tag: Value }
}

implements! {
    impl IPicker for Box1 {
        fn pick_2(this: Value, k: Value) -> Value {
            let _ = this;
            // Echo the key — proves arity-2 reaches its own slot.
            k
        }
        fn pick_3(this: Value, k: Value, fallback: Value) -> Value {
            let _ = (this, k);
            // Return the fallback — proves arity-3 reaches its own slot.
            fallback
        }
    }
}

#[test]
fn dispatch_routes_to_arity_2_slot() {
    init();
    let b = Box1::alloc(Value::int(0));
    let r = clojure_rt_macros::dispatch!(IPicker::pick, &[b, Value::int(99)]);
    assert_eq!(r.as_int(), Some(99));
    clojure_rt::drop_value(b);
}

#[test]
fn dispatch_routes_to_arity_3_slot() {
    init();
    let b = Box1::alloc(Value::int(0));
    let r = clojure_rt_macros::dispatch!(IPicker::pick, &[b, Value::int(7), Value::int(42)]);
    assert_eq!(r.as_int(), Some(42));
    clojure_rt::drop_value(b);
}

#[test]
fn explicit_suffix_at_dispatch_site_also_works() {
    init();
    let b = Box1::alloc(Value::int(0));
    // Calling the suffixed name explicitly must hit the same slot.
    let r2 = clojure_rt_macros::dispatch!(IPicker::pick_2, &[b, Value::int(11)]);
    assert_eq!(r2.as_int(), Some(11));
    let r3 = clojure_rt_macros::dispatch!(IPicker::pick_3, &[b, Value::int(0), Value::int(33)]);
    assert_eq!(r3.as_int(), Some(33));
    clojure_rt::drop_value(b);
}
