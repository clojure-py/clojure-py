//! `LazySeq` — a seq whose realization is deferred to first access.
//! Mirrors `clojure.lang.LazySeq` (JVM) and cljs's `LazySeq`.
//!
//! Holds a thunk `Box<dyn Fn() -> Value + Send + Sync>`. On first
//! `seq`/`first`/`rest`/`next`, the thunk is invoked under a
//! `parking_lot::Mutex` so concurrent threads block until one
//! finishes the realization. The realized result is cached.
//!
//! Realization is two-phase, matching JVM:
//! 1. **sval**: invoke the thunk, store the raw result in `sv`.
//! 2. **seq**: walk the LazySeq chain (the thunk's result may itself
//!    be a `LazySeq` — recurse via `sval` semantics until we hit a
//!    non-lazy seqable), then canonicalize via `rt::seq` into `s`.
//!
//! After full realization, `thunk` is `None`, `sv` is `nil`, and `s`
//! is the canonical seq (or `nil` for empty). `IHash`/`IEquiv` walk
//! via the realized seq, forcing realization of the entire chain.
//!
//! Re-entrant realization (the thunk recursively calling into the
//! same LazySeq) deadlocks on the mutex — Rust's `Mutex` isn't
//! re-entrant. JVM's `synchronized` is, so semantics differ at the
//! edges; in practice no idiomatic `lazy-seq` body should recurse
//! into itself, and this is the simplest correct shape.

use core::sync::atomic::{AtomicI32, Ordering};

use parking_lot::Mutex;

use crate::hash::murmur3;
use crate::protocols::equiv::IEquiv;
use crate::protocols::hash::IHash;
use crate::protocols::meta::{IMeta, IWithMeta};
use crate::protocols::seq::{INext, ISeq, ISeqable};
use crate::protocols::sequential::ISequential;
use crate::value::Value;

pub(crate) struct LazySeqInner {
    thunk: Option<Box<dyn Fn() -> Value + Send + Sync>>,
    sv: Value,
    s:  Value,
}

impl Drop for LazySeqInner {
    fn drop(&mut self) {
        crate::rc::drop_value(self.sv);
        crate::rc::drop_value(self.s);
        // thunk: dropping Option<Box<...>> drops the closure, which
        // runs Drop for any captured environment (e.g. a Value held
        // via a wrapper struct that decrefs on drop).
    }
}

clojure_rt_macros::register_type! {
    pub struct LazySeq {
        inner: Mutex<LazySeqInner>,
        meta:  Value,
        hash:  AtomicI32,
    }
}

#[inline]
fn is_lazy_seq(v: Value) -> bool {
    v.is_heap()
        && match LAZYSEQ_TYPE_ID.get() {
            Some(&id) => v.tag == id,
            None => false,
        }
}

impl LazySeq {
    /// Build a `LazySeq` from a Rust closure. The closure runs at most
    /// once per `LazySeq`; subsequent accesses use the cached result.
    /// The closure must be `Send + Sync` since multiple threads may
    /// race to realize the same lazy seq (the mutex serializes).
    pub fn from_fn(thunk: Box<dyn Fn() -> Value + Send + Sync>) -> Value {
        let inner = LazySeqInner {
            thunk: Some(thunk),
            sv: Value::NIL,
            s: Value::NIL,
        };
        LazySeq::alloc(Mutex::new(inner), Value::NIL, AtomicI32::new(0))
    }

    /// Build a `LazySeq` from an `IFn` Value. The thunk Value's
    /// `invoke_1` (zero-arg call) is invoked on realization. The
    /// caller's ref to `thunk` is unchanged (we dup for our own
    /// storage and decref when the LazySeq is dropped, even if the
    /// thunk was never invoked).
    pub fn from_ifn(thunk: Value) -> Value {
        crate::rc::dup(thunk);
        // Wrap the Value in a guard that decref's when the closure
        // is dropped. Without this, the thunk Value's ref leaks if
        // the LazySeq is dropped without ever being realized.
        struct ThunkValueGuard { thunk: Value }
        impl Drop for ThunkValueGuard {
            fn drop(&mut self) { crate::rc::drop_value(self.thunk); }
        }
        let guard = ThunkValueGuard { thunk };
        let closure: Box<dyn Fn() -> Value + Send + Sync> = Box::new(move || {
            crate::rt::invoke(guard.thunk, &[])
        });
        Self::from_fn(closure)
    }
}

/// Force realization of `this`, returning the canonical seq (the
/// `s` field). Subsequent calls return the same cached `s`.
fn realize(this: Value) -> Value {
    let body = unsafe { LazySeq::body(this) };
    let mut guard = body.inner.lock();

    // sval phase: invoke our own thunk if not yet realized.
    if let Some(thunk) = guard.thunk.take() {
        let result = thunk();
        guard.sv = result;
    }

    // If sv is set, walk the lazy chain + canonicalize into s.
    if !guard.sv.is_nil() {
        let mut ls = guard.sv;
        guard.sv = Value::NIL; // we own ls now

        // Walk LazySeq layers via sval semantics.
        while is_lazy_seq(ls) {
            let inner_body = unsafe { LazySeq::body(ls) };
            let mut inner_guard = inner_body.inner.lock();
            // sval on the inner LazySeq.
            if let Some(thunk) = inner_guard.thunk.take() {
                let result = thunk();
                inner_guard.sv = result;
            }
            let next = if !inner_guard.sv.is_nil() {
                let v = inner_guard.sv;
                inner_guard.sv = Value::NIL;
                v
            } else {
                let s = inner_guard.s;
                crate::rc::dup(s);
                s
            };
            drop(inner_guard);
            crate::rc::drop_value(ls);
            ls = next;
        }

        // Canonicalize via rt::seq (yields nil for empty).
        let canonical = crate::rt::seq(ls);
        crate::rc::drop_value(ls);
        guard.s = canonical;
    }

    let s = guard.s;
    crate::rc::dup(s);
    s
}

clojure_rt_macros::implements! {
    impl ISeqable for LazySeq {
        fn seq(this: Value) -> Value {
            realize(this)
        }
    }
}

clojure_rt_macros::implements! {
    impl ISeq for LazySeq {
        fn first(this: Value) -> Value {
            let s = realize(this);
            if s.is_nil() { return Value::NIL; }
            let f = crate::rt::first(s);
            crate::rc::drop_value(s);
            f
        }
        fn rest(this: Value) -> Value {
            let s = realize(this);
            if s.is_nil() {
                return crate::types::list::empty_list();
            }
            let r = crate::rt::rest(s);
            crate::rc::drop_value(s);
            r
        }
    }
}

clojure_rt_macros::implements! {
    impl INext for LazySeq {
        fn next(this: Value) -> Value {
            let s = realize(this);
            if s.is_nil() { return Value::NIL; }
            let n = crate::rt::next(s);
            crate::rc::drop_value(s);
            n
        }
    }
}

clojure_rt_macros::implements! {
    impl IMeta for LazySeq {
        fn meta(this: Value) -> Value {
            let m = unsafe { LazySeq::body(this) }.meta;
            crate::rc::dup(m);
            m
        }
    }
}

clojure_rt_macros::implements! {
    impl IWithMeta for LazySeq {
        fn with_meta(this: Value, meta: Value) -> Value {
            // Forward to the realized seq's with_meta if already
            // realized; otherwise we'd need a pre-realized clone of
            // the thunk. Simplest correct: realize, then with_meta
            // on the realized seq.
            let s = realize(this);
            if s.is_nil() {
                // Nothing to attach meta to. Build an empty list
                // with meta.
                crate::rc::dup(meta);
                return crate::rt::with_meta(crate::types::list::empty_list(), meta);
            }
            let r = crate::rt::with_meta(s, meta);
            crate::rc::drop_value(s);
            r
        }
    }
}

clojure_rt_macros::implements! {
    impl IHash for LazySeq {
        fn hash(this: Value) -> Value {
            let body = unsafe { LazySeq::body(this) };
            let cached = body.hash.load(Ordering::Relaxed);
            if cached != 0 {
                return Value::int(cached as i64);
            }
            // Force realization by walking via first/next.
            let mut hash: i32 = 1;
            let mut n: i32 = 0;
            let mut cur = realize(this);
            while !cur.is_nil() {
                let f = crate::rt::first(cur);
                let h = crate::rt::hash(f).as_int().unwrap_or(0) as i32;
                crate::rc::drop_value(f);
                hash = hash.wrapping_mul(31).wrapping_add(h);
                n = n.wrapping_add(1);
                let nxt = crate::rt::next(cur);
                crate::rc::drop_value(cur);
                cur = nxt;
            }
            let h = murmur3::mix_coll_hash(hash, n);
            body.hash.store(h, Ordering::Relaxed);
            Value::int(h as i64)
        }
    }
}

clojure_rt_macros::implements! {
    impl IEquiv for LazySeq {
        fn equiv(this: Value, other: Value) -> Value {
            // Same-type for now (cross-type sequential equiv is
            // deferred — same as Cons / VecSeq / etc.).
            if other.tag != this.tag {
                return Value::FALSE;
            }
            // Walk both via realized seqs.
            let a = realize(this);
            let b = realize(other);
            let r = crate::types::cons::generic_seq_equiv(a, b);
            crate::rc::drop_value(a);
            crate::rc::drop_value(b);
            if r { Value::TRUE } else { Value::FALSE }
        }
    }
}

clojure_rt_macros::implements! { impl ISequential for LazySeq {} }
