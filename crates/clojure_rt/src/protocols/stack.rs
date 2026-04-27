//! `IStack` — peek/pop. Mirrors JVM `IPersistentStack`. Used by lists
//! (head end) and vectors (tail end).

clojure_rt_macros::protocol! {
    pub trait IStack {
        fn peek(this: ::clojure_rt::Value) -> ::clojure_rt::Value;
        fn pop(this: ::clojure_rt::Value) -> ::clojure_rt::Value;
    }
}
