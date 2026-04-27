//! Ports of ClojureScript's `IMeta` and `IWithMeta` (Clojure JVM's
//! `IMeta` and `IObj`). Method names match cljs (`-meta`,
//! `-with-meta`).
//!
//! **Deviation from JVM:** `meta` is a `Value` (any), not an
//! `IPersistentMap`. We don't have maps yet, and the IPersistentMap
//! constraint is a user-surface concern that can be enforced by the
//! reader / `defn` macros later — at the protocol level, treating
//! meta as opaque is enough. Hash and equiv ignore meta on the types
//! that implement these protocols (matches JVM behavior).

clojure_rt_macros::protocol! {
    pub trait IMeta {
        fn meta(this: ::clojure_rt::Value) -> ::clojure_rt::Value;
    }
}

clojure_rt_macros::protocol! {
    pub trait IWithMeta {
        fn with_meta(
            this: ::clojure_rt::Value,
            meta: ::clojure_rt::Value,
        ) -> ::clojure_rt::Value;
    }
}
