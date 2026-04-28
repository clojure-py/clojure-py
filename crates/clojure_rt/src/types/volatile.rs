//! `Volatile` — single-threaded mutable cell. Mirrors JVM
//! `clojure.lang.Volatile`.
//!
//! Backed by `ArcSwap<VolatileCell>` for the same reasons as
//! `Atom`: lock-free reads (a single atomic load + Arc clone) and
//! refcount-safe publication via arc-swap's hazard-pointer-style
//! reclamation. Volatile is "single-threaded by contract" so we
//! don't expect contention, but matching Atom's storage shape keeps
//! the per-read cost identical and avoids a Mutex acquire on every
//! transducer step.
//!
//! No validators, no watches, no meta — that's by design (and by
//! `clojure.lang.IVolatile`'s minimal surface).

use std::sync::Arc;

use arc_swap::ArcSwap;

use crate::protocols::deref::IDeref;
use crate::protocols::volatile::IVolatile;
use crate::value::Value;

/// Inner cell — owns one ref of the contained `Value`. Drop runs
/// when the last `Arc<VolatileCell>` reference is released and
/// decrements the held ref.
pub(crate) struct VolatileCell {
    pub(crate) v: Value,
}

impl Drop for VolatileCell {
    fn drop(&mut self) {
        crate::rc::drop_value(self.v);
    }
}

clojure_rt_macros::register_type! {
    pub struct Volatile {
        cell: ArcSwap<VolatileCell>,
    }
}

#[inline]
fn cell_dup(v: Value) -> Arc<VolatileCell> {
    crate::rc::dup(v);
    Arc::new(VolatileCell { v })
}

impl Volatile {
    /// `(volatile! x)` — wrap `x`. Borrow semantics on `x`.
    pub fn new(initial: Value) -> Value {
        Volatile::alloc(ArcSwap::from(cell_dup(initial)))
    }
}

clojure_rt_macros::implements! {
    impl IDeref for Volatile {
        fn deref(this: Value) -> Value {
            let body = unsafe { Volatile::body(this) };
            let snap = body.cell.load_full();
            let v = snap.v;
            crate::rc::dup(v);
            v
        }
    }
}

clojure_rt_macros::implements! {
    impl IVolatile for Volatile {
        fn reset(this: Value, new_val: Value) -> Value {
            let body = unsafe { Volatile::body(this) };
            // ArcSwap::store atomically replaces the slot. The old
            // Arc is dropped here — its VolatileCell::drop releases
            // the previous value's ref.
            body.cell.store(cell_dup(new_val));
            crate::rc::dup(new_val);
            new_val
        }
    }
}
