//! Collection protocols. cljs splits JVM's `IPersistentCollection`
//! into `ICounted` + `ICollection` + `IEmptyableCollection` +
//! `IEquiv` + `ISeqable`. We have ICounted + IEquiv; this module
//! adds the rest, plus `IStack` (peek/pop) and `IIndexed` (nth) for
//! collection ports that follow.

clojure_rt_macros::protocol! {
    pub trait ICollection {
        fn conj(this: ::clojure_rt::Value, x: ::clojure_rt::Value)
            -> ::clojure_rt::Value;
    }
}

clojure_rt_macros::protocol! {
    pub trait IEmptyableCollection {
        fn empty(this: ::clojure_rt::Value) -> ::clojure_rt::Value;
    }
}

clojure_rt_macros::protocol! {
    pub trait IStack {
        fn peek(this: ::clojure_rt::Value) -> ::clojure_rt::Value;
        fn pop(this: ::clojure_rt::Value) -> ::clojure_rt::Value;
    }
}

clojure_rt_macros::protocol! {
    pub trait IIndexed {
        fn nth(this: ::clojure_rt::Value, n: ::clojure_rt::Value)
            -> ::clojure_rt::Value;
        fn nth_default(
            this: ::clojure_rt::Value,
            n: ::clojure_rt::Value,
            not_found: ::clojure_rt::Value,
        ) -> ::clojure_rt::Value;
    }
}
