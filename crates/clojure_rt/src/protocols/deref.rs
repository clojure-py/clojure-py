//! `IDeref` — `(deref x)` returns the wrapped value of a deref-able
//! reference type. First user is `Reduced`; future users include
//! `Atom`, `Ref`, `Delay`, `Future`, `Promise`.

clojure_rt_macros::protocol! {
    pub trait IDeref {
        fn deref(this: ::clojure_rt::Value) -> ::clojure_rt::Value;
    }
}
