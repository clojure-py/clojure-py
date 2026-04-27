//! Thin static dispatchers — Clojure's `RT.*` cross-roads.
//!
//! Each helper is one line: a `dispatch!` through the corresponding
//! protocol method. Type-specific behavior lives in the protocol's
//! built-in fallback or in per-type `implements!` blocks, never here.
//! Helpers return `Value` uniformly so that throwable-Value exceptions
//! propagate unobstructed; numeric extraction (`.as_int()`) happens at
//! the leaf call site.

use crate::protocols::coll::{ICollection, IEmptyableCollection, IIndexed, IStack};
use crate::protocols::counted::ICounted;
use crate::protocols::equiv::IEquiv;
use crate::protocols::hash::IHash;
use crate::protocols::meta::{IMeta, IWithMeta};
use crate::protocols::named::INamed;
use crate::protocols::seq::{INext, ISeq, ISeqable};
use crate::protocols::sequential::ISequential;
use crate::types::keyword::KeywordObj;
use crate::types::list::{empty_list, PersistentList};
use crate::types::string::StringObj;
use crate::types::symbol::SymbolObj;
use crate::value::Value;

#[inline]
pub fn count(v: Value) -> Value {
    clojure_rt_macros::dispatch!(ICounted::count, &[v])
}

#[inline]
pub fn hash(v: Value) -> Value {
    clojure_rt_macros::dispatch!(IHash::hash, &[v])
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
    clojure_rt_macros::dispatch!(INamed::name, &[v])
}

#[inline]
pub fn namespace(v: Value) -> Value {
    clojure_rt_macros::dispatch!(INamed::namespace, &[v])
}

#[inline]
pub fn meta(v: Value) -> Value {
    clojure_rt_macros::dispatch!(IMeta::meta, &[v])
}

#[inline]
pub fn with_meta(v: Value, m: Value) -> Value {
    clojure_rt_macros::dispatch!(IWithMeta::with_meta, &[v, m])
}

// --- Seq abstraction --------------------------------------------------------

#[inline]
pub fn seq(v: Value) -> Value {
    clojure_rt_macros::dispatch!(ISeqable::seq, &[v])
}

#[inline]
pub fn first(v: Value) -> Value {
    clojure_rt_macros::dispatch!(ISeq::first, &[v])
}

#[inline]
pub fn rest(v: Value) -> Value {
    clojure_rt_macros::dispatch!(ISeq::rest, &[v])
}

#[inline]
pub fn next(v: Value) -> Value {
    clojure_rt_macros::dispatch!(INext::next, &[v])
}

// --- Collection ops ---------------------------------------------------------

#[inline]
pub fn conj(coll: Value, x: Value) -> Value {
    clojure_rt_macros::dispatch!(ICollection::conj, &[coll, x])
}

#[inline]
pub fn empty(coll: Value) -> Value {
    clojure_rt_macros::dispatch!(IEmptyableCollection::empty, &[coll])
}

#[inline]
pub fn peek(coll: Value) -> Value {
    clojure_rt_macros::dispatch!(IStack::peek, &[coll])
}

#[inline]
pub fn pop(coll: Value) -> Value {
    clojure_rt_macros::dispatch!(IStack::pop, &[coll])
}

#[inline]
pub fn nth(coll: Value, n: Value) -> Value {
    clojure_rt_macros::dispatch!(IIndexed::nth, &[coll, n])
}

#[inline]
pub fn nth_default(coll: Value, n: Value, not_found: Value) -> Value {
    clojure_rt_macros::dispatch!(IIndexed::nth_default, &[coll, n, not_found])
}

// --- List constructors ------------------------------------------------------

/// Build a `PersistentList` from a slice of `Value`s.
#[inline]
pub fn list(items: &[Value]) -> Value {
    PersistentList::list(items)
}

/// Cons `x` onto the head of `coll`. If `coll` is nil or a non-list
/// seqable, it's first run through `seq` so the result is always
/// list-shaped.
#[inline]
pub fn cons(x: Value, coll: Value) -> Value {
    let tail = if coll.is_nil() {
        empty_list()
    } else {
        // For PersistentList/EmptyList, this is essentially identity (their
        // ISeqable::seq returns self/nil). For other seqables, this
        // coerces.
        let s = seq(coll);
        if s.is_nil() {
            empty_list()
        } else {
            s
        }
    };
    let result = PersistentList::cons(x, tail);
    crate::rc::drop_value(tail);
    result
}

/// `(sequential? x)` — does `x`'s type marker-implement `ISequential`?
#[inline]
pub fn sequential(v: Value) -> bool {
    crate::protocol::satisfies(&ISequential::MARKER, v)
}
