//! `ITransientVector` — `(pop! t)`. Mirrors `IStack::pop` for the
//! transient world.

clojure_rt_macros::protocol! {
    pub trait ITransientVector {
        fn pop_bang(this: ::clojure_rt::Value) -> ::clojure_rt::Value;
    }
}
