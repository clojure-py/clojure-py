//! `IChunk` — operations on a fixed-size block of values produced by
//! a chunked seq. Chunks expose count + nth via `ICounted`/`IIndexed`
//! plus `drop_first` for incremental consumption.

clojure_rt_macros::protocol! {
    pub trait IChunk {
        fn drop_first(this: ::clojure_rt::Value) -> ::clojure_rt::Value;
    }
}
