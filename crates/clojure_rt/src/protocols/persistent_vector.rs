//! `IPersistentVector` — marker protocol identifying types that satisfy
//! the persistent-vector contract (indexed, conj-at-tail, assoc-by-int).
//! Marker only; behavior lives in the constituent protocols. Used by
//! `vector?`-style queries and by collection helpers that branch on
//! "this is sequential and indexed".

clojure_rt_macros::protocol! {
    pub trait IPersistentVector {}
}
