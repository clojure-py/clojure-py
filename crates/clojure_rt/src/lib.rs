//! clojure-py runtime substrate.
//!
//! See `docs/superpowers/specs/2026-04-26-clojure-py-runtime-substrate-design.md`.

pub mod value;
pub mod header;
pub mod gc;
pub mod rc;
pub mod type_registry;
pub mod dispatch;
pub mod protocol;
pub mod error;

pub use value::{
    Value, TypeId,
    TYPE_NIL, TYPE_BOOL, TYPE_INT64, TYPE_FLOAT64, TYPE_CHAR, TYPE_PYOBJECT,
    FIRST_HEAP_TYPE,
};
pub use header::Header;
pub use rc::{dup, drop_value, share};
