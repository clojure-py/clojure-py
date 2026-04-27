//! Port of `clojure.lang.Named` — types that have a name (and
//! optional namespace). Implemented by `SymbolObj` and `KeywordObj`.

clojure_rt_macros::protocol! {
    pub trait Named {
        fn get_namespace(this: ::clojure_rt::Value) -> ::clojure_rt::Value;
        fn get_name(this: ::clojure_rt::Value) -> ::clojure_rt::Value;
    }
}
