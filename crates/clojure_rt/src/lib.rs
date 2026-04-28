//! clojure-py runtime substrate.
//!
//! See `docs/superpowers/specs/2026-04-26-clojure-py-runtime-substrate-design.md`.

// Lets `::clojure_rt::...` paths emitted by `clojure_rt_macros` resolve
// when the macros are invoked from inside this crate (e.g. ports in
// `protocols/`).
extern crate self as clojure_rt;

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
    TYPE_NIL, TYPE_BOOL, TYPE_INT64, TYPE_FLOAT64, TYPE_CHAR,
    FIRST_HEAP_TYPE,
};
pub use header::Header;
pub use rc::{dup, drop_value, share};

pub mod registry;
pub use registry::init;

pub mod exception;
pub mod hash;
pub mod primitives;
pub mod protocols;
pub mod rt;
pub mod types;
pub mod bootstrap;
pub mod reader;

#[doc(hidden)]
pub use inventory;

#[doc(hidden)]
#[macro_export]
macro_rules! __inventory_submit_type {
    ($e:expr) => { $crate::inventory::submit! { $e } };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __inventory_submit_protocol {
    ($e:expr) => { $crate::inventory::submit! { $e } };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __inventory_submit_impl {
    ($e:expr) => { $crate::inventory::submit! { $e } };
}

pub use clojure_rt_macros::{register_type, protocol, implements, dispatch};
