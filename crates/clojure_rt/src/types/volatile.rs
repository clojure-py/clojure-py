//! `Volatile` — single-threaded mutable cell. Mirrors JVM
//! `clojure.lang.Volatile`.
//!
//! Backed by `parking_lot::Mutex<Value>`. The lock is required for
//! `Sync`-correctness in the type system but is uncontended by
//! contract — volatiles exist for transducer state where one thread
//! holds and mutates the cell. JVM uses a `volatile` field for
//! visibility; we use a Mutex which gives the same memory-ordering
//! guarantee plus the (hopefully unused) ability to take the lock
//! from another thread without UB.
//!
//! No validators, no watches, no meta — that's by design (and by
//! `clojure.lang.IVolatile`'s minimal surface).

use parking_lot::Mutex;

use crate::protocols::deref::IDeref;
use crate::protocols::volatile::IVolatile;
use crate::value::Value;

pub(crate) struct VolatileInner {
    pub(crate) v: Value,
}

impl Drop for VolatileInner {
    fn drop(&mut self) {
        crate::rc::drop_value(self.v);
    }
}

clojure_rt_macros::register_type! {
    pub struct Volatile {
        inner: Mutex<VolatileInner>,
    }
}

impl Volatile {
    /// `(volatile! x)` — wrap `x`. Borrow semantics on `x`.
    pub fn new(initial: Value) -> Value {
        crate::rc::dup(initial);
        Volatile::alloc(Mutex::new(VolatileInner { v: initial }))
    }
}

clojure_rt_macros::implements! {
    impl IDeref for Volatile {
        fn deref(this: Value) -> Value {
            let body = unsafe { Volatile::body(this) };
            let g = body.inner.lock();
            let v = g.v;
            crate::rc::dup(v);
            v
        }
    }
}

clojure_rt_macros::implements! {
    impl IVolatile for Volatile {
        fn reset(this: Value, new_val: Value) -> Value {
            let body = unsafe { Volatile::body(this) };
            crate::rc::dup(new_val);
            let mut g = body.inner.lock();
            // Drop the old value's ref outside the lock — no, we
            // can't return until we install. Just decrement under the
            // lock; drop_value is fast and doesn't recurse into us.
            let old = g.v;
            g.v = new_val;
            drop(g);
            crate::rc::drop_value(old);
            crate::rc::dup(new_val);
            new_val
        }
    }
}
