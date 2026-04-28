//! `TransientVector` — single-thread mutable view of a
//! `PersistentVector`. Mirrors `clojure.lang.PersistentVector$
//! TransientVector` (JVM) and cljs's `TransientVector`.
//!
//! Storage layout matches the persistent shape: count, shift,
//! root (Arc<PVNode>), tail (Vec<Value>), plus an `alive` flag.
//!
//! All mutations operate in place where ownership permits:
//! - **Tail**: `Vec<Value>` is mutable, push/pop/replace happen
//!   without any path-copy.
//! - **Trie**: nodes are accessed via `Arc::make_mut`. The first
//!   touch on a shared node clones it (path-copy at that level
//!   only); subsequent mutations along the same path hit the
//!   uniquely-owned node directly with zero allocations and zero
//!   atomic refcount ops on the trie structure.
//!
//! For batch-conj! workflows where the underlying trie nodes are
//! mostly fresh allocations, this collapses to in-place pushes
//! through the trie. For assoc!-heavy workflows on a transient that
//! still shares its trie with the persistent source, the first hit
//! at each level pays a clone (same as a persistent path-copy);
//! subsequent hits are free.
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
use crate::types::vector::{
    self, PersistentVector,
    BRANCHING_PUB, MASK_PUB, SHIFT_STEP_PUB,
};
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
        let mut tail: Vec<Value> = Vec::with_capacity(BRANCHING_PUB);
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

            if body.tail.len() < BRANCHING_PUB {
                crate::rc::dup(x);
                body.tail.push(x);
                body.count += 1;
                crate::rc::dup(this);
                return this;
            }

            // Tail full → promote it to a leaf node and push into
            // the trie. The Vec drains into a [Value; 32]; refs move
            // through (no per-element dup or drop).
            let mut leaf_block = [Value::NIL; BRANCHING_PUB];
            let drained = std::mem::replace(&mut body.tail, Vec::with_capacity(1));
            for (i, v) in drained.into_iter().enumerate() {
                leaf_block[i] = v;
            }
            let leaf_arc: Arc<crate::types::vector::PVNode> = Arc::new(
                crate::types::vector::PVNode::Leaf { children: leaf_block }
            );

            let trie_count = body.count - BRANCHING_PUB as i64;
            if (trie_count >> SHIFT_STEP_PUB) > (1i64 << body.shift) - 1 {
                // Root is full at this depth — grow up by one level.
                // The new root has the old root in slot 0 and a fresh
                // path for the just-promoted leaf in slot 1.
                let path = vector::new_path_pub(body.shift, leaf_arc);
                let mut children = empty_children();
                let placeholder = vector::empty_internal_pub();
                let old_root = std::mem::replace(&mut body.root, placeholder);
                children[0] = Some(old_root);
                children[1] = Some(path);
                body.root = Arc::new(
                    crate::types::vector::PVNode::Internal { children }
                );
                body.shift += SHIFT_STEP_PUB;
            } else {
                vector::push_tail_in_place_pub(
                    &mut body.root, body.shift, body.count, leaf_arc,
                );
            }

            // New tail = [x].
            crate::rc::dup(x);
            body.tail.push(x);
            body.count += 1;
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
            } else {
                // Trie position: in-place mutation via Arc::make_mut
                // (clones nodes lazily on first touch, in-place after).
                vector::assoc_in_place_pub(&mut body.root, body.shift, i, v);
            }
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
                let last = body.tail.pop().unwrap();
                crate::rc::drop_value(last);
                body.count = 0;
                crate::rc::dup(this);
                return this;
            }
            // Tail has 1 elem; trie has more. In-place: drop the
            // tail elem, pop the rightmost trie leaf into the new
            // tail, possibly shrink shift.
            let last = body.tail.pop().unwrap();
            crate::rc::drop_value(last);

            let leaf_arc = vector::pop_tail_in_place_pub(
                &mut body.root, body.shift, body.count,
            );

            // Materialize the new tail by dup-ing each child of the
            // returned leaf. We deliberately don't try to mutate
            // through the Arc — `pop_tail_in_place_pub` may return a
            // leaf that's still shared with the persistent source
            // (when `make_mut` cloned a parent without affecting the
            // leaf), so unique-ownership isn't guaranteed. Dup-and-
            // drop is correct regardless of refcount: if the leaf
            // hits zero on the `drop`, `PVNode::Drop` decrefs each
            // child exactly once, balancing our N dups; if the leaf
            // stays alive (still shared), our dups are the new tail's
            // owned refs and the leaf's children are unaffected.
            let mut new_tail: Vec<Value> = Vec::with_capacity(BRANCHING_PUB);
            match leaf_arc.as_ref() {
                crate::types::vector::PVNode::Leaf { children } => {
                    for &v in children.iter() {
                        crate::rc::dup(v);
                        new_tail.push(v);
                    }
                }
                crate::types::vector::PVNode::Internal { .. } => {
                    unreachable!("pop_tail returned an Internal node");
                }
            }
            drop(leaf_arc);
            body.tail = new_tail;

            // Shrink shift if the root is now an Internal with a
            // single populated child.
            if body.shift > SHIFT_STEP_PUB {
                let collapsible = match body.root.as_ref() {
                    crate::types::vector::PVNode::Internal { children } => {
                        let count = children.iter().filter(|c| c.is_some()).count();
                        count == 1
                    }
                    _ => false,
                };
                if collapsible {
                    let root_node = Arc::make_mut(&mut body.root);
                    if let crate::types::vector::PVNode::Internal { children } = root_node {
                        let idx = (0..BRANCHING_PUB)
                            .find(|&i| children[i].is_some())
                            .unwrap();
                        let only = children[idx].take().unwrap();
                        body.root = only;
                        body.shift -= SHIFT_STEP_PUB;
                    }
                }
            }

            body.count -= 1;
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
// Internal helpers
// ============================================================================

fn transient_nth_borrowed(body: &TransientVector, i: i64) -> Value {
    let tail_off = body.count - body.tail.len() as i64;
    if i >= tail_off {
        let v = body.tail[(i - tail_off) as usize];
        crate::rc::dup(v);
        return v;
    }
    let v = vector::leaf_block_at_pub(&body.root, body.shift, i)
        [(i & MASK_PUB) as usize];
    crate::rc::dup(v);
    v
}

/// `[None; 32]` for `Option<Arc<PVNode>>` — Option<Arc<T>> isn't
/// `Copy`, so we use `from_fn` instead of array-repeat syntax.
fn empty_children() -> [Option<Arc<crate::types::vector::PVNode>>; BRANCHING_PUB] {
    std::array::from_fn(|_| None)
}

// silence unused-import for AtomicI32 (used by register_type! generated alloc)
#[allow(dead_code)]
fn _atomic_i32_holder() -> AtomicI32 { AtomicI32::new(0) }
