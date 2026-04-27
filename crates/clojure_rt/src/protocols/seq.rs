//! Seq abstraction protocols — `ISeqable`, `ISeq`, `INext`. Plus nil
//! impls for all three (cljs convention: `(extend-type nil ...)` for
//! ISeq is canonical).

use crate::primitives::*;
use crate::value::Value;

clojure_rt_macros::protocol! {
    pub trait ISeqable {
        fn seq(this: ::clojure_rt::Value) -> ::clojure_rt::Value;
    }
}

clojure_rt_macros::protocol! {
    pub trait ISeq {
        fn first(this: ::clojure_rt::Value) -> ::clojure_rt::Value;
        fn rest(this: ::clojure_rt::Value) -> ::clojure_rt::Value;
    }
}

clojure_rt_macros::protocol! {
    pub trait INext {
        fn next(this: ::clojure_rt::Value) -> ::clojure_rt::Value;
    }
}

// nil impls — cljs (extend-type nil ISeq ...) shape. (first nil) → nil,
// (rest nil) → empty list, (next nil) → nil, (seq nil) → nil.

clojure_rt_macros::implements! {
    impl ISeqable for Nil {
        fn seq(this: Value) -> Value {
            let _ = this;
            Value::NIL
        }
    }
}

clojure_rt_macros::implements! {
    impl ISeq for Nil {
        fn first(this: Value) -> Value {
            let _ = this;
            Value::NIL
        }
        fn rest(this: Value) -> Value {
            let _ = this;
            crate::types::list::empty_list()
        }
    }
}

clojure_rt_macros::implements! {
    impl INext for Nil {
        fn next(this: Value) -> Value {
            let _ = this;
            Value::NIL
        }
    }
}
