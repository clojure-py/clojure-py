//! `IPending` — has-this-deferred-value-been-realized? Mirrors JVM
//! `clojure.lang.IPending`. Used by `(realized? x)` against delays,
//! futures, promises, and lazy seqs; we expose the predicate from
//! the same protocol slot.

clojure_rt_macros::protocol! {
    pub trait IPending {
        fn is_realized(this: ::clojure_rt::Value) -> ::clojure_rt::Value;
    }
}
