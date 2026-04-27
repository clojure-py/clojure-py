//! Persistent list — `EmptyList` (singleton, with optional meta) plus
//! `ConsObj` cons-cells. Eager only; `LazySeq` is its own design.
//!
//! The "empty list" is an actual heap-allocated value, not
//! `Value::NIL`. Using nil for empty would force tag-case-analysis in
//! `rt::*` helpers (the same double-polymorphism we already
//! rejected). With a singleton + per-type protocol impls, dispatch
//! routes naturally.

use core::sync::atomic::{AtomicI32, Ordering};
use std::sync::OnceLock;

use crate::hash::murmur3;
use crate::protocols::coll::{ICollection, IEmptyableCollection, IStack};
use crate::protocols::counted::ICounted;
use crate::protocols::equiv::IEquiv;
use crate::protocols::hash::IHash;
use crate::protocols::meta::{IMeta, IWithMeta};
use crate::protocols::seq::{INext, ISeq, ISeqable};
use crate::protocols::sequential::ISequential;
use crate::value::Value;

clojure_rt_macros::register_type! {
    pub struct EmptyList {
        meta: Value,
    }
}

clojure_rt_macros::register_type! {
    pub struct ConsObj {
        first: Value,
        rest:  Value,    // ConsObj or EmptyList — never NIL
        meta:  Value,
        count: i64,
        hash:  AtomicI32, // 0 = uncomputed
    }
}

/// The canonical empty list with no metadata. Other empty lists with
/// non-nil meta are fresh allocations (matches JVM `PersistentList$EmptyList`,
/// where every meta-replacing `withMeta` produces a new instance).
static EMPTY_LIST_SINGLETON: OnceLock<Value> = OnceLock::new();

/// Canonical empty-list `Value` with no meta. Auto-initializes on
/// first call after `clojure_rt::init` has run. Increments the
/// refcount before returning, so callers should `drop_value` when
/// they're done.
pub fn empty_list() -> Value {
    let v = *EMPTY_LIST_SINGLETON.get_or_init(|| EmptyList::alloc(Value::NIL));
    crate::rc::dup(v);
    v
}

impl ConsObj {
    /// Wrap `first` onto the head of `rest`. `rest` must be a list-shaped
    /// `Value` (ConsObj or EmptyList); to cons onto an arbitrary
    /// seqable, callers should run it through `rt::seq` first or use
    /// `rt::cons` which does the coercion.
    pub fn cons(first: Value, rest: Value) -> Value {
        crate::rc::dup(first);
        crate::rc::dup(rest);
        let count = list_count(rest) + 1;
        Self::alloc(first, rest, Value::NIL, count, AtomicI32::new(0))
    }

    /// Build a list from a slice of `Value`s, right-to-left. Each
    /// element's refcount is bumped (the new ConsObjs hold the refs).
    pub fn list(items: &[Value]) -> Value {
        let mut acc = empty_list();
        for &item in items.iter().rev() {
            let new = Self::cons(item, acc);
            crate::rc::drop_value(acc);
            acc = new;
        }
        acc
    }
}

/// O(1) count for a list-shaped Value. Internal helper; assumes `v`
/// is either `EmptyList` or `ConsObj`.
fn list_count(v: Value) -> i64 {
    if v.tag == empty_list_type_id() {
        0
    } else {
        unsafe { ConsObj::body(v) }.count
    }
}

fn empty_list_type_id() -> crate::value::TypeId {
    *EMPTYLIST_TYPE_ID.get().expect("EmptyList: clojure_rt::init() not called")
}

fn cons_type_id() -> crate::value::TypeId {
    *CONSOBJ_TYPE_ID.get().expect("ConsObj: clojure_rt::init() not called")
}

// ============================================================================
// EmptyList impls
// ============================================================================

clojure_rt_macros::implements! {
    impl ICounted for EmptyList {
        fn count(this: Value) -> Value {
            let _ = this;
            Value::int(0)
        }
    }
}

clojure_rt_macros::implements! {
    impl ISeqable for EmptyList {
        fn seq(this: Value) -> Value {
            let _ = this;
            Value::NIL
        }
    }
}

clojure_rt_macros::implements! {
    impl ISeq for EmptyList {
        fn first(this: Value) -> Value {
            let _ = this;
            Value::NIL
        }
        fn rest(this: Value) -> Value {
            crate::rc::dup(this);
            this
        }
    }
}

clojure_rt_macros::implements! {
    impl INext for EmptyList {
        fn next(this: Value) -> Value {
            let _ = this;
            Value::NIL
        }
    }
}

clojure_rt_macros::implements! {
    impl ICollection for EmptyList {
        fn conj(this: Value, x: Value) -> Value {
            ConsObj::cons(x, this)
        }
    }
}

clojure_rt_macros::implements! {
    impl IEmptyableCollection for EmptyList {
        fn empty(this: Value) -> Value {
            crate::rc::dup(this);
            this
        }
    }
}

clojure_rt_macros::implements! {
    impl IStack for EmptyList {
        fn peek(this: Value) -> Value {
            let _ = this;
            Value::NIL
        }
        fn pop(this: Value) -> Value {
            let _ = this;
            crate::exception::make_foreign(
                "Can't pop empty list".to_string(),
            )
        }
    }
}

clojure_rt_macros::implements! {
    impl IHash for EmptyList {
        fn hash(this: Value) -> Value {
            let _ = this;
            // hash_ordered over an empty iterator: hash=1, n=0, then mix.
            Value::int(murmur3::mix_coll_hash(1, 0) as i64)
        }
    }
}

clojure_rt_macros::implements! {
    impl IEquiv for EmptyList {
        fn equiv(this: Value, other: Value) -> Value {
            // Equal to any other EmptyList (including with-meta variants)
            // and to any zero-element ConsObj (none can exist by
            // construction, but the count-0 check is defensive).
            if other.tag == this.tag {
                Value::TRUE
            } else if other.tag == cons_type_id() {
                // Defensive: a ConsObj is non-empty by construction.
                Value::FALSE
            } else {
                Value::FALSE
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl IMeta for EmptyList {
        fn meta(this: Value) -> Value {
            let m = unsafe { EmptyList::body(this) }.meta;
            crate::rc::dup(m);
            m
        }
    }
}

clojure_rt_macros::implements! {
    impl IWithMeta for EmptyList {
        fn with_meta(this: Value, meta: Value) -> Value {
            let _ = this;
            crate::rc::dup(meta);
            EmptyList::alloc(meta)
        }
    }
}

clojure_rt_macros::implements! {
    impl ISequential for EmptyList {}
}

// ============================================================================
// ConsObj impls
// ============================================================================

clojure_rt_macros::implements! {
    impl ICounted for ConsObj {
        fn count(this: Value) -> Value {
            Value::int(unsafe { ConsObj::body(this) }.count)
        }
    }
}

clojure_rt_macros::implements! {
    impl ISeqable for ConsObj {
        fn seq(this: Value) -> Value {
            crate::rc::dup(this);
            this
        }
    }
}

clojure_rt_macros::implements! {
    impl ISeq for ConsObj {
        fn first(this: Value) -> Value {
            let v = unsafe { ConsObj::body(this) }.first;
            crate::rc::dup(v);
            v
        }
        fn rest(this: Value) -> Value {
            let v = unsafe { ConsObj::body(this) }.rest;
            crate::rc::dup(v);
            v
        }
    }
}

clojure_rt_macros::implements! {
    impl INext for ConsObj {
        fn next(this: Value) -> Value {
            let r = unsafe { ConsObj::body(this) }.rest;
            if r.tag == empty_list_type_id() {
                Value::NIL
            } else {
                crate::rc::dup(r);
                r
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl ICollection for ConsObj {
        fn conj(this: Value, x: Value) -> Value {
            // (conj '(2 3) 1) => (1 2 3) — prepend.
            ConsObj::cons(x, this)
        }
    }
}

clojure_rt_macros::implements! {
    impl IEmptyableCollection for ConsObj {
        fn empty(this: Value) -> Value {
            let _ = this;
            empty_list()
        }
    }
}

clojure_rt_macros::implements! {
    impl IStack for ConsObj {
        fn peek(this: Value) -> Value {
            let v = unsafe { ConsObj::body(this) }.first;
            crate::rc::dup(v);
            v
        }
        fn pop(this: Value) -> Value {
            let v = unsafe { ConsObj::body(this) }.rest;
            crate::rc::dup(v);
            v
        }
    }
}

clojure_rt_macros::implements! {
    impl IHash for ConsObj {
        fn hash(this: Value) -> Value {
            let body = unsafe { ConsObj::body(this) };
            let cached = body.hash.load(Ordering::Relaxed);
            if cached != 0 {
                return Value::int(cached as i64);
            }
            let h = compute_seq_hash(this);
            body.hash.store(h, Ordering::Relaxed);
            Value::int(h as i64)
        }
    }
}

clojure_rt_macros::implements! {
    impl IEquiv for ConsObj {
        fn equiv(this: Value, other: Value) -> Value {
            if other.tag != this.tag {
                // Cross-type sequential equiv (e.g. list vs vector)
                // is deferred — see plan's "Sequential-equiv across
                // collection types" out-of-scope note.
                return Value::FALSE;
            }
            if seqs_equiv(this, other) {
                Value::TRUE
            } else {
                Value::FALSE
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl IMeta for ConsObj {
        fn meta(this: Value) -> Value {
            let m = unsafe { ConsObj::body(this) }.meta;
            crate::rc::dup(m);
            m
        }
    }
}

clojure_rt_macros::implements! {
    impl IWithMeta for ConsObj {
        fn with_meta(this: Value, meta: Value) -> Value {
            let body = unsafe { ConsObj::body(this) };
            crate::rc::dup(body.first);
            crate::rc::dup(body.rest);
            crate::rc::dup(meta);
            ConsObj::alloc(body.first, body.rest, meta, body.count, AtomicI32::new(0))
        }
    }
}

clojure_rt_macros::implements! {
    impl ISequential for ConsObj {}
}

// ============================================================================
// Internal helpers — seq walks for hash and equiv
// ============================================================================

fn compute_seq_hash(start: Value) -> i32 {
    // Walk the seq, collect element hashes, feed into hash_ordered.
    let mut hashes: Vec<i32> = Vec::new();
    let mut cur = start;
    let empty_id = empty_list_type_id();
    while cur.tag != empty_id {
        let body = unsafe { ConsObj::body(cur) };
        let elem_hash = clojure_rt_macros::dispatch!(IHash::hash, &[body.first])
            .as_int()
            .unwrap_or(0) as i32;
        hashes.push(elem_hash);
        cur = body.rest;
    }
    murmur3::hash_ordered(hashes)
}

fn seqs_equiv(a: Value, b: Value) -> bool {
    // Both are ConsObjs (caller ensured). Walk both in lockstep.
    let mut x = a;
    let mut y = b;
    let empty_id = empty_list_type_id();
    loop {
        let x_empty = x.tag == empty_id;
        let y_empty = y.tag == empty_id;
        if x_empty && y_empty {
            return true;
        }
        if x_empty || y_empty {
            return false;
        }
        let xb = unsafe { ConsObj::body(x) };
        let yb = unsafe { ConsObj::body(y) };
        let eq = clojure_rt_macros::dispatch!(IEquiv::equiv, &[xb.first, yb.first])
            .as_bool()
            .unwrap_or(false);
        if !eq {
            return false;
        }
        x = xb.rest;
        y = yb.rest;
    }
}
