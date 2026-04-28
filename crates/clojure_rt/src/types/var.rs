//! `Var` — namespace-qualified mutable cell with thread-local
//! bindings. Mirrors JVM `clojure.lang.Var`.
//!
//! # Storage
//! Same `ArcSwap<VarCell>` discipline as `Atom` for the
//! refcount-safe, lock-free reference-type slots:
//!
//! - `root`      — the root value (used when no thread binding is in scope)
//! - `meta`      — mutable meta map (or nil)
//! - `validator` — 1-arg fn (or nil)
//! - `watches`   — key→fn map (or nil for the no-watch fast path)
//!
//! Plus immutable identification fields:
//!
//! - `ns`        — namespace (or `Value::NIL` for an anonymous var)
//! - `sym`       — symbol naming the var (or `Value::NIL`)
//! - `dynamic`   — `AtomicBool`, true means thread bindings are
//!                 honored on `deref`
//!
//! # Thread-local bindings
//! Each thread carries an optional linked list of "frames". Each
//! frame is a `PersistentArrayMap` (or `PersistentHashMap` after
//! promotion) of `Var → value` plus a pointer to the previous
//! frame. `push_thread_bindings(map)` merges `map` into the current
//! top frame's bindings (via `rt::assoc`) and pushes a new frame
//! on top; `pop_thread_bindings()` discards the top frame and
//! restores the previous one. `deref` of a dynamic var consults
//! only the current top frame (a single `rt::get`) — no walking,
//! since merge-on-push is what JVM does too.
//!
//! # Identity
//! `IEquiv` and `IHash` are identity-based (heap pointer + tag) so
//! a Var can be used as a key in a `PersistentArrayMap` /
//! `PersistentHashMap`. Two Var instances are equal iff they're
//! the same heap object — matches JVM, where `Var` doesn't override
//! `equals`/`hashCode`.
//!
//! # Static globals
//! Idiomatic JVM vars are class-static: `public static final Var X =
//! Var.intern(...)`. The Rust analog is a `OnceLock<Value>`
//! initialized lazily (or in `init`):
//!
//! ```ignore
//! use std::sync::OnceLock;
//! pub static MY_VAR: OnceLock<clojure_rt::Value> = OnceLock::new();
//! pub fn my_var() -> clojure_rt::Value {
//!     *MY_VAR.get_or_init(|| {
//!         let sym = clojure_rt::rt::symbol(None, "my-var");
//!         let v = clojure_rt::types::var::Var::intern(
//!             clojure_rt::Value::NIL, sym, clojure_rt::Value::int(0),
//!         );
//!         clojure_rt::rc::drop_value(sym);
//!         v
//!     })
//! }
//! ```

use core::sync::atomic::{AtomicBool, Ordering};
use std::cell::RefCell;
use std::sync::Arc;

use arc_swap::ArcSwap;

use crate::protocols::deref::IDeref;
use crate::protocols::equiv::IEquiv;
use crate::protocols::hash::IHash;
use crate::protocols::meta::IMeta;
use crate::protocols::r#ref::IRef;
use crate::protocols::reference::IReference;
use crate::protocols::watchable::IWatchable;
use crate::value::Value;

pub(crate) struct VarCell {
    pub(crate) v: Value,
}

impl Drop for VarCell {
    fn drop(&mut self) {
        crate::rc::drop_value(self.v);
    }
}

clojure_rt_macros::register_type! {
    pub struct Var {
        root:      ArcSwap<VarCell>,
        meta:      ArcSwap<VarCell>,
        validator: ArcSwap<VarCell>,
        watches:   ArcSwap<VarCell>,
        ns:        Value,
        sym:       Value,
        dynamic:   AtomicBool,
    }
}

#[inline]
fn cell(v: Value) -> Arc<VarCell> {
    Arc::new(VarCell { v })
}

#[inline]
fn cell_dup(v: Value) -> Arc<VarCell> {
    crate::rc::dup(v);
    Arc::new(VarCell { v })
}

// --- Per-thread bindings ----------------------------------------------------

struct Frame {
    /// Merged bindings — a PHM/PAM of `Var → value`. Always the
    /// fully-merged view; `deref` consults only this map.
    bindings: Value,
    /// Pointer back to the next-outer frame (or None at the
    /// bottom). Linked-list shape so push/pop are O(1).
    prev: Option<Box<Frame>>,
}

thread_local! {
    static TOP_FRAME: RefCell<Option<Box<Frame>>>
        = const { RefCell::new(None) };
}

/// Push a new bindings frame for the current thread. `bindings` is
/// a map of `Var → value`; the new frame merges it onto the
/// previous frame's bindings via `rt::assoc` so a single map
/// lookup answers `deref` regardless of nesting depth.
///
/// Borrow semantics on `bindings`. The frame holds its own +1 ref
/// of the merged map.
pub fn push_thread_bindings(bindings: Value) {
    TOP_FRAME.with(|cell| {
        let mut top = cell.borrow_mut();
        let prev = top.take();
        // Start from the previous frame's merged view (or empty).
        let starting = match prev.as_ref() {
            Some(f) => {
                crate::rc::dup(f.bindings);
                f.bindings
            }
            None => crate::rt::array_map(&[]),
        };
        let mut merged = starting;
        let mut s = crate::rt::seq(bindings);
        while !s.is_nil() {
            let entry = crate::rt::first(s);
            let k = crate::rt::key(entry);
            let v = crate::rt::val(entry);
            let next_merged = crate::rt::assoc(merged, k, v);
            crate::rc::drop_value(merged);
            crate::rc::drop_value(k);
            crate::rc::drop_value(v);
            crate::rc::drop_value(entry);
            merged = next_merged;
            let n = crate::rt::next(s);
            crate::rc::drop_value(s);
            s = n;
        }
        crate::rc::drop_value(s);
        *top = Some(Box::new(Frame {
            bindings: merged,
            prev,
        }));
    });
}

/// Pop the top frame, restoring the previous one. No-op when the
/// stack is empty.
pub fn pop_thread_bindings() {
    TOP_FRAME.with(|cell| {
        let mut top = cell.borrow_mut();
        if let Some(boxed) = top.take() {
            let Frame { bindings, prev } = *boxed;
            crate::rc::drop_value(bindings);
            *top = prev;
        }
    });
}

/// Look up a thread binding for `var`. Returns the bound value
/// (with a fresh +1 ref) if found in the current top frame, or
/// `None` when no frame is active or `var` isn't bound there.
///
/// Distinguishes `present-with-nil-value` from `absent` via
/// `rt::contains_key` so a `(binding [*x* nil] ...)` doesn't fall
/// through to the root.
fn thread_binding(var: Value) -> Option<Value> {
    TOP_FRAME.with(|cell| {
        let top = cell.borrow();
        let frame = top.as_ref()?;
        if !crate::rt::contains_key(frame.bindings, var)
            .as_bool()
            .unwrap_or(false)
        {
            return None;
        }
        Some(crate::rt::get(frame.bindings, var))
    })
}

// --- Var construction + accessors ------------------------------------------

impl Var {
    /// `(intern ns sym root)` — build a fresh `Var` bound to
    /// `root`. `ns` and `sym` may be `Value::NIL` for an anonymous
    /// var (no namespace/symbol identification, useful for
    /// stand-alone references). Borrow semantics on all three args.
    pub fn intern(ns: Value, sym: Value, root: Value) -> Value {
        crate::rc::dup(ns);
        crate::rc::dup(sym);
        crate::rc::share(ns);
        crate::rc::share(sym);
        crate::rc::share(root);
        let v = Var::alloc(
            ArcSwap::from(cell_dup(root)),
            ArcSwap::from(cell(Value::NIL)),
            ArcSwap::from(cell(Value::NIL)),
            ArcSwap::from(cell(Value::NIL)),
            ns,
            sym,
            AtomicBool::new(false),
        );
        crate::rc::share(v);
        v
    }

    /// Mark `this` as `^:dynamic` — thread bindings are now honored
    /// on `deref`. Returns `this` for chaining (with a fresh ref).
    pub fn set_dynamic(this: Value) -> Value {
        let body = unsafe { Var::body(this) };
        body.dynamic.store(true, Ordering::Release);
        crate::rc::dup(this);
        this
    }

    pub fn is_dynamic(this: Value) -> bool {
        let body = unsafe { Var::body(this) };
        body.dynamic.load(Ordering::Acquire)
    }

    /// Read-only accessors — borrow semantics, returns dup'd refs.
    pub fn ns(this: Value) -> Value {
        let body = unsafe { Var::body(this) };
        crate::rc::dup(body.ns);
        body.ns
    }

    pub fn sym(this: Value) -> Value {
        let body = unsafe { Var::body(this) };
        crate::rc::dup(body.sym);
        body.sym
    }

    /// `(.bindRoot v new)` — install a new root, firing watches
    /// with the (old, new) pair. Validators run on the candidate
    /// before commit. Returns `Value::NIL` on success or an
    /// exception value on validator failure.
    pub fn bind_root(this: Value, new_root: Value) -> Value {
        if let Some(err) = run_validator(this, new_root) {
            return err;
        }
        let body = unsafe { Var::body(this) };
        crate::rc::share(new_root);
        let new_arc = cell_dup(new_root);
        let old_arc = body.root.swap(new_arc);
        fire_watches(this, old_arc.v, new_root);
        drop(old_arc);
        Value::NIL
    }

    /// `(alter-var-root v f args…)` — apply `f` to the current
    /// root + extra args, install the result, return the new root.
    /// CAS-retry under contention; validator + watches as for
    /// `bind_root`. Borrow semantics throughout.
    pub fn alter_root(this: Value, f: Value, args: &[Value]) -> Value {
        let body = unsafe { Var::body(this) };
        loop {
            let snap = body.root.load_full();
            let cur = snap.v;
            let mut call_args: Vec<Value> = Vec::with_capacity(1 + args.len());
            call_args.push(cur);
            call_args.extend_from_slice(args);
            let new_root = crate::rt::invoke(f, &call_args);
            if new_root.is_exception() {
                return new_root;
            }
            if let Some(err) = run_validator(this, new_root) {
                crate::rc::drop_value(new_root);
                return err;
            }
            crate::rc::share(new_root);
            let new_arc = cell(new_root);
            let witness = body.root.compare_and_swap(&snap, new_arc);
            if Arc::ptr_eq(&witness, &snap) {
                fire_watches(this, snap.v, new_root);
                crate::rc::dup(new_root);
                return new_root;
            }
        }
    }
}

// --- IDeref -----------------------------------------------------------------

clojure_rt_macros::implements! {
    impl IDeref for Var {
        fn deref(this: Value) -> Value {
            // Dynamic vars consult thread bindings first.
            let body = unsafe { Var::body(this) };
            if body.dynamic.load(Ordering::Acquire) {
                if let Some(bound) = thread_binding(this) {
                    return bound;
                }
            }
            let snap = body.root.load_full();
            let v = snap.v;
            crate::rc::dup(v);
            v
        }
    }
}

// --- Identity equiv + hash --------------------------------------------------

clojure_rt_macros::implements! {
    impl IEquiv for Var {
        fn equiv(this: Value, other: Value) -> Value {
            // Identity: same heap object iff payload + tag match.
            if this.tag == other.tag && this.payload == other.payload {
                Value::TRUE
            } else {
                Value::FALSE
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl IHash for Var {
        fn hash(this: Value) -> Value {
            // Identity hash — derived from the heap pointer. Mix
            // the low 32 bits of the payload to avoid clustering on
            // allocator stride.
            let p = this.payload as u64;
            let h = (p ^ (p >> 32)) as i32;
            Value::int(h as i64)
        }
    }
}

// --- IRef (validators) ------------------------------------------------------

clojure_rt_macros::implements! {
    impl IRef for Var {
        fn set_validator(this: Value, f: Value) -> Value {
            let body = unsafe { Var::body(this) };
            if !f.is_nil() {
                let snap = body.root.load_full();
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
            let body = unsafe { Var::body(this) };
            let snap = body.validator.load_full();
            let v = snap.v;
            crate::rc::dup(v);
            v
        }
    }
}

// --- IWatchable -------------------------------------------------------------

clojure_rt_macros::implements! {
    impl IWatchable for Var {
        fn add_watch(this: Value, key: Value, f: Value) -> Value {
            let body = unsafe { Var::body(this) };
            crate::rc::share(key);
            crate::rc::share(f);
            loop {
                let snap = body.watches.load_full();
                let cur = snap.v;
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
            let body = unsafe { Var::body(this) };
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
    impl IMeta for Var {
        fn meta(this: Value) -> Value {
            let body = unsafe { Var::body(this) };
            let snap = body.meta.load_full();
            let m = snap.v;
            crate::rc::dup(m);
            m
        }
    }
}

clojure_rt_macros::implements! {
    impl IReference for Var {
        fn reset_meta(this: Value, m: Value) -> Value {
            let body = unsafe { Var::body(this) };
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

fn run_validator(this: Value, candidate: Value) -> Option<Value> {
    let body = unsafe { Var::body(this) };
    let snap = body.validator.load_full();
    let validator = snap.v;
    if validator.is_nil() {
        return None;
    }
    let r = crate::rt::invoke(validator, &[candidate]);
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

fn fire_watches(this: Value, old: Value, new: Value) -> Value {
    let body = unsafe { Var::body(this) };
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

fn alter_meta_impl(this: Value, f: Value, extra: &[Value]) -> Value {
    let body = unsafe { Var::body(this) };
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
