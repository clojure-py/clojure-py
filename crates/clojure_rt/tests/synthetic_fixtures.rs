//! Reusable synthetic protocols + types for end-to-end and benchmark tests.
//! Declared in a single test crate file because Rust integration tests
//! don't share modules; benches that want these fixtures should `include!`
//! this file or duplicate the relevant pieces.

#![allow(dead_code)]

use clojure_rt::{register_type, protocol, implements, Value};

protocol! {
    pub trait Greeter {
        fn greet(this: Value) -> Value;
    }
}

protocol! {
    pub trait CounterP {
        fn count(this: Value) -> Value;
    }
}

register_type! { pub struct Foo { tag: Value } }
register_type! { pub struct Bar { tag: Value } }

implements! { impl Greeter for Foo { fn greet(this: Value) -> Value { let _ = this; Value::int(100) } } }
implements! { impl Greeter for Bar { fn greet(this: Value) -> Value { let _ = this; Value::int(200) } } }
implements! { impl CounterP for Foo { fn count(this: Value) -> Value { let _ = this; Value::int(1) } } }
implements! { impl CounterP for Bar { fn count(this: Value) -> Value { let _ = this; Value::int(2) } } }

#[test]
fn synthetic_fixtures_compile_and_dispatch() {
    use clojure_rt::{init, dispatch};
    init();
    let f = Foo::alloc(Value::NIL);
    let b = Bar::alloc(Value::NIL);
    assert_eq!(dispatch!(Greeter::greet, &[f]).as_int(), Some(100));
    assert_eq!(dispatch!(Greeter::greet, &[b]).as_int(), Some(200));
    assert_eq!(dispatch!(CounterP::count, &[f]).as_int(), Some(1));
    assert_eq!(dispatch!(CounterP::count, &[b]).as_int(), Some(2));
    clojure_rt::drop_value(f);
    clojure_rt::drop_value(b);
}
