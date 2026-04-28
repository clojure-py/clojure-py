//! `Cons` — a single cons cell that prepends `first` onto an
//! arbitrary `rest` seqable. Used as the result of `(cons x coll)`
//! whenever `coll` isn't already a `PersistentList` (which has its
//! own count-tracked cons-cell shape).
//!
//! Mirrors `clojure.lang.Cons` (JVM) and cljs's `Cons`. The key
//! difference from `PersistentList`:
//! - `Cons` doesn't track count — `rest` may be a lazy/infinite seq.
//! - `Cons` doesn't have an "empty" variant; it always has a `first`.
//!   When constructed with `rest = nil`, `ISeq::rest` returns the
//!   canonical empty list (matching JVM `Cons.more()` returning
//!   `PersistentList.EMPTY`).

use core::sync::atomic::{AtomicI32, Ordering};

use crate::hash::murmur3;
use crate::protocols::equiv::IEquiv;
use crate::protocols::hash::IHash;
use crate::protocols::meta::{IMeta, IWithMeta};
use crate::protocols::seq::{INext, ISeq, ISeqable};
use crate::protocols::sequential::ISequential;
use crate::value::Value;

clojure_rt_macros::register_type! {
    pub struct Cons {
        first: Value,
        rest:  Value,    // arbitrary seqable; nil treated as empty
        meta:  Value,
        hash:  AtomicI32, // 0 = uncomputed
    }
}

impl Cons {
    /// `(cons x coll)` shape. Borrow semantics: caller's `first` and
    /// `rest` refs are unchanged; the new cons dups both.
    pub fn new(first: Value, rest: Value) -> Value {
        crate::rc::dup(first);
        crate::rc::dup(rest);
        Cons::alloc(first, rest, Value::NIL, AtomicI32::new(0))
    }
}

clojure_rt_macros::implements! {
    impl ISeq for Cons {
        fn first(this: Value) -> Value {
            let v = unsafe { Cons::body(this) }.first;
            crate::rc::dup(v);
            v
        }
        fn rest(this: Value) -> Value {
            let r = unsafe { Cons::body(this) }.rest;
            if r.is_nil() {
                crate::types::list::empty_list()
            } else {
                crate::rc::dup(r);
                r
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl INext for Cons {
        fn next(this: Value) -> Value {
            let r = unsafe { Cons::body(this) }.rest;
            if r.is_nil() {
                Value::NIL
            } else {
                // Canonicalize via seq — handles lazy chains and
                // returns nil for empty seqs.
                crate::rt::seq(r)
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl ISeqable for Cons {
        fn seq(this: Value) -> Value {
            crate::rc::dup(this);
            this
        }
    }
}

clojure_rt_macros::implements! {
    impl IMeta for Cons {
        fn meta(this: Value) -> Value {
            let m = unsafe { Cons::body(this) }.meta;
            crate::rc::dup(m);
            m
        }
    }
}

clojure_rt_macros::implements! {
    impl IWithMeta for Cons {
        fn with_meta(this: Value, meta: Value) -> Value {
            let body = unsafe { Cons::body(this) };
            crate::rc::dup(body.first);
            crate::rc::dup(body.rest);
            crate::rc::dup(meta);
            Cons::alloc(body.first, body.rest, meta, AtomicI32::new(0))
        }
    }
}

clojure_rt_macros::implements! {
    impl IHash for Cons {
        fn hash(this: Value) -> Value {
            let body = unsafe { Cons::body(this) };
            let cached = body.hash.load(Ordering::Relaxed);
            if cached != 0 {
                return Value::int(cached as i64);
            }
            let h = generic_seq_hash(this);
            body.hash.store(h, Ordering::Relaxed);
            Value::int(h as i64)
        }
    }
}

clojure_rt_macros::implements! {
    impl IEquiv for Cons {
        fn equiv(this: Value, other: Value) -> Value {
            // Same-type only for now. Cross-type sequential equiv
            // (Cons vs PersistentList vs LazySeq vs VecSeq) is the
            // same deferred work as elsewhere — will land when we
            // do the unified ISequential equiv layer.
            if other.tag != this.tag {
                return Value::FALSE;
            }
            if generic_seq_equiv(this, other) { Value::TRUE } else { Value::FALSE }
        }
    }
}

clojure_rt_macros::implements! { impl ISequential for Cons {} }

// ============================================================================
// Generic seq walks via rt::first / rt::next. Used by hash + equiv
// here; LazySeq uses the same pattern.
// ============================================================================

pub(crate) fn generic_seq_hash(start: Value) -> i32 {
    let mut hash: i32 = 1;
    let mut n: i32 = 0;
    let mut cur = start;
    crate::rc::dup(cur);
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
    murmur3::mix_coll_hash(hash, n)
}

pub(crate) fn generic_seq_equiv(a: Value, b: Value) -> bool {
    let mut x = a;
    let mut y = b;
    crate::rc::dup(x);
    crate::rc::dup(y);
    loop {
        let x_nil = x.is_nil();
        let y_nil = y.is_nil();
        if x_nil && y_nil {
            return true;
        }
        if x_nil || y_nil {
            crate::rc::drop_value(x);
            crate::rc::drop_value(y);
            return false;
        }
        let xf = crate::rt::first(x);
        let yf = crate::rt::first(y);
        let eq = crate::rt::equiv(xf, yf).as_bool().unwrap_or(false);
        crate::rc::drop_value(xf);
        crate::rc::drop_value(yf);
        if !eq {
            crate::rc::drop_value(x);
            crate::rc::drop_value(y);
            return false;
        }
        let xn = crate::rt::next(x);
        let yn = crate::rt::next(y);
        crate::rc::drop_value(x);
        crate::rc::drop_value(y);
        x = xn;
        y = yn;
    }
}
