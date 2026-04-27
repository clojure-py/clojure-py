//! Linear seqs over a `PersistentVector`. Two variants:
//!
//! - `VecSeq`  — forward walk, index `i` increments with each `rest`.
//! - `VecRSeq` — reverse walk, index `i` decrements with each `rest`.
//!
//! Both hold a strong ref to the underlying vector so element walks
//! stay valid for the seq's lifetime. Chunked variants (`IChunkedSeq`,
//! `chunked-first`/`chunked-next`) are deferred to a follow-up; this
//! implementation walks one element at a time via `nth`.

use core::sync::atomic::{AtomicI32, Ordering};

use crate::hash::murmur3;
use crate::protocols::counted::ICounted;
use crate::protocols::equiv::IEquiv;
use crate::protocols::hash::IHash;
use crate::protocols::meta::{IMeta, IWithMeta};
use crate::protocols::seq::{INext, ISeq, ISeqable};
use crate::protocols::sequential::ISequential;
use crate::types::vector::PersistentVector;
use crate::value::Value;

clojure_rt_macros::register_type! {
    pub struct VecSeq {
        vec:   Value,       // PersistentVector — held alive for the seq's lifetime
        index: i64,         // 0..count; never reaches count (empty seqs are NIL)
        meta:  Value,
        hash:  AtomicI32,   // 0 = uncomputed
    }
}

clojure_rt_macros::register_type! {
    pub struct VecRSeq {
        vec:   Value,       // PersistentVector
        index: i64,         // count-1 down to 0
        meta:  Value,
        hash:  AtomicI32,
    }
}

impl VecSeq {
    /// Create a seq positioned at the start of `vec`. Caller's
    /// reference to `vec` is *not* consumed; this dup-s into the seq.
    pub fn start(vec: Value) -> Value {
        crate::rc::dup(vec);
        VecSeq::alloc(vec, 0, Value::NIL, AtomicI32::new(0))
    }
}

impl VecRSeq {
    /// Create a reverse seq positioned at the last element of `vec`.
    pub fn start(vec: Value) -> Value {
        let body_count = PersistentVector::count_of(vec);
        debug_assert!(body_count > 0, "VecRSeq::start: empty vector");
        crate::rc::dup(vec);
        VecRSeq::alloc(vec, body_count - 1, Value::NIL, AtomicI32::new(0))
    }
}

// ============================================================================
// VecSeq — forward
// ============================================================================

clojure_rt_macros::implements! {
    impl ISeq for VecSeq {
        fn first(this: Value) -> Value {
            let body = unsafe { VecSeq::body(this) };
            PersistentVector::nth(body.vec, body.index)
                .expect("VecSeq invariant: index in range")
        }
        fn rest(this: Value) -> Value {
            let body = unsafe { VecSeq::body(this) };
            let count = PersistentVector::count_of(body.vec);
            if body.index + 1 >= count {
                crate::types::list::empty_list()
            } else {
                crate::rc::dup(body.vec);
                VecSeq::alloc(body.vec, body.index + 1, Value::NIL, AtomicI32::new(0))
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl INext for VecSeq {
        fn next(this: Value) -> Value {
            let body = unsafe { VecSeq::body(this) };
            let count = PersistentVector::count_of(body.vec);
            if body.index + 1 >= count {
                Value::NIL
            } else {
                crate::rc::dup(body.vec);
                VecSeq::alloc(body.vec, body.index + 1, Value::NIL, AtomicI32::new(0))
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl ISeqable for VecSeq {
        fn seq(this: Value) -> Value {
            crate::rc::dup(this);
            this
        }
    }
}

clojure_rt_macros::implements! {
    impl ICounted for VecSeq {
        fn count(this: Value) -> Value {
            let body = unsafe { VecSeq::body(this) };
            let count = PersistentVector::count_of(body.vec);
            Value::int(count - body.index)
        }
    }
}

clojure_rt_macros::implements! {
    impl IMeta for VecSeq {
        fn meta(this: Value) -> Value {
            let m = unsafe { VecSeq::body(this) }.meta;
            crate::rc::dup(m);
            m
        }
    }
}

clojure_rt_macros::implements! {
    impl IWithMeta for VecSeq {
        fn with_meta(this: Value, meta: Value) -> Value {
            let body = unsafe { VecSeq::body(this) };
            crate::rc::dup(body.vec);
            crate::rc::dup(meta);
            VecSeq::alloc(body.vec, body.index, meta, AtomicI32::new(0))
        }
    }
}

clojure_rt_macros::implements! {
    impl IHash for VecSeq {
        fn hash(this: Value) -> Value {
            let body = unsafe { VecSeq::body(this) };
            let cached = body.hash.load(Ordering::Relaxed);
            if cached != 0 {
                return Value::int(cached as i64);
            }
            let h = compute_seq_hash_forward(body.vec, body.index);
            body.hash.store(h, Ordering::Relaxed);
            Value::int(h as i64)
        }
    }
}

clojure_rt_macros::implements! {
    impl IEquiv for VecSeq {
        fn equiv(this: Value, other: Value) -> Value {
            if other.tag != this.tag {
                return Value::FALSE;
            }
            if seqs_forward_equiv(this, other) { Value::TRUE } else { Value::FALSE }
        }
    }
}

clojure_rt_macros::implements! { impl ISequential for VecSeq {} }

// ============================================================================
// VecRSeq — reverse
// ============================================================================

clojure_rt_macros::implements! {
    impl ISeq for VecRSeq {
        fn first(this: Value) -> Value {
            let body = unsafe { VecRSeq::body(this) };
            PersistentVector::nth(body.vec, body.index)
                .expect("VecRSeq invariant: index in range")
        }
        fn rest(this: Value) -> Value {
            let body = unsafe { VecRSeq::body(this) };
            if body.index == 0 {
                crate::types::list::empty_list()
            } else {
                crate::rc::dup(body.vec);
                VecRSeq::alloc(body.vec, body.index - 1, Value::NIL, AtomicI32::new(0))
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl INext for VecRSeq {
        fn next(this: Value) -> Value {
            let body = unsafe { VecRSeq::body(this) };
            if body.index == 0 {
                Value::NIL
            } else {
                crate::rc::dup(body.vec);
                VecRSeq::alloc(body.vec, body.index - 1, Value::NIL, AtomicI32::new(0))
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl ISeqable for VecRSeq {
        fn seq(this: Value) -> Value {
            crate::rc::dup(this);
            this
        }
    }
}

clojure_rt_macros::implements! {
    impl ICounted for VecRSeq {
        fn count(this: Value) -> Value {
            Value::int(unsafe { VecRSeq::body(this) }.index + 1)
        }
    }
}

clojure_rt_macros::implements! {
    impl IMeta for VecRSeq {
        fn meta(this: Value) -> Value {
            let m = unsafe { VecRSeq::body(this) }.meta;
            crate::rc::dup(m);
            m
        }
    }
}

clojure_rt_macros::implements! {
    impl IWithMeta for VecRSeq {
        fn with_meta(this: Value, meta: Value) -> Value {
            let body = unsafe { VecRSeq::body(this) };
            crate::rc::dup(body.vec);
            crate::rc::dup(meta);
            VecRSeq::alloc(body.vec, body.index, meta, AtomicI32::new(0))
        }
    }
}

clojure_rt_macros::implements! {
    impl IHash for VecRSeq {
        fn hash(this: Value) -> Value {
            let body = unsafe { VecRSeq::body(this) };
            let cached = body.hash.load(Ordering::Relaxed);
            if cached != 0 {
                return Value::int(cached as i64);
            }
            let h = compute_seq_hash_reverse(body.vec, body.index);
            body.hash.store(h, Ordering::Relaxed);
            Value::int(h as i64)
        }
    }
}

clojure_rt_macros::implements! {
    impl IEquiv for VecRSeq {
        fn equiv(this: Value, other: Value) -> Value {
            if other.tag != this.tag {
                return Value::FALSE;
            }
            if seqs_reverse_equiv(this, other) { Value::TRUE } else { Value::FALSE }
        }
    }
}

clojure_rt_macros::implements! { impl ISequential for VecRSeq {} }

// ============================================================================
// Internal helpers
// ============================================================================

fn compute_seq_hash_forward(vec: Value, start: i64) -> i32 {
    let count = PersistentVector::count_of(vec);
    let mut hashes: Vec<i32> = Vec::with_capacity((count - start) as usize);
    let mut i = start;
    while i < count {
        let v = PersistentVector::nth(vec, i).expect("range");
        let h = clojure_rt_macros::dispatch!(IHash::hash, &[v]).as_int().unwrap_or(0) as i32;
        hashes.push(h);
        crate::rc::drop_value(v);
        i += 1;
    }
    murmur3::hash_ordered(hashes)
}

fn compute_seq_hash_reverse(vec: Value, start: i64) -> i32 {
    let mut hashes: Vec<i32> = Vec::with_capacity((start + 1) as usize);
    let mut i = start;
    loop {
        let v = PersistentVector::nth(vec, i).expect("range");
        let h = clojure_rt_macros::dispatch!(IHash::hash, &[v]).as_int().unwrap_or(0) as i32;
        hashes.push(h);
        crate::rc::drop_value(v);
        if i == 0 { break; }
        i -= 1;
    }
    murmur3::hash_ordered(hashes)
}

fn seqs_forward_equiv(a: Value, b: Value) -> bool {
    let ab = unsafe { VecSeq::body(a) };
    let bb = unsafe { VecSeq::body(b) };
    let ac = PersistentVector::count_of(ab.vec) - ab.index;
    let bc = PersistentVector::count_of(bb.vec) - bb.index;
    if ac != bc {
        return false;
    }
    let mut i = 0i64;
    while i < ac {
        let x = PersistentVector::nth(ab.vec, ab.index + i).expect("range");
        let y = PersistentVector::nth(bb.vec, bb.index + i).expect("range");
        let eq = clojure_rt_macros::dispatch!(IEquiv::equiv, &[x, y])
            .as_bool().unwrap_or(false);
        crate::rc::drop_value(x);
        crate::rc::drop_value(y);
        if !eq { return false; }
        i += 1;
    }
    true
}

fn seqs_reverse_equiv(a: Value, b: Value) -> bool {
    let ab = unsafe { VecRSeq::body(a) };
    let bb = unsafe { VecRSeq::body(b) };
    if ab.index != bb.index {
        return false;
    }
    let mut i = ab.index;
    loop {
        let x = PersistentVector::nth(ab.vec, i).expect("range");
        let y = PersistentVector::nth(bb.vec, i).expect("range");
        let eq = clojure_rt_macros::dispatch!(IEquiv::equiv, &[x, y])
            .as_bool().unwrap_or(false);
        crate::rc::drop_value(x);
        crate::rc::drop_value(y);
        if !eq { return false; }
        if i == 0 { break; }
        i -= 1;
    }
    true
}
