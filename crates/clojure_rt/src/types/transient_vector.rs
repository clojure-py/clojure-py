//! `TransientVector` — single-thread mutable view of a
//! `PersistentVector`. Mirrors `clojure.lang.PersistentVector$
//! TransientVector` (JVM) and cljs's `TransientVector`.
//!
//! Storage layout matches the persistent shape: count, shift,
//! root (Arc<PVNode>), tail (Vec<Value>), plus an `alive` flag.
//! `tail` is a `Vec<Value>` — mutable in place — so the hot
//! batch-`conj!` case never path-copies. Trie-side operations
//! (`assoc!` of a trie-position index, `pop!` past a tail boundary)
//! currently fall back to persistent-style path-copy; switching to
//! `Arc::get_mut`-based in-place editing of uniquely-owned trie
//! nodes is a follow-up perf step.
//!
//! `persistent!` flips `alive` to false and converts the body back
//! to a `PersistentVector`. Subsequent mutation calls throw.

use core::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::Arc;

use crate::protocols::counted::ICounted;
use crate::protocols::indexed::IIndexed;
use crate::protocols::transient_associative::ITransientAssociative;
use crate::protocols::transient_collection::ITransientCollection;
use crate::protocols::transient_vector::ITransientVector;
use crate::types::vector::PersistentVector;
use crate::value::Value;

clojure_rt_macros::register_type! {
    pub struct TransientVector {
        count: i64,
        shift: i32,
        root:  Arc<crate::types::vector::PVNode>,
        tail:  Vec<Value>,
        alive: AtomicBool,
    }
}

impl TransientVector {
    /// Build a fresh transient from a persistent. Trie root Arc is
    /// shared (cheap clone); tail elements are dup'd into the
    /// transient's owned `Vec`. Caller's ref to the persistent is
    /// unchanged.
    pub fn from_persistent(p: Value) -> Value {
        let (count, shift, root, p_tail) = PersistentVector::parts(p);
        let mut tail: Vec<Value> = Vec::with_capacity(32);
        for &t in p_tail.iter() {
            crate::rc::dup(t);
            tail.push(t);
        }
        TransientVector::alloc(
            count,
            shift,
            root.clone(),
            tail,
            AtomicBool::new(true),
        )
    }

    fn ensure_alive(this: Value) -> Result<(), Value> {
        let body = unsafe { TransientVector::body(this) };
        if !body.alive.load(Ordering::Relaxed) {
            return Err(crate::exception::make_foreign(
                "Transient used after persistent!".to_string(),
            ));
        }
        Ok(())
    }
}

clojure_rt_macros::implements! {
    impl ITransientCollection for TransientVector {
        fn conj_bang(this: Value, x: Value) -> Value {
            if let Err(e) = TransientVector::ensure_alive(this) { return e; }
            let body = unsafe { TransientVector::body_mut(this) };

            if body.tail.len() < 32 {
                crate::rc::dup(x);
                body.tail.push(x);
                body.count += 1;
                crate::rc::dup(this);
                return this;
            }

            // Tail full — promote it into the trie. We rebuild a
            // PersistentVector from the transient's current state,
            // call its `cons` (which path-copies), then re-extract
            // the new state into the transient. This is the
            // path-copy fallback for the trie side; the tail-
            // append fast path above is what makes batch conj! a
            // win even with this implementation.
            let snapshot = TransientVector::snapshot_as_persistent(this);
            let next = PersistentVector::cons(snapshot, x);
            crate::rc::drop_value(snapshot);
            // Update transient's state from the new persistent.
            TransientVector::overwrite_from_persistent(this, next);
            crate::rc::drop_value(next);
            crate::rc::dup(this);
            this
        }
        fn persistent_bang(this: Value) -> Value {
            if let Err(e) = TransientVector::ensure_alive(this) { return e; }
            let body = unsafe { TransientVector::body_mut(this) };
            body.alive.store(false, Ordering::Relaxed);

            let count = body.count;
            let shift = body.shift;
            let root = body.root.clone();
            // Drain the transient's tail into a fresh Box<[Value]>.
            // The Vec gives up ownership of every element ref; the
            // new persistent's body takes those refs without re-dup.
            let tail = std::mem::take(&mut body.tail).into_boxed_slice();

            crate::types::vector::PersistentVector::from_owned_parts(
                count, shift, root, tail,
            )
        }
    }
}

clojure_rt_macros::implements! {
    impl ITransientAssociative for TransientVector {
        fn assoc_bang(this: Value, k: Value, v: Value) -> Value {
            if let Err(e) = TransientVector::ensure_alive(this) { return e; }
            let i = match k.as_int() {
                Some(i) => i,
                None => return crate::exception::make_foreign(
                    "Vector key must be an integer".to_string(),
                ),
            };
            let body = unsafe { TransientVector::body_mut(this) };
            if i < 0 || i > body.count {
                return crate::exception::make_foreign(format!(
                    "Index {} out of bounds for transient vector of size {}",
                    i, body.count,
                ));
            }
            if i == body.count {
                return clojure_rt_macros::dispatch!(
                    ITransientCollection::conj_bang, &[this, v]
                );
            }
            let tail_off = body.count - body.tail.len() as i64;
            if i >= tail_off {
                // In-tail: mutate Vec slot in place.
                let slot = (i - tail_off) as usize;
                let old = body.tail[slot];
                crate::rc::dup(v);
                body.tail[slot] = v;
                crate::rc::drop_value(old);
                crate::rc::dup(this);
                return this;
            }
            // Trie-position: path-copy via the persistent assoc, then
            // overwrite transient state. (Future: in-place via
            // Arc::get_mut on uniquely-owned subtrees.)
            let snapshot = TransientVector::snapshot_as_persistent(this);
            let next = PersistentVector::assoc(snapshot, i, v);
            crate::rc::drop_value(snapshot);
            TransientVector::overwrite_from_persistent(this, next);
            crate::rc::drop_value(next);
            crate::rc::dup(this);
            this
        }
    }
}

clojure_rt_macros::implements! {
    impl ITransientVector for TransientVector {
        fn pop_bang(this: Value) -> Value {
            if let Err(e) = TransientVector::ensure_alive(this) { return e; }
            let body = unsafe { TransientVector::body_mut(this) };
            if body.count == 0 {
                return crate::exception::make_foreign(
                    "Can't pop empty transient vector".to_string(),
                );
            }
            if body.tail.len() > 1 {
                let last = body.tail.pop().unwrap();
                crate::rc::drop_value(last);
                body.count -= 1;
                crate::rc::dup(this);
                return this;
            }
            if body.count == 1 {
                // Only one elem total — in the tail.
                let last = body.tail.pop().unwrap();
                crate::rc::drop_value(last);
                body.count = 0;
                crate::rc::dup(this);
                return this;
            }
            // Tail has 1 elem; trie has more. Path-copy via the
            // persistent pop, then overwrite transient state.
            let snapshot = TransientVector::snapshot_as_persistent(this);
            let next = PersistentVector::pop(snapshot);
            crate::rc::drop_value(snapshot);
            TransientVector::overwrite_from_persistent(this, next);
            crate::rc::drop_value(next);
            crate::rc::dup(this);
            this
        }
    }
}

clojure_rt_macros::implements! {
    impl ICounted for TransientVector {
        fn count(this: Value) -> Value {
            let body = unsafe { TransientVector::body(this) };
            Value::int(body.count)
        }
    }
}

clojure_rt_macros::implements! {
    impl IIndexed for TransientVector {
        fn nth_2(this: Value, n: Value) -> Value {
            let i = match n.as_int() {
                Some(i) => i,
                None => return crate::exception::make_foreign(
                    "nth: index must be integer".to_string(),
                ),
            };
            let body = unsafe { TransientVector::body(this) };
            if i < 0 || i >= body.count {
                return crate::exception::make_foreign(format!(
                    "Index {} out of bounds for transient vector of size {}",
                    i, body.count,
                ));
            }
            // Borrow into tail or trie via a transient snapshot.
            // (TransientVector currently uses the same trie storage
            // as persistent, so we can read directly.)
            transient_nth_borrowed(body, i)
        }
        fn nth_3(this: Value, n: Value, not_found: Value) -> Value {
            let body = unsafe { TransientVector::body(this) };
            let Some(i) = n.as_int() else {
                crate::rc::dup(not_found);
                return not_found;
            };
            if i < 0 || i >= body.count {
                crate::rc::dup(not_found);
                return not_found;
            }
            transient_nth_borrowed(body, i)
        }
    }
}

// ============================================================================
// Internal helpers — borrow nth + persistent snapshot for fallback paths
// ============================================================================

fn transient_nth_borrowed(body: &TransientVector, i: i64) -> Value {
    let tail_off = body.count - body.tail.len() as i64;
    if i >= tail_off {
        let v = body.tail[(i - tail_off) as usize];
        crate::rc::dup(v);
        return v;
    }
    let v = crate::types::vector::leaf_block_at_pub(&body.root, body.shift, i)
        [(i & 0x1f) as usize];
    crate::rc::dup(v);
    v
}

impl TransientVector {
    /// Materialize the transient's current state as a fresh
    /// PersistentVector for the path-copy fallback paths in
    /// `conj_bang` / `assoc_bang` / `pop_bang`. The persistent shares
    /// the trie root Arc (cheap) and copies the tail (one dup per
    /// element). Caller owns one ref; must drop_value.
    fn snapshot_as_persistent(this: Value) -> Value {
        let body = unsafe { TransientVector::body(this) };
        let root = body.root.clone();
        let mut tail_box: Vec<Value> = Vec::with_capacity(body.tail.len());
        for &t in body.tail.iter() {
            crate::rc::dup(t);
            tail_box.push(t);
        }
        crate::types::vector::PersistentVector::from_owned_parts(
            body.count,
            body.shift,
            root,
            tail_box.into_boxed_slice(),
        )
    }

    /// Replace the transient's count/shift/root/tail from a
    /// just-computed persistent. Drops the old transient state's
    /// element refs. Used by the path-copy fallback paths to lift
    /// the persistent result back into the transient's body.
    fn overwrite_from_persistent(this: Value, p: Value) {
        let (p_count, p_shift, p_root, p_tail) =
            crate::types::vector::PersistentVector::parts(p);
        let body = unsafe { TransientVector::body_mut(this) };
        // Drop old tail element refs.
        for &v in body.tail.iter() {
            crate::rc::drop_value(v);
        }
        body.tail.clear();
        body.tail.reserve(p_tail.len());
        for &t in p_tail.iter() {
            crate::rc::dup(t);
            body.tail.push(t);
        }
        body.root = p_root.clone();
        body.shift = p_shift;
        body.count = p_count;
    }
}

// hush unused-import warning when the `AtomicI32` is in
// register_type! generated alloc but not visibly used in this file.
#[allow(dead_code)]
fn _atomic_i32_holder() -> AtomicI32 {
    AtomicI32::new(0)
}
