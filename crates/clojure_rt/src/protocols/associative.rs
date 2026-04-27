//! `IAssociative` — key→value association protocol. Mirrors JVM
//! `Associative` plus cljs `IAssociative`/`IFind`. `assoc` extends or
//! replaces; `contains_key` is a presence check; `find` returns a
//! `MapEntry`-shaped pair (or `nil` on miss) and is the standard way
//! to disambiguate "missing" from "present-with-nil-value".

clojure_rt_macros::protocol! {
    pub trait IAssociative {
        fn assoc(
            this: ::clojure_rt::Value,
            k: ::clojure_rt::Value,
            v: ::clojure_rt::Value,
        ) -> ::clojure_rt::Value;
        fn contains_key(
            this: ::clojure_rt::Value,
            k: ::clojure_rt::Value,
        ) -> ::clojure_rt::Value;
        fn find(
            this: ::clojure_rt::Value,
            k: ::clojure_rt::Value,
        ) -> ::clojure_rt::Value;
    }
}
