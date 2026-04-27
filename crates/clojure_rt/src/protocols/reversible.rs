//! `IReversible` — `(rseq coll)` returns a seq over the collection in
//! reverse order, in O(1). Mirrors JVM `Reversible`. Implemented by
//! vectors and sorted maps/sets.

clojure_rt_macros::protocol! {
    pub trait IReversible {
        fn rseq(this: ::clojure_rt::Value) -> ::clojure_rt::Value;
    }
}
