//! `IMap` — `(dissoc m k)` removes a key. Splits off from JVM's
//! `IPersistentMap` (which bundles assoc/without/etc.); the assoc
//! side lives in `IAssociative`.

clojure_rt_macros::protocol! {
    pub trait IMap {
        fn dissoc(this: ::clojure_rt::Value, k: ::clojure_rt::Value)
            -> ::clojure_rt::Value;
    }
}
