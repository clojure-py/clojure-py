//! Thin static dispatchers — Clojure's `RT.*` cross-roads.
//!
//! Each helper is one line: a `dispatch!` through the corresponding
//! protocol method. Type-specific behavior lives in the protocol's
//! built-in fallback or in per-type `implements!` blocks, never here.
//! Helpers return `Value` uniformly so that throwable-Value exceptions
//! propagate unobstructed; numeric extraction (`.as_int()`) happens at
//! the leaf call site.

use crate::protocols::counted::Counted;
use crate::protocols::equiv::IEquiv;
use crate::protocols::hash_eq::IHashEq;
use crate::protocols::meta::{IMeta, IObj};
use crate::protocols::named::Named;
use crate::types::keyword::KeywordObj;
use crate::types::string::StringObj;
use crate::types::symbol::SymbolObj;
use crate::value::Value;

#[inline]
pub fn count(v: Value) -> Value {
    clojure_rt_macros::dispatch!(Counted::count, &[v])
}

#[inline]
pub fn hasheq(v: Value) -> Value {
    clojure_rt_macros::dispatch!(IHashEq::hasheq, &[v])
}

#[inline]
pub fn equiv(a: Value, b: Value) -> Value {
    clojure_rt_macros::dispatch!(IEquiv::equiv, &[a, b])
}

#[inline]
pub fn str_new(s: &str) -> Value {
    StringObj::new(s)
}

#[inline]
pub fn symbol(ns: Option<&str>, name: &str) -> Value {
    SymbolObj::intern(ns, name)
}

#[inline]
pub fn keyword(ns: Option<&str>, name: &str) -> Value {
    KeywordObj::intern(ns, name)
}

#[inline]
pub fn name(v: Value) -> Value {
    clojure_rt_macros::dispatch!(Named::get_name, &[v])
}

#[inline]
pub fn namespace(v: Value) -> Value {
    clojure_rt_macros::dispatch!(Named::get_namespace, &[v])
}

#[inline]
pub fn meta(v: Value) -> Value {
    clojure_rt_macros::dispatch!(IMeta::meta, &[v])
}

#[inline]
pub fn with_meta(v: Value, m: Value) -> Value {
    clojure_rt_macros::dispatch!(IObj::with_meta, &[v, m])
}
