use clojure_rt::{register_type, Value};

register_type! {
    pub struct Smoke { head: Value, tail: Value }
}

fn main() {
    let _id_cell = &SMOKE_TYPE_ID;
}
