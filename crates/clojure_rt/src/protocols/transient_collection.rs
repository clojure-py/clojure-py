//! `ITransientCollection` — operations on a transient.
//!
//! - `conj_bang(t, x)` — mutate-add `x`.
//! - `persistent_bang(t)` — freeze; the transient becomes invalid for
//!   further mutation.
//!
//! Each `*!` op may return a different transient handle (JVM Clojure
//! convention). Callers must use the returned value, not the original.

clojure_rt_macros::protocol! {
    pub trait ITransientCollection {
        fn conj_bang(this: ::clojure_rt::Value, x: ::clojure_rt::Value)
            -> ::clojure_rt::Value;
        fn persistent_bang(this: ::clojure_rt::Value) -> ::clojure_rt::Value;
    }
}
