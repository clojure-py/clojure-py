//! `IPersistentSet` — marker protocol for "satisfies the persistent-
//! set contract." The `(set? x)` predicate target.

clojure_rt_macros::protocol! {
    pub trait IPersistentSet {}
}
