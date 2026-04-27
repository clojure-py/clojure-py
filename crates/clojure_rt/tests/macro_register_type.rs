use clojure_rt::{init, Value, register_type};

register_type! {
    pub struct MacroSmoke {
        head: Value,
        tail: Value,
    }
}

#[test]
fn register_type_macro_alloc_and_drop() {
    init();
    let v = MacroSmoke::alloc(Value::int(1), Value::int(2));
    assert!(v.is_heap());
    clojure_rt::drop_value(v);
}
