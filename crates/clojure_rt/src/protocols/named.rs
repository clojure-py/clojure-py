//! Port of ClojureScript's `INamed` (Clojure JVM's `Named`). Methods
//! `name` and `namespace` match cljs's `-name` / `-namespace`. The
//! leading `-` convention disappears at the Rust level; `rt::name` /
//! `rt::namespace` are the user-facing wrappers.

clojure_rt_macros::protocol! {
    pub trait INamed {
        fn name(this: ::clojure_rt::Value) -> ::clojure_rt::Value;
        fn namespace(this: ::clojure_rt::Value) -> ::clojure_rt::Value;
    }
}
