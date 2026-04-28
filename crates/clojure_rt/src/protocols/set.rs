//! `ISet` — set-shaped operations. `disjoin` removes a member;
//! `contains` is a presence check (the analog of
//! `IAssociative::contains_key` for sets, mirroring JVM
//! `IPersistentSet.contains`). User-visible names are `disj` /
//! `contains?`.

clojure_rt_macros::protocol! {
    pub trait ISet {
        fn disjoin(this: ::clojure_rt::Value, k: ::clojure_rt::Value)
            -> ::clojure_rt::Value;
        fn contains(this: ::clojure_rt::Value, k: ::clojure_rt::Value)
            -> ::clojure_rt::Value;
    }
}
