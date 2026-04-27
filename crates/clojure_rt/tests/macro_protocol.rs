use clojure_rt::{init, protocol};

protocol! {
    pub trait Greeter {
        fn greet(this: Value) -> Value;
    }
}

#[test]
fn protocol_macro_registers_method() {
    init();
    let mid = *Greeter::GREET_1_METHOD_ID.get().expect("method id unset");
    assert!(mid >= 1);
    assert_eq!(Greeter::GREET_1.method_id.load(std::sync::atomic::Ordering::Relaxed), mid);
    assert!(Greeter::GREET_1.proto_id.load(std::sync::atomic::Ordering::Relaxed) >= 1);
}
