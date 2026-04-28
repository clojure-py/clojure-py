//! `IVolatile` — single-threaded mutable cell. Mirrors JVM
//! `clojure.lang.IVolatile` (just `reset`); the surface-level
//! `vswap!` is implemented in terms of `deref` + `reset` at the
//! rt-helper layer, matching how `clojure.core/vswap!` is defined.
//!
//! Volatiles are intended for transducer state — single-thread by
//! contract. They don't carry validators, watches, or meta; if you
//! need any of that, use `Atom`.

clojure_rt_macros::protocol! {
    pub trait IVolatile {
        fn reset(this: ::clojure_rt::Value, new_val: ::clojure_rt::Value)
            -> ::clojure_rt::Value;
    }
}
