use clojure_rt::{init, protocol};

protocol! {
    pub trait Greeter {
        fn greet(this: Value) -> Value;
    }
}

#[test]
fn protocol_macro_registers_method() {
    init();
    let mid = *Greeter::GREET_METHOD_ID.get().expect("method id unset");
    assert!(mid >= 1);
    assert_eq!(Greeter::GREET.method_id, mid);
    assert!(Greeter::GREET.proto_id >= 1);
}
