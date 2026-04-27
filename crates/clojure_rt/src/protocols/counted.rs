//! Port of `clojure.lang.Counted` (`int count()`).
//!
//! Per-primitive Clojure semantics are installed as ordinary `implements!`
//! blocks against the marker types in `crate::primitives`; dispatch finds
//! them through the standard per-type table and the protocol carries no
//! tag-case-analysis fallback.

use crate::primitives::*;
use crate::value::Value;

clojure_rt_macros::protocol! {
    pub trait Counted {
        fn count(this: ::clojure_rt::Value) -> ::clojure_rt::Value;
    }
}

clojure_rt_macros::implements! {
    impl Counted for Nil {
        fn count(this: Value) -> Value {
            let _ = this;
            Value::int(0)
        }
    }
}
