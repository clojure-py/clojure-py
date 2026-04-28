//! `IMapEntry` — `(key e)` and `(val e)` on a map-entry pair. Mirrors
//! cljs's `IMapEntry` and JVM's `IMapEntry extends Map.Entry`. Map
//! entries also act as 2-vectors (IIndexed/ICounted) so `(let [[k v]
//! e] …)` destructuring works.

clojure_rt_macros::protocol! {
    pub trait IMapEntry {
        fn key(this: ::clojure_rt::Value) -> ::clojure_rt::Value;
        fn val(this: ::clojure_rt::Value) -> ::clojure_rt::Value;
    }
}
