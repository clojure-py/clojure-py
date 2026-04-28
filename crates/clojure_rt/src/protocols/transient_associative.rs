//! `ITransientAssociative` — `(assoc! t k v)` for transients (vectors
//! and maps). Vectors require integer keys.

clojure_rt_macros::protocol! {
    pub trait ITransientAssociative {
        fn assoc_bang(
            this: ::clojure_rt::Value,
            k:    ::clojure_rt::Value,
            v:    ::clojure_rt::Value,
        ) -> ::clojure_rt::Value;
    }
}
