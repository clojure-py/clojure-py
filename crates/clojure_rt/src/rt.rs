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
