//! `ISequential` — marker protocol for ordered, sequential
//! collections (lists, vectors, seqs). No methods; the `protocol!`
//! macro synthesizes a `MARKER` method whose presence in a type's
//! per-type table answers `protocol::satisfies(&ISequential::MARKER, v)`.
//!
//! User-facing `(sequential? x)` is `rt::sequential?(v)`.

clojure_rt_macros::protocol! {
    pub trait ISequential {}
}
