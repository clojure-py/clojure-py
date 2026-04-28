//! `Atom` — Clojure's reference type for uncoordinated, synchronous,
//! independent state. Mirrors JVM `clojure.lang.Atom`.
//!
//! # Cells
//! Four `ArcSwap<AtomCell>` slots, each `AtomCell` owning one ref of
//! a `Value` (released on cell drop):
//!
//! - `cell`      — the current value
//! - `meta`      — mutable meta map (or nil)
//! - `validator` — a 1-arg fn predicate (or nil)
//! - `watches`   — a key→callback map (or nil when no watches yet)
//!
//! ArcSwap gives lock-free reads + CAS-style writes that stay
//! refcount-safe across threads — its hazard-pointer-style epoch
//! reclamation closes the observe-pointer-then-`dup` window that a
//! naive lock-free pointer swap would leave open under a refcounted
//! heap. The JVM gets the same property from GC.
//!
//! # Validator + watches lifecycle
//! Every value transition (`reset!`, `swap!`, `compare-and-set!`)
//! runs the validator on the candidate new value before commit and
//! fires watches `(fn [key ref old new])` after a successful commit.
//! Validator failure rejects the change without retry; watch failure
//! propagates but does not (cannot) roll the value back.
//!
//! # Meta
//! Atoms have *mutable* meta — reachable via `(meta a)`,
//! `(reset-meta! a m)`, `(alter-meta! a f & args)`. They do *not*
//! implement `IWithMeta`: `(with-meta atom-x m)` is unsupported in
//! JVM Clojure for the same reason.
//!
//! Inner Values are forced into shared mode before publication so
//! cross-thread `dup`/`drop` won't trip the owner-tid debug check;
//! the atom heap object itself is shared the moment `Atom::new`
//! returns.

use std::sync::Arc;

use arc_swap::ArcSwap;

use crate::protocols::atom::IAtom;
use crate::protocols::deref::IDeref;
use crate::protocols::meta::IMeta;
use crate::protocols::r#ref::IRef;
use crate::protocols::reference::IReference;
use crate::protocols::watchable::IWatchable;
use crate::value::Value;

/// Inner cell — owns one ref of a `Value`. `Drop` runs when the last
/// `Arc<AtomCell>` reference is released and decrements the held ref.
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
        cell:      ArcSwap<AtomCell>,
        meta:      ArcSwap<AtomCell>,
        validator: ArcSwap<AtomCell>,
        watches:   ArcSwap<AtomCell>,
    }
}

/// Wrap a `Value` we own a +1 of into a fresh `Arc<AtomCell>`. The
/// caller is transferring the ref to the cell (no extra dup here).
#[inline]
fn cell(v: Value) -> Arc<AtomCell> {
    Arc::new(AtomCell { v })
}

/// Take a borrowed `Value` and produce an `Arc<AtomCell>` that owns
/// a fresh +1 of it (dup'd internally). For storing snapshots we
/// don't already own.
#[inline]
fn cell_dup(v: Value) -> Arc<AtomCell> {
    crate::rc::dup(v);
    Arc::new(AtomCell { v })
}

impl Atom {
    /// `(atom x)` — wrap `x` as a fresh atom. Borrow semantics on
    /// `x`. The atom itself is shared (it exists to cross threads);
    /// the inner value is also shared so cross-thread reads can dup.
    pub fn new(initial: Value) -> Value {
        crate::rc::share(initial);
        let v = Atom::alloc(
            ArcSwap::from(cell_dup(initial)),
            ArcSwap::from(cell(Value::NIL)),
            ArcSwap::from(cell(Value::NIL)),
            ArcSwap::from(cell(Value::NIL)),
        );
        crate::rc::share(v);
        v
    }
}

// --- IDeref -----------------------------------------------------------------

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

// --- IAtom ------------------------------------------------------------------

clojure_rt_macros::implements! {
    impl IAtom for Atom {
        fn reset(this: Value, new_val: Value) -> Value {
            if let Some(err) = run_validator(this, new_val) {
                return err;
            }
            let body = unsafe { Atom::body(this) };
            crate::rc::share(new_val);
            let new_arc = cell_dup(new_val);
            // ArcSwap::swap atomically replaces and returns the old Arc.
            let old_arc = body.cell.swap(new_arc);
            // Fire watches with the committed (old, new) pair. old_arc
            // pins the old value's ref alive across the call.
            fire_watches(this, old_arc.v, new_val);
            drop(old_arc);
            crate::rc::dup(new_val);
            new_val
        }

        fn compare_and_set(this: Value, old: Value, new: Value) -> Value {
            let body = unsafe { Atom::body(this) };
            let snap = body.cell.load_full();
            if !crate::rt::equiv(snap.v, old).as_bool().unwrap_or(false) {
                return Value::FALSE;
            }
            if let Some(err) = run_validator(this, new) {
                return err;
            }
            crate::rc::share(new);
            let new_arc = cell_dup(new);
            let witness = body.cell.compare_and_swap(&snap, new_arc);
            if Arc::ptr_eq(&witness, &snap) {
                fire_watches(this, snap.v, new);
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

// --- IRef -------------------------------------------------------------------

clojure_rt_macros::implements! {
    impl IRef for Atom {
        fn set_validator(this: Value, f: Value) -> Value {
            let body = unsafe { Atom::body(this) };
            // Reject the install if the new validator wouldn't accept
            // the atom's current value — same shape as JVM
            // Atom.setValidator.
            if !f.is_nil() {
                let snap = body.cell.load_full();
                let r = crate::rt::invoke(f, &[snap.v]);
                if r.is_exception() {
                    return r;
                }
                let ok = r.as_bool().unwrap_or(false);
                crate::rc::drop_value(r);
                if !ok {
                    return crate::exception::make_foreign(
                        "Invalid reference state".to_string(),
                    );
                }
            }
            crate::rc::share(f);
            body.validator.store(cell_dup(f));
            Value::NIL
        }

        fn get_validator(this: Value) -> Value {
            let body = unsafe { Atom::body(this) };
            let snap = body.validator.load_full();
            let v = snap.v;
            crate::rc::dup(v);
            v
        }
    }
}

// --- IWatchable -------------------------------------------------------------

clojure_rt_macros::implements! {
    impl IWatchable for Atom {
        fn add_watch(this: Value, key: Value, f: Value) -> Value {
            let body = unsafe { Atom::body(this) };
            crate::rc::share(key);
            crate::rc::share(f);
            loop {
                let snap = body.watches.load_full();
                let cur = snap.v;
                // nil → start with an empty array map. assoc returns
                // a fresh map either way; share before publishing.
                let base = if cur.is_nil() {
                    crate::rt::array_map(&[])
                } else {
                    crate::rc::dup(cur);
                    cur
                };
                let new_map = crate::rt::assoc(base, key, f);
                crate::rc::drop_value(base);
                crate::rc::share(new_map);
                let new_arc = cell(new_map);
                let witness = body.watches.compare_and_swap(&snap, new_arc);
                if Arc::ptr_eq(&witness, &snap) {
                    crate::rc::dup(this);
                    return this;
                }
            }
        }

        fn remove_watch(this: Value, key: Value) -> Value {
            let body = unsafe { Atom::body(this) };
            loop {
                let snap = body.watches.load_full();
                let cur = snap.v;
                if cur.is_nil() {
                    crate::rc::dup(this);
                    return this;
                }
                crate::rc::dup(cur);
                let new_map = crate::rt::dissoc(cur, key);
                crate::rc::drop_value(cur);
                crate::rc::share(new_map);
                let new_arc = cell(new_map);
                let witness = body.watches.compare_and_swap(&snap, new_arc);
                if Arc::ptr_eq(&witness, &snap) {
                    crate::rc::dup(this);
                    return this;
                }
            }
        }

        fn notify_watches(this: Value, old: Value, new: Value) -> Value {
            fire_watches(this, old, new);
            Value::NIL
        }
    }
}

// --- IMeta + IReference -----------------------------------------------------

clojure_rt_macros::implements! {
    impl IMeta for Atom {
        fn meta(this: Value) -> Value {
            let body = unsafe { Atom::body(this) };
            let snap = body.meta.load_full();
            let m = snap.v;
            crate::rc::dup(m);
            m
        }
    }
}

clojure_rt_macros::implements! {
    impl IReference for Atom {
        fn reset_meta(this: Value, m: Value) -> Value {
            let body = unsafe { Atom::body(this) };
            crate::rc::share(m);
            body.meta.store(cell_dup(m));
            crate::rc::dup(m);
            m
        }

        fn alter_meta_2(this: Value, f: Value) -> Value {
            alter_meta_impl(this, f, &[])
        }
        fn alter_meta_3(this: Value, f: Value, a1: Value) -> Value {
            alter_meta_impl(this, f, &[a1])
        }
        fn alter_meta_4(this: Value, f: Value, a1: Value, a2: Value) -> Value {
            alter_meta_impl(this, f, &[a1, a2])
        }
        fn alter_meta_5(this: Value, f: Value, a1: Value, a2: Value, a3: Value) -> Value {
            alter_meta_impl(this, f, &[a1, a2, a3])
        }
    }
}

// --- Internals --------------------------------------------------------------

/// Apply the atom's validator (if any) to a candidate new value.
/// Returns `Some(exception_value)` when the change should be
/// rejected, `None` when the value is acceptable.
fn run_validator(this: Value, new_val: Value) -> Option<Value> {
    let body = unsafe { Atom::body(this) };
    let snap = body.validator.load_full();
    let validator = snap.v;
    if validator.is_nil() {
        return None;
    }
    let r = crate::rt::invoke(validator, &[new_val]);
    if r.is_exception() {
        return Some(r);
    }
    let ok = r.as_bool().unwrap_or(false);
    crate::rc::drop_value(r);
    if !ok {
        return Some(crate::exception::make_foreign(
            "Invalid reference state".to_string(),
        ));
    }
    None
}

/// Walk the watches map and call each `(fn [key ref old new])`. If a
/// watch returns an exception value, propagate immediately —
/// remaining watches do not fire (matches JVM, which lets the first
/// throwing watch break the iteration). The atom's value transition
/// has already committed by this point and cannot be rolled back.
fn fire_watches(this: Value, old: Value, new: Value) -> Value {
    let body = unsafe { Atom::body(this) };
    let snap = body.watches.load_full();
    let watches = snap.v;
    if watches.is_nil() {
        return Value::NIL;
    }
    let mut cur = crate::rt::seq(watches);
    while !cur.is_nil() {
        let entry = crate::rt::first(cur);
        let k = crate::rt::key(entry);
        let f = crate::rt::val(entry);
        let r = crate::rt::invoke(f, &[k, this, old, new]);
        crate::rc::drop_value(k);
        crate::rc::drop_value(f);
        crate::rc::drop_value(entry);
        if r.is_exception() {
            crate::rc::drop_value(cur);
            return r;
        }
        crate::rc::drop_value(r);
        let n = crate::rt::next(cur);
        crate::rc::drop_value(cur);
        cur = n;
    }
    crate::rc::drop_value(cur);
    Value::NIL
}

/// Generalized swap! body. `extra` are the trailing user args; each
/// `f` invocation receives `(current, extra…)`. Validator runs on
/// each candidate; watches fire after the CAS commits.
fn swap_impl(this: Value, f: Value, extra: &[Value]) -> Value {
    let body = unsafe { Atom::body(this) };
    loop {
        let snap = body.cell.load_full();
        let cur = snap.v;
        let mut args: Vec<Value> = Vec::with_capacity(1 + extra.len());
        args.push(cur);
        args.extend_from_slice(extra);
        let new_v = crate::rt::invoke(f, &args);
        if new_v.is_exception() {
            return new_v;
        }
        if let Some(err) = run_validator(this, new_v) {
            crate::rc::drop_value(new_v);
            return err;
        }
        crate::rc::share(new_v);
        let new_arc = cell(new_v);
        let witness = body.cell.compare_and_swap(&snap, new_arc);
        if Arc::ptr_eq(&witness, &snap) {
            fire_watches(this, snap.v, new_v);
            crate::rc::dup(new_v);
            return new_v;
        }
        // CAS lost — `new_arc` was dropped, releasing our +1 of
        // `new_v`. Loop against the fresh state.
    }
}

/// Generalized alter-meta! body. Same shape as `swap_impl` but on
/// the meta cell — no validator, no watches.
fn alter_meta_impl(this: Value, f: Value, extra: &[Value]) -> Value {
    let body = unsafe { Atom::body(this) };
    loop {
        let snap = body.meta.load_full();
        let cur = snap.v;
        let mut args: Vec<Value> = Vec::with_capacity(1 + extra.len());
        args.push(cur);
        args.extend_from_slice(extra);
        let new_m = crate::rt::invoke(f, &args);
        if new_m.is_exception() {
            return new_m;
        }
        crate::rc::share(new_m);
        let new_arc = cell(new_m);
        let witness = body.meta.compare_and_swap(&snap, new_arc);
        if Arc::ptr_eq(&witness, &snap) {
            crate::rc::dup(new_m);
            return new_m;
        }
    }
}
