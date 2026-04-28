//! `Delay` — lazy, single-shot, memoizing computation. Mirrors JVM
//! `clojure.lang.Delay`. The thunk runs at most once per Delay; the
//! first `force`/`deref` realizes it under a mutex (so concurrent
//! racers wait for the in-progress realization), and subsequent
//! reads return the cached result.
//!
//! Same machinery as `LazySeq` (also Mutex-guarded one-shot), but
//! one shape simpler: there's no chain of nested delays to
//! canonicalize, no `seq` walk, just a single-cell cache.

use core::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;

use crate::protocols::deref::IDeref;
use crate::protocols::pending::IPending;
use crate::value::Value;

pub(crate) struct DelayInner {
    /// Realized result. `Value::NIL` until realization completes;
    /// after that, holds the cached output (with one ref owned by
    /// the Delay).
    val: Value,
    /// Producer. `None` after first realization. Send + Sync because
    /// other threads may race to be the realizing thread.
    thunk: Option<Box<dyn Fn() -> Value + Send + Sync>>,
}

impl Drop for DelayInner {
    fn drop(&mut self) {
        crate::rc::drop_value(self.val);
        // thunk: dropping the Option<Box<...>> runs Drop on the
        // captured environment (e.g. Values held via wrappers).
    }
}

clojure_rt_macros::register_type! {
    pub struct Delay {
        inner:    Mutex<DelayInner>,
        realized: AtomicBool,
    }
}

impl Delay {
    /// Build a Delay from a thunk. The thunk is invoked at most
    /// once. `Send + Sync` is required because realization may
    /// happen on whichever thread first calls `force`/`deref`.
    pub fn from_fn(thunk: Box<dyn Fn() -> Value + Send + Sync>) -> Value {
        let inner = DelayInner {
            val: Value::NIL,
            thunk: Some(thunk),
        };
        Delay::alloc(Mutex::new(inner), AtomicBool::new(false))
    }

    /// Force realization. Returns a borrowed-style Value: caller
    /// gets a +1 ref. Internal entry point used by both `IDeref`
    /// and tests.
    pub fn force(this: Value) -> Value {
        let body = unsafe { Delay::body(this) };
        // Fast path: already realized — just dup the cached value.
        // Use the atomic flag to avoid taking the mutex on every
        // read once the result is cached.
        if body.realized.load(Ordering::Acquire) {
            let g = body.inner.lock();
            let v = g.val;
            crate::rc::dup(v);
            return v;
        }
        // Slow path: lock, double-check, run thunk if still needed.
        let mut g = body.inner.lock();
        if let Some(thunk) = g.thunk.take() {
            let v = thunk();
            // Exception during realization: don't cache, rerun on
            // next access (matches JVM's Delay.deref which lets the
            // thrown exception escape and leaves the delay un-realized
            // for retry). Restore the thunk.
            if v.is_exception() {
                g.thunk = Some(thunk);
                return v;
            }
            g.val = v;
            // Release ordering pairs with Acquire on the fast path.
            body.realized.store(true, Ordering::Release);
        }
        let v = g.val;
        crate::rc::dup(v);
        v
    }
}

clojure_rt_macros::implements! {
    impl IDeref for Delay {
        fn deref(this: Value) -> Value {
            Delay::force(this)
        }
    }
}

clojure_rt_macros::implements! {
    impl IPending for Delay {
        fn is_realized(this: Value) -> Value {
            let body = unsafe { Delay::body(this) };
            if body.realized.load(Ordering::Acquire) {
                Value::TRUE
            } else {
                Value::FALSE
            }
        }
    }
}
