//! `IReduce` — collection-driven reduce. Two arities:
//!
//! - `reduce_2(coll, f)` — no init. Empty coll calls `(f)` for the
//!   identity; non-empty uses the first element as the seed.
//! - `reduce_3(coll, f, init)` — folds with `init` as the seed.
//!
//! Implementations short-circuit when the accumulator is a `Reduced`,
//! returning `@reduced-acc`. The runtime helper `rt::reduce` provides
//! the chunk-aware fallback for collections without a direct impl.

clojure_rt_macros::protocol! {
    pub trait IReduce {
        fn reduce_2(this: ::clojure_rt::Value, f: ::clojure_rt::Value)
            -> ::clojure_rt::Value;
        fn reduce_3(
            this: ::clojure_rt::Value,
            f:    ::clojure_rt::Value,
            init: ::clojure_rt::Value,
        ) -> ::clojure_rt::Value;
    }
}
