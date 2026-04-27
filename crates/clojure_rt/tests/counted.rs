//! End-to-end tests for the first migrated Java interface, `ICounted`,
//! and (transitively) the throwable-Value error model that backs it.

use clojure_rt::{drop_value, init, register_type, implements, rt, Value};
use clojure_rt::protocols::counted::ICounted;

#[test]
fn count_of_nil_is_zero() {
    init();
    let v = rt::count(Value::NIL);
    assert_eq!(v.as_int(), Some(0));
}

register_type! { pub struct Bag { size: Value } }

implements! {
    impl ICounted for Bag {
        fn count(this: Value) -> Value {
            unsafe { Bag::body(this) }.size
        }
    }
}

#[test]
fn count_of_bag_returns_its_size() {
    init();
    let bag = Bag::alloc(Value::int(5));
    assert_eq!(rt::count(bag).as_int(), Some(5));
    drop_value(bag);
}

#[test]
fn count_of_unhandled_type_returns_exception_value() {
    init();
    let v = rt::count(Value::int(7));
    assert!(v.is_exception(), "expected throwable Value, got tag={}", v.tag);

    let msg = clojure_rt::exception::message(v).expect("exception payload missing");
    assert!(msg.contains("ICounted") && msg.contains("count"),
            "exception message should name protocol/method, got: {msg}");
    drop_value(v);
}

#[test]
fn satisfies_counted_for_nil_is_true() {
    init();
    assert!(clojure_rt::protocol::satisfies(&ICounted::COUNT_1, Value::NIL));
}

#[test]
fn satisfies_counted_for_int_is_false() {
    init();
    assert!(!clojure_rt::protocol::satisfies(&ICounted::COUNT_1, Value::int(7)));
}

#[test]
fn satisfies_counted_for_bag_is_true() {
    init();
    let bag = Bag::alloc(Value::int(0));
    assert!(clojure_rt::protocol::satisfies(&ICounted::COUNT_1, bag));
    drop_value(bag);
}
