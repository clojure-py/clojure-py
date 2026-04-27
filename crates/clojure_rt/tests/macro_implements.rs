use clojure_rt::{init, register_type, protocol, implements, Value};

protocol! {
    pub trait Greeter {
        fn greet(this: Value) -> Value;
    }
}

register_type! {
    pub struct ImplFoo { _placeholder: Value }
}

implements! {
    impl Greeter for ImplFoo {
        fn greet(this: Value) -> Value {
            let _ = this;
            Value::int(42)
        }
    }
}

#[test]
fn implements_wires_method_into_type_table() {
    init();
    let foo = ImplFoo::alloc(Value::NIL);

    // Use slow path to verify the table is populated.
    use clojure_rt::dispatch::dispatch_fn;
    use clojure_rt::dispatch::ic::ICSlot;
    static IC: ICSlot = ICSlot::EMPTY;
    let r = dispatch_fn(&IC, &Greeter::GREET, &[foo]);
    assert_eq!(r.as_int(), Some(42));

    clojure_rt::drop_value(foo);
}
