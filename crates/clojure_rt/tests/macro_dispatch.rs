use clojure_rt::{init, register_type, protocol, implements, dispatch, Value};

protocol! {
    pub trait Counter {
        fn count(this: Value) -> Value;
    }
}

register_type! { pub struct DispFoo { _p: Value } }
register_type! { pub struct DispBar { _p: Value } }

implements! { impl Counter for DispFoo { fn count(this: Value) -> Value { let _ = this; Value::int(1) } } }
implements! { impl Counter for DispBar { fn count(this: Value) -> Value { let _ = this; Value::int(2) } } }

#[test]
fn dispatch_macro_resolves_per_type() {
    init();
    let foo = DispFoo::alloc(Value::NIL);
    let bar = DispBar::alloc(Value::NIL);
    assert_eq!(dispatch!(Counter::count, &[foo]).as_int(), Some(1));
    assert_eq!(dispatch!(Counter::count, &[bar]).as_int(), Some(2));
    // Run again to exercise IC hits.
    assert_eq!(dispatch!(Counter::count, &[foo]).as_int(), Some(1));
    assert_eq!(dispatch!(Counter::count, &[foo]).as_int(), Some(1));
    clojure_rt::drop_value(foo);
    clojure_rt::drop_value(bar);
}
