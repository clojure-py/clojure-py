use clojure_rt::protocol;

protocol! {
    pub trait P {
        fn m(this: clojure_rt::Value) -> clojure_rt::Value;
    }
}

fn main() {
    let _id_cell = &P::M_1_METHOD_ID;
}
