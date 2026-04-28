//! `ITransientMap` — `(dissoc! t k)`. cljs calls this `-without!`;
//! we use the user-visible name (`dissoc_bang`) for symmetry with
//! `IMap::dissoc`.

clojure_rt_macros::protocol! {
    pub trait ITransientMap {
        fn dissoc_bang(this: ::clojure_rt::Value, k: ::clojure_rt::Value)
            -> ::clojure_rt::Value;
    }
}
