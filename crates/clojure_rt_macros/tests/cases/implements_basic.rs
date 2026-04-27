use clojure_rt::{register_type, protocol, implements, Value};

protocol! { pub trait P { fn m(this: Value) -> Value; } }
register_type! { pub struct T { x: Value } }
implements! { impl P for T { fn m(this: Value) -> Value { let _ = this; Value::NIL } } }

fn main() {}
