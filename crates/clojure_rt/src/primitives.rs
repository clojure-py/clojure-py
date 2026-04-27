//! First-class primitive "types".
//!
//! Each primitive Value tag (`TYPE_NIL`, `TYPE_BOOL`, `TYPE_INT64`,
//! `TYPE_FLOAT64`, `TYPE_CHAR`, `TYPE_PYOBJECT`) is registered in the
//! `type_registry` at `init()` time and given a `TYPE_ID` cell of the same
//! shape as `register_type!`-generated cells. This lets `implements!`
//! install a primitive-targeted impl through the same per-type dispatch
//! machinery as heap types — no fallback, no tag-case-analysis layer.
//!
//! Usage:
//!
//! ```ignore
//! use clojure_rt::primitives::Nil;
//! implements! { impl Counted for Nil { fn count(_: Value) -> Value { Value::int(0) } } }
//! ```

use once_cell::sync::OnceCell;

use crate::type_registry::register_primitive;
use crate::value::{
    TypeId,
    TYPE_BOOL, TYPE_CHAR, TYPE_FLOAT64, TYPE_INT64, TYPE_NIL, TYPE_PYOBJECT,
};

// Marker types — zero-sized; only the name participates in `implements!`
// (which forms `{NAME}_TYPE_ID` for cell lookup).

pub struct Nil;
pub struct Bool;
pub struct Int64;
pub struct Float64;
pub struct Char;
pub struct PyObject;

pub static NIL_TYPE_ID:      OnceCell<TypeId> = OnceCell::new();
pub static BOOL_TYPE_ID:     OnceCell<TypeId> = OnceCell::new();
pub static INT64_TYPE_ID:    OnceCell<TypeId> = OnceCell::new();
pub static FLOAT64_TYPE_ID:  OnceCell<TypeId> = OnceCell::new();
pub static CHAR_TYPE_ID:     OnceCell<TypeId> = OnceCell::new();
pub static PYOBJECT_TYPE_ID: OnceCell<TypeId> = OnceCell::new();

/// Register every primitive in the type registry and pre-set the
/// corresponding `*_TYPE_ID` cells. Called by `crate::init()` between
/// protocol-id assignment and impl-table installation, so that impls
/// targeting primitive types resolve their type cells in time.
pub fn init() {
    register_primitive(TYPE_NIL,      "Nil");
    register_primitive(TYPE_BOOL,     "Bool");
    register_primitive(TYPE_INT64,    "Int64");
    register_primitive(TYPE_FLOAT64,  "Float64");
    register_primitive(TYPE_CHAR,     "Char");
    register_primitive(TYPE_PYOBJECT, "PyObject");

    NIL_TYPE_ID.set(TYPE_NIL).ok();
    BOOL_TYPE_ID.set(TYPE_BOOL).ok();
    INT64_TYPE_ID.set(TYPE_INT64).ok();
    FLOAT64_TYPE_ID.set(TYPE_FLOAT64).ok();
    CHAR_TYPE_ID.set(TYPE_CHAR).ok();
    PYOBJECT_TYPE_ID.set(TYPE_PYOBJECT).ok();
}
