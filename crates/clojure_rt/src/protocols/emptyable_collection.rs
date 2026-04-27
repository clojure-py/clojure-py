//! `IEmptyableCollection` — `(empty coll)` returns the empty form of a
//! collection's type. Lifted from cljs (JVM has no separate interface;
//! the operation lives on `IPersistentCollection`).

clojure_rt_macros::protocol! {
    pub trait IEmptyableCollection {
        fn empty(this: ::clojure_rt::Value) -> ::clojure_rt::Value;
    }
}
