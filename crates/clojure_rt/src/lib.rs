//! clojure-py runtime substrate.
//!
//! See `docs/superpowers/specs/2026-04-26-clojure-py-runtime-substrate-design.md`.

pub mod value;

pub use value::{
    Value, TypeId,
    TYPE_NIL, TYPE_BOOL, TYPE_INT64, TYPE_FLOAT64, TYPE_CHAR, TYPE_PYOBJECT,
    FIRST_HEAP_TYPE,
};

pub mod header;
pub use header::Header;
