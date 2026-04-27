//! `ICollection` — the conj-ability protocol. cljs splits JVM's
//! `IPersistentCollection` into several smaller protocols (`ICounted`,
//! `ICollection`, `IEmptyableCollection`, `IEquiv`, `ISeqable`); this
//! module owns the conj half.

clojure_rt_macros::protocol! {
    pub trait ICollection {
        fn conj(this: ::clojure_rt::Value, x: ::clojure_rt::Value)
            -> ::clojure_rt::Value;
    }
}
