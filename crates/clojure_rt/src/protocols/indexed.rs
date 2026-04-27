//! `IIndexed` — random-access by integer index. Multi-arity:
//!
//! - `nth_2(coll, n)` — never throws on its own; impls return the
//!   `Value::NOT_FOUND` sentinel for out-of-bounds. The `rt::nth`
//!   helper translates the sentinel into a thrown exception.
//! - `nth_3(coll, n, not_found)` — caller-supplied default; impls
//!   return `not_found` on OOB.

clojure_rt_macros::protocol! {
    pub trait IIndexed {
        fn nth_2(this: ::clojure_rt::Value, n: ::clojure_rt::Value)
            -> ::clojure_rt::Value;
        fn nth_3(
            this: ::clojure_rt::Value,
            n: ::clojure_rt::Value,
            not_found: ::clojure_rt::Value,
        ) -> ::clojure_rt::Value;
    }
}
