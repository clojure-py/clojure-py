//! `Atom` — Clojure's reference type for uncoordinated, synchronous,
//! independent state. Mirrors JVM `clojure.lang.Atom`.
//!
//! Storage is `ArcSwap<AtomCell>` — readers do a lock-free
//! `load_full` to snapshot the current `Arc<AtomCell>`, copy the
//! 16-byte `Value` out, and `dup` it. Writers (`reset!`, `swap!`,
//! `compare-and-set!`) build a fresh `Arc<AtomCell>` and CAS the
//! slot; on contention `swap!` retries.
//!
//! Why ArcSwap rather than `Mutex<Value>`: reads stay lock-free and
//! refcount-safe across threads. The JVM uses `AtomicReference` (lock-
//! free thanks to GC); we get the same shape via ArcSwap's epoch /
//! hazard-pointer machinery, which delays freeing the previous Arc
//! until no outstanding reader observes it. That closes the
//! observe-then-dup window that would otherwise be unsafe under
//! refcounted heap.
//!
//! Inner `Value`s are forced into shared mode before publication —
//! biased mode would fail under cross-thread `dup`/`drop`. The atom
//! itself is shared the moment `Atom::new` returns, since by
//! construction it's meant for cross-thread use.
//!
//! Watches/validators (the `IRef` half of the JVM split) are
//! deferred — this slice is just the core mutable cell.

use std::sync::Arc;

use arc_swap::ArcSwap;

use crate::protocols::atom::IAtom;
use crate::protocols::deref::IDeref;
use crate::protocols::meta::{IMeta, IWithMeta};
use crate::value::Value;

/// Inner cell — owns one ref of the contained `Value`. Drop runs
/// when the last `Arc<AtomCell>` reference is released (i.e. the
/// atom has moved on past this value), and decrements the held ref.
pub(crate) struct AtomCell {
    pub(crate) v: Value,
}

impl Drop for AtomCell {
    fn drop(&mut self) {
        crate::rc::drop_value(self.v);
    }
}

clojure_rt_macros::register_type! {
    pub struct Atom {
        cell: ArcSwap<AtomCell>,
        meta: Value,
    }
}

impl Atom {
    /// `(atom x)` — wrap `x` as a fresh atom. Borrow semantics: the
    /// caller's `x` is dup'd into the atom's storage. The atom is
    /// pre-shared since atoms exist to cross threads; the inner
    /// value is also shared so cross-thread reads can `dup` it.
    pub fn new(initial: Value) -> Value {
        crate::rc::dup(initial);
        crate::rc::share(initial);
        let cell = Arc::new(AtomCell { v: initial });
        let v = Atom::alloc(ArcSwap::from(cell), Value::NIL);
        crate::rc::share(v);
        v
    }
}

clojure_rt_macros::implements! {
    impl IDeref for Atom {
        fn deref(this: Value) -> Value {
            let body = unsafe { Atom::body(this) };
            let snap = body.cell.load_full();
            let v = snap.v;
            crate::rc::dup(v);
            v
        }
    }
}

clojure_rt_macros::implements! {
    impl IAtom for Atom {
        fn reset(this: Value, new_val: Value) -> Value {
            let body = unsafe { Atom::body(this) };
            crate::rc::dup(new_val);
            crate::rc::share(new_val);
            let new_arc = Arc::new(AtomCell { v: new_val });
            // `store` drops the previous Arc — its AtomCell::drop
            // releases the old value's ref.
            body.cell.store(new_arc);
            crate::rc::dup(new_val);
            new_val
        }

        fn compare_and_set(this: Value, old: Value, new: Value) -> Value {
            let body = unsafe { Atom::body(this) };
            // Snapshot, equiv against `old`, then ptr-CAS. We use
            // pointer-equality on the Arc to confirm the swap took
            // (matches JVM AtomicReference.compareAndSet).
            let snap = body.cell.load_full();
            if !crate::rt::equiv(snap.v, old).as_bool().unwrap_or(false) {
                return Value::FALSE;
            }
            crate::rc::dup(new);
            crate::rc::share(new);
            let new_arc = Arc::new(AtomCell { v: new });
            let witness = body.cell.compare_and_swap(&snap, new_arc);
            if Arc::ptr_eq(&witness, &snap) {
                Value::TRUE
            } else {
                Value::FALSE
            }
        }

        fn swap_2(this: Value, f: Value) -> Value {
            swap_impl(this, f, &[])
        }

        fn swap_3(this: Value, f: Value, a1: Value) -> Value {
            swap_impl(this, f, &[a1])
        }

        fn swap_4(this: Value, f: Value, a1: Value, a2: Value) -> Value {
            swap_impl(this, f, &[a1, a2])
        }

        fn swap_5(this: Value, f: Value, a1: Value, a2: Value, a3: Value) -> Value {
            swap_impl(this, f, &[a1, a2, a3])
        }
    }
}

clojure_rt_macros::implements! {
    impl IMeta for Atom {
        fn meta(this: Value) -> Value {
            let m = unsafe { Atom::body(this) }.meta;
            crate::rc::dup(m);
            m
        }
    }
}

clojure_rt_macros::implements! {
    impl IWithMeta for Atom {
        fn with_meta(this: Value, meta: Value) -> Value {
            // Atoms are reference types; `with-meta` returns a fresh
            // atom holding the same current value snapshot but with
            // new meta. (JVM Atom is mutable-meta in place; we
            // intentionally diverge to keep with-meta value-semantic
            // here — callers wanting to set meta in place would use
            // a future `alter-meta!` direct call.)
            let body = unsafe { Atom::body(this) };
            let snap = body.cell.load_full();
            let v = snap.v;
            crate::rc::dup(v);
            crate::rc::share(v);
            crate::rc::dup(meta);
            let cell = Arc::new(AtomCell { v });
            let a = Atom::alloc(ArcSwap::from(cell), meta);
            crate::rc::share(a);
            a
        }
    }
}

/// Generalized swap! body. `extra` are the user-supplied trailing
/// args (excluding the receiver and `f`); each call to `f` receives
/// `(current, extra…)`. Borrow semantics throughout.
fn swap_impl(this: Value, f: Value, extra: &[Value]) -> Value {
    let body = unsafe { Atom::body(this) };
    loop {
        let snap = body.cell.load_full();
        let cur = snap.v;
        // Build the call args = [cur, extra...]. We hold `snap`
        // alive across the invoke so `cur`'s heap storage is pinned.
        let mut args: Vec<Value> = Vec::with_capacity(1 + extra.len());
        args.push(cur);
        args.extend_from_slice(extra);
        let new_v = crate::rt::invoke(f, &args);
        if new_v.is_exception() {
            return new_v;
        }
        crate::rc::share(new_v);
        let new_arc = Arc::new(AtomCell { v: new_v });
        let witness = body.cell.compare_and_swap(&snap, new_arc);
        if Arc::ptr_eq(&witness, &snap) {
            // Slot now owns new_v through the installed arc; bump
            // the ref so the caller gets their own.
            crate::rc::dup(new_v);
            return new_v;
        }
        // CAS lost — `new_arc` was dropped (its AtomCell::drop
        // released our +1 of new_v). Retry against the fresh state.
    }
}
