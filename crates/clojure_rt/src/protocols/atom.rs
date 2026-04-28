//! `IAtom` — the `(atom x)` reference-type protocol. Mirrors JVM
//! `clojure.lang.IAtom` (with `IAtom2`'s `*-vals` variants deferred):
//! `reset!`, `compare-and-set!`, and `swap!` at fixed arities up to
//! 5 user args. Naming follows the project's multi-arity convention:
//! each `swap_<N>` slot's suffix is the *total Rust arity* (receiver
//! `this` + `f` + user args). So `swap_2` is `(swap! a f)`,
//! `swap_3` is `(swap! a f x)`, etc.
//!
//! Watches and validators (the `IRef` half of the JVM split) are
//! deferred to a follow-up slice.

clojure_rt_macros::protocol! {
    pub trait IAtom {
        fn reset(this: ::clojure_rt::Value, new_val: ::clojure_rt::Value)
            -> ::clojure_rt::Value;
        fn compare_and_set(
            this: ::clojure_rt::Value,
            old: ::clojure_rt::Value,
            new: ::clojure_rt::Value,
        ) -> ::clojure_rt::Value;
        fn swap_2(this: ::clojure_rt::Value, f: ::clojure_rt::Value)
            -> ::clojure_rt::Value;
        fn swap_3(
            this: ::clojure_rt::Value,
            f: ::clojure_rt::Value,
            a1: ::clojure_rt::Value,
        ) -> ::clojure_rt::Value;
        fn swap_4(
            this: ::clojure_rt::Value,
            f: ::clojure_rt::Value,
            a1: ::clojure_rt::Value,
            a2: ::clojure_rt::Value,
        ) -> ::clojure_rt::Value;
        fn swap_5(
            this: ::clojure_rt::Value,
            f: ::clojure_rt::Value,
            a1: ::clojure_rt::Value,
            a2: ::clojure_rt::Value,
            a3: ::clojure_rt::Value,
        ) -> ::clojure_rt::Value;
    }
}
