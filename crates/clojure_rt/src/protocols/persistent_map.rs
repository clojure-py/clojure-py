//! `IPersistentMap` — marker protocol for "this satisfies the
//! persistent-map contract" (associative + countable + seqable +
//! conj-of-MapEntry-extends). Behavior lives in the constituent
//! protocols; this is the `(map? x)` predicate target.

clojure_rt_macros::protocol! {
    pub trait IPersistentMap {}
}
