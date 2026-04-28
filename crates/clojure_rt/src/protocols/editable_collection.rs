//! `IEditableCollection` — `(transient coll)`. The persistent →
//! transient handoff: the resulting transient shares structure with
//! the persistent source, mutates in place where it can (Arc-unique
//! interior nodes), and converts back via `persistent!`.

clojure_rt_macros::protocol! {
    pub trait IEditableCollection {
        fn as_transient(this: ::clojure_rt::Value) -> ::clojure_rt::Value;
    }
}
