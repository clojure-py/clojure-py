//! Persistent vector — 32-way trie + tail array, eager. Mirrors the
//! shape of `clojure.lang.PersistentVector` (JVM) and cljs's
//! `cljs.core.PersistentVector`. Tail-optimization is in (cons is O(1)
//! amortized); chunked seqs, transients, and IReduce specializations
//! are deferred to follow-up slices.
//!
//! Layout:
//!   - `count`  — total elements in this vector.
//!   - `shift`  — depth of the trie × 5 (5 bits per level → 32-way).
//!     A vector with `count <= 32` keeps everything in the tail and
//!     the root is the empty leaf node; `shift = 5` regardless. As
//!     elements overflow the tail, full 32-element blocks get pushed
//!     into the trie and the root grows in height.
//!   - `root`   — `Arc<PVNode>`. Internal-only; not a registered
//!     runtime type, never observable from Clojure code. `Arc` gives
//!     us cheap path-copy: unchanged subtrees are shared by clone, a
//!     modified row builds a new node.
//!   - `tail`   — owned `Box<[Value]>` of length 0..=32 holding the
//!     rightmost block. `cons` appends here until full, then promotes
//!     the block into the trie.
//!   - `meta`   — `IMeta` slot.
//!   - `hash`   — cached `IHash`; 0 means uncomputed.
//!
//! `PVNode` is a two-variant enum: `Internal` holds 32 optional
//! sub-nodes, `Leaf` holds 32 user `Value`s. The trie invariant is
//! "all children of a given parent are the same kind"; the parent's
//! depth (== `level/SHIFT_STEP`) determines which variant is expected.

use core::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, OnceLock};

use crate::hash::murmur3;
use crate::protocols::associative::IAssociative;
use crate::protocols::collection::ICollection;
use crate::protocols::counted::ICounted;
use crate::protocols::editable_collection::IEditableCollection;
use crate::protocols::emptyable_collection::IEmptyableCollection;
use crate::protocols::equiv::IEquiv;
use crate::protocols::hash::IHash;
use crate::protocols::indexed::IIndexed;
use crate::protocols::lookup::ILookup;
use crate::protocols::meta::{IMeta, IWithMeta};
use crate::protocols::persistent_vector::IPersistentVector;
use crate::protocols::reduce::IReduce;
use crate::protocols::reversible::IReversible;
use crate::protocols::seq::ISeqable;
use crate::protocols::sequential::ISequential;
use crate::protocols::stack::IStack;
use crate::value::Value;

const BRANCHING:  usize = 32;
const SHIFT_STEP: i32   = 5;
const MASK:       i64   = 0x1f;

// ============================================================================
// Internal trie node
// ============================================================================

pub(crate) enum PVNode {
    Internal { children: [Option<Arc<PVNode>>; BRANCHING] },
    Leaf     { children: [Value; BRANCHING] },
}

impl Drop for PVNode {
    fn drop(&mut self) {
        if let PVNode::Leaf { children } = self {
            for v in children.iter() {
                crate::rc::drop_value(*v);
            }
        }
        // Internal: each Option<Arc<PVNode>>'s own Drop chains through
        // Arc::drop and (eventually) recurses into this Drop again on
        // the contained PVNode, so we don't walk Internal here.
    }
}

/// Manual `Clone` for path-copy. Used by `Arc::make_mut` when a
/// transient mutation hits a node it doesn't uniquely own.
///
/// - `Internal`: clone the children array. Each `Option<Arc<PVNode>>::
///   clone` is an atomic Arc-refcount bump on its inner.
/// - `Leaf`: dup each `Value` into a fresh `[Value; 32]`. The clone
///   owns one ref per element; the original is unchanged.
impl Clone for PVNode {
    fn clone(&self) -> Self {
        match self {
            PVNode::Internal { children } => {
                let mut new_children = empty_internal_children();
                for (i, c) in children.iter().enumerate() {
                    new_children[i] = c.clone();
                }
                PVNode::Internal { children: new_children }
            }
            PVNode::Leaf { children } => {
                let mut new_children = [Value::NIL; BRANCHING];
                for (i, &v) in children.iter().enumerate() {
                    crate::rc::dup(v);
                    new_children[i] = v;
                }
                PVNode::Leaf { children: new_children }
            }
        }
    }
}

impl PVNode {
    fn empty_internal() -> Arc<PVNode> {
        EMPTY_INTERNAL.get_or_init(|| {
            Arc::new(PVNode::Internal { children: empty_internal_children() })
        }).clone()
    }
}

static EMPTY_INTERNAL: OnceLock<Arc<PVNode>> = OnceLock::new();

fn empty_internal_children() -> [Option<Arc<PVNode>>; BRANCHING] {
    // `[None; 32]` requires `Copy`, which `Option<Arc<T>>` doesn't have.
    // Use `from_fn` instead.
    std::array::from_fn(|_| None)
}

// ============================================================================
// Vector body
// ============================================================================

clojure_rt_macros::register_type! {
    pub struct PersistentVector {
        count: i64,
        shift: i32,
        root:  Arc<PVNode>,   // its Drop runs when the body is destructed.
        tail:  Box<[Value]>,  // 0..=32 elements; refs are owned.
        meta:  Value,
        hash:  AtomicI32,     // 0 = uncomputed.
    }
}

static EMPTY_VECTOR_SINGLETON: OnceLock<Value> = OnceLock::new();

/// The canonical empty vector with no metadata. `withMeta` returns a
/// fresh allocation rather than mutating this singleton.
///
/// The root is an empty Internal node so that the first tail-promotion
/// (`push_tail` at `shift = SHIFT_STEP`) finds the right shape — an
/// Internal node whose children will be leaf-blocks at level 0.
///
/// Like the empty-list singleton, this Value is published from the
/// first caller's thread to all other threads via the `OnceLock`. We
/// flip the rc to shared-mode inside `get_or_init` before publication
/// so subsequent dup/drop calls from other threads use the atomic
/// path. (See the matching note in `types/list.rs::empty_list`.)
pub fn empty_vector() -> Value {
    let v = *EMPTY_VECTOR_SINGLETON.get_or_init(|| {
        let v = PersistentVector::alloc(
            0,
            SHIFT_STEP,
            PVNode::empty_internal(),
            Vec::<Value>::new().into_boxed_slice(),
            Value::NIL,
            AtomicI32::new(0),
        );
        crate::rc::share(v);
        v
    });
    crate::rc::dup(v);
    v
}

fn vector_type_id() -> crate::value::TypeId {
    *PERSISTENTVECTOR_TYPE_ID.get().expect("PersistentVector: clojure_rt::init() not called")
}

// ============================================================================
// PersistentVector — public constructors / mutators
// ============================================================================

impl PersistentVector {
    /// Build a vector from a slice. **Borrow semantics**: caller's
    /// refs are unchanged; the new vector dups each element for its
    /// own storage.
    pub fn from_slice(items: &[Value]) -> Value {
        let mut v = empty_vector();
        for &x in items {
            let nv = PersistentVector::cons(v, x);
            crate::rc::drop_value(v);
            v = nv;
        }
        v
    }

    /// Cons `x` onto the tail. **Borrow semantics**: caller's ref to
    /// `x` is unchanged; the new vector dups `x` for its own storage.
    pub fn cons(this: Value, x: Value) -> Value {
        let body = unsafe { PersistentVector::body(this) };
        let tail_len = body.tail.len();

        if tail_len < BRANCHING {
            // Tail has room: copy + append.
            let mut new_tail: Vec<Value> = Vec::with_capacity(tail_len + 1);
            for &t in body.tail.iter() {
                crate::rc::dup(t);
                new_tail.push(t);
            }
            crate::rc::dup(x);
            new_tail.push(x);
            crate::rc::dup(body.meta);
            return PersistentVector::alloc(
                body.count + 1,
                body.shift,
                body.root.clone(),
                new_tail.into_boxed_slice(),
                body.meta,
                AtomicI32::new(0),
            );
        }

        // Tail is full. Promote the existing tail into the trie and
        // start a fresh tail with `x`.
        let mut tail_block = [Value::NIL; BRANCHING];
        for (i, &t) in body.tail.iter().enumerate() {
            crate::rc::dup(t);
            tail_block[i] = t;
        }
        let tail_node: Arc<PVNode> = Arc::new(PVNode::Leaf { children: tail_block });

        let trie_count = body.count - BRANCHING as i64;
        let (new_root, new_shift) =
            if (trie_count >> SHIFT_STEP) > (1i64 << body.shift) - 1 {
                // Root is full at this depth → grow up by one level.
                // The new root has the old root in slot 0 and a fresh
                // path for `tail_node` in slot 1.
                let path = new_path(body.shift, tail_node);
                let mut children = empty_internal_children();
                children[0] = Some(body.root.clone());
                children[1] = Some(path);
                (Arc::new(PVNode::Internal { children }), body.shift + SHIFT_STEP)
            } else {
                (push_tail(body.shift, &body.root, body.count, tail_node), body.shift)
            };

        crate::rc::dup(body.meta);
        let mut new_tail = Vec::with_capacity(1);
        crate::rc::dup(x);
        new_tail.push(x);
        PersistentVector::alloc(
            body.count + 1,
            new_shift,
            new_root,
            new_tail.into_boxed_slice(),
            body.meta,
            AtomicI32::new(0),
        )
    }

    /// Out-of-bounds returns `None`. Caller decides between
    /// throw-on-OOB and not-found semantics. The returned Value owns
    /// one ref — caller is responsible for `drop_value` when done.
    pub fn nth(this: Value, n: i64) -> Option<Value> {
        let v = Self::nth_borrowed(this, n)?;
        crate::rc::dup(v);
        Some(v)
    }

    /// Like `nth` but the returned Value's ref is **borrowed** from
    /// the underlying leaf block (or tail). Caller must NOT
    /// `drop_value` it. Used by per-element read loops (hash, equiv,
    /// reduce) that don't need a separate owned ref to pass through
    /// dispatch — the surrounding leaf node's Arc keeps the element
    /// alive for the duration of the call.
    ///
    /// The trie descent is done entirely with borrows
    /// (`Arc::as_ref()` + `Option::as_ref()`); zero atomic operations
    /// on the walk.
    pub(crate) fn nth_borrowed(this: Value, n: i64) -> Option<Value> {
        let body = unsafe { PersistentVector::body(this) };
        if n < 0 || n >= body.count {
            return None;
        }
        let tail_off = tail_offset(body.count, body.tail.len());
        if n >= tail_off {
            return Some(body.tail[(n - tail_off) as usize]);
        }
        let leaf = leaf_block_at(&body.root, body.shift, n);
        Some(leaf[(n & MASK) as usize])
    }

    /// Path-copy assoc at an existing index. `n == count` extends via
    /// `cons`; `n` outside `[0, count]` returns an exception Value.
    /// **Borrow semantics**: caller's ref to `x` is unchanged.
    pub fn assoc(this: Value, n: i64, x: Value) -> Value {
        let body = unsafe { PersistentVector::body(this) };
        if n < 0 || n > body.count {
            return crate::exception::make_foreign(format!(
                "Index {} out of bounds for vector of size {}", n, body.count
            ));
        }
        if n == body.count {
            // cons borrows; we forward x as borrowed.
            return PersistentVector::cons(this, x);
        }

        let tail_off = tail_offset(body.count, body.tail.len());
        if n >= tail_off {
            // Replace within the tail. Dup x for the new slot; dup
            // the surviving siblings.
            let mut new_tail: Vec<Value> = Vec::with_capacity(body.tail.len());
            for (i, &t) in body.tail.iter().enumerate() {
                if i as i64 == n - tail_off {
                    crate::rc::dup(x);
                    new_tail.push(x);
                } else {
                    crate::rc::dup(t);
                    new_tail.push(t);
                }
            }
            crate::rc::dup(body.meta);
            return PersistentVector::alloc(
                body.count,
                body.shift,
                body.root.clone(),
                new_tail.into_boxed_slice(),
                body.meta,
                AtomicI32::new(0),
            );
        }

        // Path-copy down the trie to a leaf, replacing one slot.
        // `do_assoc` is a private internal helper; it transfers `x`
        // into the new leaf, so we dup once here for the placement.
        crate::rc::dup(x);
        let new_root = do_assoc(body.shift, &body.root, n, x);
        crate::rc::dup(body.meta);
        let mut tail_owned: Vec<Value> = Vec::with_capacity(body.tail.len());
        for &t in body.tail.iter() {
            crate::rc::dup(t);
            tail_owned.push(t);
        }
        PersistentVector::alloc(
            body.count,
            body.shift,
            new_root,
            tail_owned.into_boxed_slice(),
            body.meta,
            AtomicI32::new(0),
        )
    }

    /// Returns the empty vector when called on a 1-element vector.
    /// Calling on the empty vector returns an exception Value.
    pub fn pop(this: Value) -> Value {
        let body = unsafe { PersistentVector::body(this) };
        if body.count == 0 {
            return crate::exception::make_foreign("Can't pop empty vector".to_string());
        }
        if body.count == 1 {
            return empty_vector();
        }
        if body.tail.len() > 1 {
            let new_len = body.tail.len() - 1;
            let mut new_tail: Vec<Value> = Vec::with_capacity(new_len);
            for &t in body.tail.iter().take(new_len) {
                crate::rc::dup(t);
                new_tail.push(t);
            }
            crate::rc::dup(body.meta);
            return PersistentVector::alloc(
                body.count - 1,
                body.shift,
                body.root.clone(),
                new_tail.into_boxed_slice(),
                body.meta,
                AtomicI32::new(0),
            );
        }
        // Tail has 1 element. Promote the rightmost trie leaf-block to
        // the new tail; drop that leaf from the trie.
        let new_tail_node = rightmost_leaf(&body.root, body.shift);
        let mut new_tail_vec: Vec<Value> = Vec::with_capacity(BRANCHING);
        if let PVNode::Leaf { children } = new_tail_node.as_ref() {
            for &v in children.iter() {
                crate::rc::dup(v);
                new_tail_vec.push(v);
            }
        } else {
            unreachable!("rightmost_leaf returned an Internal node");
        }

        let mut new_root = pop_tail(body.shift, &body.root, body.count);
        let mut new_shift = body.shift;
        // Shrink: collapse a height level when the root has only one
        // populated child and we're above leaf level.
        if new_shift > SHIFT_STEP {
            if let PVNode::Internal { children } = new_root.as_ref() {
                let populated: Vec<&Arc<PVNode>> = children
                    .iter()
                    .filter_map(|c| c.as_ref())
                    .collect();
                if populated.len() == 1 {
                    let only = populated[0].clone();
                    new_root = only;
                    new_shift -= SHIFT_STEP;
                }
            }
        }

        crate::rc::dup(body.meta);
        PersistentVector::alloc(
            body.count - 1,
            new_shift,
            new_root,
            new_tail_vec.into_boxed_slice(),
            body.meta,
            AtomicI32::new(0),
        )
    }

    pub fn count_of(this: Value) -> i64 {
        unsafe { PersistentVector::body(this) }.count
    }

    /// Construct a `PersistentVector` directly from already-owned
    /// parts. Caller transfers one ref of every element in `tail`
    /// and the Arc to `root`. Used by `TransientVector::persistent_
    /// bang` to freeze without re-dup'ing the tail elements.
    pub(crate) fn from_owned_parts(
        count: i64,
        shift: i32,
        root: Arc<PVNode>,
        tail: Box<[Value]>,
    ) -> Value {
        PersistentVector::alloc(
            count, shift, root, tail, Value::NIL, AtomicI32::new(0),
        )
    }

    /// Crate-private accessor for the four data fields a transient
    /// snapshot needs. Borrows are tied to the body's lifetime; the
    /// caller must not let them outlive the underlying Value.
    pub(crate) fn parts<'a>(this: Value)
        -> (i64, i32, &'a Arc<PVNode>, &'a [Value])
    {
        let body = unsafe { PersistentVector::body(this) };
        (body.count, body.shift, &body.root, &body.tail)
    }

    /// Length of the tail block (0..=32). Exposed for chunked seqs
    /// that need to compute block boundaries without reaching into
    /// private fields.
    pub fn tail_len_of(this: Value) -> i64 {
        unsafe { PersistentVector::body(this) }.tail.len() as i64
    }

    /// Extract the leaf-block (or tail slice) containing element index
    /// `i`, dup-ing each element into a fresh `Vec<Value>`. Returns
    /// the block's logical start and end indices in the vector along
    /// with the materialized values. `i` must be in `[0, count)`.
    /// Used by `ChunkedVecSeq` / `IChunkedSeq` to vend `ArrayChunk`s.
    pub fn block_for(this: Value, i: i64) -> (Vec<Value>, i64, i64) {
        let body = unsafe { PersistentVector::body(this) };
        debug_assert!(i >= 0 && i < body.count, "block_for: index out of range");
        let tail_off = tail_offset(body.count, body.tail.len());
        if i >= tail_off {
            // Tail block: from tail_off to count.
            let mut out: Vec<Value> = Vec::with_capacity(body.tail.len());
            for &v in body.tail.iter() {
                crate::rc::dup(v);
                out.push(v);
            }
            return (out, tail_off, body.count);
        }
        // Trie block: align `i` down to a 32-multiple, walk to leaf.
        let block_start = i & !MASK;
        let block_end = (block_start + BRANCHING as i64).min(tail_off);
        let mut node: Arc<PVNode> = body.root.clone();
        let mut s = body.shift;
        while s > 0 {
            let idx = ((block_start >> s) & MASK) as usize;
            let next = match node.as_ref() {
                PVNode::Internal { children } => children[idx]
                    .as_ref()
                    .expect("trie invariant: child present")
                    .clone(),
                PVNode::Leaf { .. } => panic!("trie invariant: leaf above level 0"),
            };
            node = next;
            s -= SHIFT_STEP;
        }
        let leaf = match node.as_ref() {
            PVNode::Leaf { children } => children,
            PVNode::Internal { .. } => panic!("trie invariant: internal at level 0"),
        };
        let span = (block_end - block_start) as usize;
        let mut out: Vec<Value> = Vec::with_capacity(span);
        for &v in leaf.iter().take(span) {
            crate::rc::dup(v);
            out.push(v);
        }
        (out, block_start, block_end)
    }
}

// ============================================================================
// Internal trie helpers
// ============================================================================

fn tail_offset(count: i64, tail_len: usize) -> i64 {
    count - tail_len as i64
}

/// Build a chain of internal nodes from height `level` down to 0 with
/// a single child at index 0 each level, terminating at `tail_node`.
/// Used when growing the root upward.
fn new_path(level: i32, tail_node: Arc<PVNode>) -> Arc<PVNode> {
    if level == 0 {
        return tail_node;
    }
    let inner = new_path(level - SHIFT_STEP, tail_node);
    let mut children = empty_internal_children();
    children[0] = Some(inner);
    Arc::new(PVNode::Internal { children })
}

/// Insert `tail_node` (a leaf-shaped PVNode) into the trie at the next
/// available slot, returning a new root. `count` is the total element
/// count *before* this insertion (i.e., trie size + the now-promoted
/// tail).
fn push_tail(shift: i32, parent: &Arc<PVNode>, count: i64, tail_node: Arc<PVNode>)
    -> Arc<PVNode>
{
    let parent_children = match parent.as_ref() {
        PVNode::Internal { children } => children,
        PVNode::Leaf { .. } => unreachable!("push_tail above leaf level"),
    };
    let sub_idx = (((count - 1) >> shift) & MASK) as usize;
    let mut new_children = empty_internal_children();
    for (i, c) in parent_children.iter().enumerate() {
        new_children[i] = c.clone();
    }
    let to_insert = if shift == SHIFT_STEP {
        tail_node
    } else {
        match parent_children[sub_idx].as_ref() {
            None => new_path(shift - SHIFT_STEP, tail_node),
            Some(child) => push_tail(shift - SHIFT_STEP, child, count, tail_node),
        }
    };
    new_children[sub_idx] = Some(to_insert);
    Arc::new(PVNode::Internal { children: new_children })
}

/// Path-copy assoc inside the trie. `level == 0` replaces a leaf slot;
/// higher levels recurse. Caller transfers one ref of `x` to the
/// returned root.
fn do_assoc(level: i32, node: &Arc<PVNode>, i: i64, x: Value) -> Arc<PVNode> {
    if level == 0 {
        let leaf_children = match node.as_ref() {
            PVNode::Leaf { children } => children,
            PVNode::Internal { .. } => unreachable!("do_assoc level=0 on internal"),
        };
        let mut new_children = [Value::NIL; BRANCHING];
        let idx = (i & MASK) as usize;
        for (j, &c) in leaf_children.iter().enumerate() {
            if j == idx {
                new_children[j] = x;
            } else {
                crate::rc::dup(c);
                new_children[j] = c;
            }
        }
        return Arc::new(PVNode::Leaf { children: new_children });
    }
    let int_children = match node.as_ref() {
        PVNode::Internal { children } => children,
        PVNode::Leaf { .. } => unreachable!("do_assoc level>0 on leaf"),
    };
    let idx = ((i >> level) & MASK) as usize;
    let mut new_children = empty_internal_children();
    for (j, c) in int_children.iter().enumerate() {
        if j == idx {
            let child = c.as_ref().expect("trie invariant: assoc path exists");
            new_children[j] = Some(do_assoc(level - SHIFT_STEP, child, i, x));
        } else {
            new_children[j] = c.clone();
        }
    }
    Arc::new(PVNode::Internal { children: new_children })
}

/// Walk to the rightmost leaf node in the trie.
fn rightmost_leaf(node: &Arc<PVNode>, shift: i32) -> Arc<PVNode> {
    if shift == 0 {
        return node.clone();
    }
    let int_children = match node.as_ref() {
        PVNode::Internal { children } => children,
        PVNode::Leaf { .. } => unreachable!("rightmost_leaf at level>0 hit a Leaf"),
    };
    let mut last_idx = 0usize;
    for (i, c) in int_children.iter().enumerate() {
        if c.is_some() {
            last_idx = i;
        }
    }
    let child = int_children[last_idx].as_ref().expect("populated");
    rightmost_leaf(child, shift - SHIFT_STEP)
}

/// Remove the rightmost leaf from the trie. `count` is the total
/// element count *before* the pop. Returns a new root; the caller's
/// PersistentVector::pop handles further height shrink.
fn pop_tail(shift: i32, node: &Arc<PVNode>, count: i64) -> Arc<PVNode> {
    let int_children = match node.as_ref() {
        PVNode::Internal { children } => children,
        PVNode::Leaf { .. } => unreachable!("pop_tail at level>0 on Leaf"),
    };
    let sub_idx = (((count - 2) >> shift) & MASK) as usize;
    if shift > SHIFT_STEP {
        let child = int_children[sub_idx].as_ref().expect("populated path");
        let new_child = pop_tail(shift - SHIFT_STEP, child, count);
        let new_child_empty = match new_child.as_ref() {
            PVNode::Internal { children } => children.iter().all(|c| c.is_none()),
            PVNode::Leaf { children } => children.iter().all(|v| v.is_nil()),
        };
        if new_child_empty && sub_idx == 0 {
            return PVNode::empty_internal();
        }
        let mut new_children = empty_internal_children();
        for (i, c) in int_children.iter().enumerate() {
            if i == sub_idx {
                if !new_child_empty {
                    new_children[i] = Some(new_child.clone());
                }
            } else {
                new_children[i] = c.clone();
            }
        }
        return Arc::new(PVNode::Internal { children: new_children });
    }
    // shift == SHIFT_STEP: leaf-blocks are direct children. Drop sub_idx.
    if sub_idx == 0 {
        return PVNode::empty_internal();
    }
    let mut new_children = empty_internal_children();
    for (i, c) in int_children.iter().enumerate() {
        if i == sub_idx { continue; }
        new_children[i] = c.clone();
    }
    Arc::new(PVNode::Internal { children: new_children })
}

// ============================================================================
// Protocol impls
// ============================================================================

clojure_rt_macros::implements! {
    impl ICounted for PersistentVector {
        fn count(this: Value) -> Value {
            Value::int(unsafe { PersistentVector::body(this) }.count)
        }
    }
}

clojure_rt_macros::implements! {
    impl IIndexed for PersistentVector {
        fn nth_2(this: Value, n: Value) -> Value {
            let Some(i) = n.as_int() else {
                return crate::exception::make_foreign(
                    format!("nth: index must be an integer, got tag {}", n.tag),
                );
            };
            match PersistentVector::nth(this, i) {
                Some(v) => v,
                None => crate::exception::make_foreign(
                    format!("Index {} out of bounds for vector of size {}",
                            i, unsafe { PersistentVector::body(this) }.count),
                ),
            }
        }
        fn nth_3(this: Value, n: Value, not_found: Value) -> Value {
            let Some(i) = n.as_int() else {
                crate::rc::dup(not_found);
                return not_found;
            };
            match PersistentVector::nth(this, i) {
                Some(v) => v,
                None => {
                    crate::rc::dup(not_found);
                    not_found
                }
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl ICollection for PersistentVector {
        fn conj(this: Value, x: Value) -> Value {
            // PersistentVector::cons borrows x; no pre-dup needed.
            PersistentVector::cons(this, x)
        }
    }
}

clojure_rt_macros::implements! {
    impl IEmptyableCollection for PersistentVector {
        fn empty(this: Value) -> Value {
            let _ = this;
            empty_vector()
        }
    }
}

clojure_rt_macros::implements! {
    impl IStack for PersistentVector {
        fn peek(this: Value) -> Value {
            let body = unsafe { PersistentVector::body(this) };
            if body.count == 0 {
                Value::NIL
            } else {
                let last = body.tail[body.tail.len() - 1];
                crate::rc::dup(last);
                last
            }
        }
        fn pop(this: Value) -> Value {
            PersistentVector::pop(this)
        }
    }
}

clojure_rt_macros::implements! {
    impl IAssociative for PersistentVector {
        fn assoc(this: Value, k: Value, v: Value) -> Value {
            let Some(i) = k.as_int() else {
                return crate::exception::make_foreign(
                    "Vector key must be an integer".to_string(),
                );
            };
            // PersistentVector::assoc borrows v; no pre-dup needed.
            PersistentVector::assoc(this, i, v)
        }
        fn contains_key(this: Value, k: Value) -> Value {
            let Some(i) = k.as_int() else { return Value::FALSE };
            let body = unsafe { PersistentVector::body(this) };
            if 0 <= i && i < body.count { Value::TRUE } else { Value::FALSE }
        }
        fn find(this: Value, k: Value) -> Value {
            let Some(i) = k.as_int() else { return Value::NIL };
            match PersistentVector::nth(this, i) {
                Some(v) => {
                    // MapEntry::new borrows; we're holding `v` from
                    // nth so drop our owned copy after construction.
                    let me = crate::types::map_entry::MapEntry::new(k, v);
                    crate::rc::drop_value(v);
                    me
                }
                None => Value::NIL,
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl ILookup for PersistentVector {
        fn lookup_2(this: Value, k: Value) -> Value {
            let Some(i) = k.as_int() else { return Value::NIL };
            PersistentVector::nth(this, i).unwrap_or(Value::NIL)
        }
        fn lookup_3(this: Value, k: Value, not_found: Value) -> Value {
            let Some(i) = k.as_int() else {
                crate::rc::dup(not_found);
                return not_found;
            };
            match PersistentVector::nth(this, i) {
                Some(v) => v,
                None => {
                    crate::rc::dup(not_found);
                    not_found
                }
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl IReversible for PersistentVector {
        fn rseq(this: Value) -> Value {
            let body = unsafe { PersistentVector::body(this) };
            if body.count == 0 {
                return Value::NIL;
            }
            crate::types::vec_seq::VecRSeq::start(this)
        }
    }
}

clojure_rt_macros::implements! {
    impl ISeqable for PersistentVector {
        fn seq(this: Value) -> Value {
            let body = unsafe { PersistentVector::body(this) };
            if body.count == 0 {
                Value::NIL
            } else {
                crate::types::vec_seq::VecSeq::start(this)
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl IMeta for PersistentVector {
        fn meta(this: Value) -> Value {
            let m = unsafe { PersistentVector::body(this) }.meta;
            crate::rc::dup(m);
            m
        }
    }
}

clojure_rt_macros::implements! {
    impl IWithMeta for PersistentVector {
        fn with_meta(this: Value, meta: Value) -> Value {
            let body = unsafe { PersistentVector::body(this) };
            crate::rc::dup(meta);
            let mut tail_owned: Vec<Value> = Vec::with_capacity(body.tail.len());
            for &t in body.tail.iter() {
                crate::rc::dup(t);
                tail_owned.push(t);
            }
            PersistentVector::alloc(
                body.count,
                body.shift,
                body.root.clone(),
                tail_owned.into_boxed_slice(),
                meta,
                AtomicI32::new(0),
            )
        }
    }
}

clojure_rt_macros::implements! {
    impl IHash for PersistentVector {
        fn hash(this: Value) -> Value {
            let body = unsafe { PersistentVector::body(this) };
            let cached = body.hash.load(Ordering::Relaxed);
            if cached != 0 {
                return Value::int(cached as i64);
            }
            let h = compute_vector_hash(this);
            body.hash.store(h, Ordering::Relaxed);
            Value::int(h as i64)
        }
    }
}

clojure_rt_macros::implements! {
    impl IEquiv for PersistentVector {
        fn equiv(this: Value, other: Value) -> Value {
            if other.tag != vector_type_id() {
                // Cross-type sequential equiv (vector vs list, etc.) is
                // deferred — see the matching note in types/list.rs.
                return Value::FALSE;
            }
            if vectors_equiv(this, other) { Value::TRUE } else { Value::FALSE }
        }
    }
}

clojure_rt_macros::implements! { impl ISequential       for PersistentVector {} }
clojure_rt_macros::implements! { impl IPersistentVector for PersistentVector {} }

clojure_rt_macros::implements! {
    impl IEditableCollection for PersistentVector {
        fn as_transient(this: Value) -> Value {
            crate::types::transient_vector::TransientVector::from_persistent(this)
        }
    }
}

// `IReduce` walks the trie one *leaf-block at a time* and iterates the
// resulting `&[Value; 32]` directly. Each element is **borrowed** for
// the duration of its `IFn::invoke` — the surrounding leaf node's Arc
// keeps the element refs alive, so we skip the per-element dup/drop
// pair. The accumulator is the only Value the loop owns: we drop the
// old `acc` after each step and own the new one returned by `invoke`.
//
// Compared to the per-`nth` shape, this saves an Arc walk + a dup +
// a drop per element — for a 1024-element vector that's roughly two
// orders of magnitude fewer atomic operations on the rc/Arc paths.
clojure_rt_macros::implements! {
    impl IReduce for PersistentVector {
        fn reduce_2(this: Value, f: Value) -> Value {
            let body = unsafe { PersistentVector::body(this) };
            if body.count == 0 {
                // (reduce f []) => (f)
                return clojure_rt_macros::dispatch!(
                    crate::protocols::ifn::IFn::invoke, &[f]
                );
            }
            // Seed = first element. We *own* the seed, so dup once.
            let seed = first_element(body);
            crate::rc::dup(seed);
            reduce_walk(body, f, 1, seed)
        }
        fn reduce_3(this: Value, f: Value, init: Value) -> Value {
            let body = unsafe { PersistentVector::body(this) };
            crate::rc::dup(init);
            reduce_walk(body, f, 0, init)
        }
    }
}

/// First element of a non-empty vector. Borrowed; caller decides
/// whether to dup. Centralizes the "is index 0 in the trie or in the
/// tail" branch so reduce_2's seed read uses the same helper as the
/// loop body would.
fn first_element(body: &PersistentVector) -> Value {
    let tail_off = tail_offset(body.count, body.tail.len());
    if tail_off > 0 {
        leaf_block_at(&body.root, body.shift, 0)[0]
    } else {
        body.tail[0]
    }
}

/// Walk the vector starting at `start_idx`, threading `acc` through
/// `f`. The caller must already own `acc` (one ref); the returned
/// Value also owns one ref. Borrowed per-element semantics: the leaf
/// node / tail keeps its element refs alive across the invoke, so we
/// don't dup `x` before the call or drop after.
fn reduce_walk(body: &PersistentVector, f: Value, start_idx: i64, mut acc: Value) -> Value {
    let count = body.count;
    let tail_off = tail_offset(body.count, body.tail.len());
    let reduced_tag = crate::types::reduced::Reduced::type_id();

    let mut i = start_idx;
    while i < tail_off {
        // Trie block: walk to the containing leaf, then iterate the
        // [Value; 32] slice directly.
        let leaf = leaf_block_at(&body.root, body.shift, i);
        let block_start = i & !MASK;
        let block_end = (block_start + BRANCHING as i64).min(tail_off);
        let mut j = (i - block_start) as usize;
        let last = (block_end - block_start) as usize;
        while j < last {
            let x = leaf[j]; // borrowed
            let new_acc = clojure_rt_macros::dispatch!(
                crate::protocols::ifn::IFn::invoke, &[f, acc, x]
            );
            crate::rc::drop_value(acc);
            acc = new_acc;
            if acc.tag == reduced_tag {
                return crate::rt::unreduced(acc);
            }
            if acc.is_exception() {
                return acc;
            }
            j += 1;
        }
        i = block_end;
    }

    // Tail block: same shape, source is `body.tail`.
    while i < count {
        let x = body.tail[(i - tail_off) as usize]; // borrowed
        let new_acc = clojure_rt_macros::dispatch!(
            crate::protocols::ifn::IFn::invoke, &[f, acc, x]
        );
        crate::rc::drop_value(acc);
        acc = new_acc;
        if acc.tag == reduced_tag {
            return crate::rt::unreduced(acc);
        }
        if acc.is_exception() {
            return acc;
        }
        i += 1;
    }

    acc
}

/// Public-from-crate alias for `leaf_block_at`. Used by
/// `TransientVector::nth_borrowed` so it doesn't have to duplicate
/// the trie-walk logic. Same borrow-only semantics.
pub(crate) fn leaf_block_at_pub<'a>(
    root: &'a Arc<PVNode>,
    shift: i32,
    i: i64,
) -> &'a [Value; BRANCHING] {
    leaf_block_at(root, shift, i)
}

// ============================================================================
// In-place trie mutators for `TransientVector`. Each uses
// `Arc::make_mut` to either acquire a unique mutable reference or
// clone the node when shared. Subsequent mutations on the same path
// hit the unique reference directly — zero allocations, zero
// refcount touches on the trie structure.
// ============================================================================

/// In-place trie assoc. The leaf node containing index `i` gets its
/// `children[i & 31]` slot replaced with a *dup'd* copy of `x`; the
/// old element is dropped. Caller's ref to `x` is unchanged.
pub(crate) fn assoc_in_place_pub(arc: &mut Arc<PVNode>, level: i32, i: i64, x: Value) {
    let node = Arc::make_mut(arc);
    if level == 0 {
        match node {
            PVNode::Leaf { children } => {
                let idx = (i & MASK) as usize;
                let old = children[idx];
                crate::rc::dup(x);
                children[idx] = x;
                crate::rc::drop_value(old);
            }
            PVNode::Internal { .. } => unreachable!("assoc_in_place at level 0 on Internal"),
        }
    } else {
        match node {
            PVNode::Internal { children } => {
                let idx = ((i >> level) & MASK) as usize;
                let child = children[idx]
                    .as_mut()
                    .expect("trie invariant: assoc path exists");
                assoc_in_place_pub(child, level - SHIFT_STEP, i, x);
            }
            PVNode::Leaf { .. } => unreachable!("assoc_in_place at level>0 on Leaf"),
        }
    }
}

/// In-place push-tail. Insert `leaf` (a fully-populated `PVNode::
/// Leaf`) into the trie at the next available slot for an `count`-
/// element vector (where `count` is the size *before* the leaf was
/// promoted from the tail). Caller transfers ownership of `leaf`.
pub(crate) fn push_tail_in_place_pub(
    arc: &mut Arc<PVNode>,
    level: i32,
    count: i64,
    leaf: Arc<PVNode>,
) {
    let node = Arc::make_mut(arc);
    let sub_idx = (((count - 1) >> level) & MASK) as usize;
    match node {
        PVNode::Internal { children } => {
            if level == SHIFT_STEP {
                debug_assert!(
                    children[sub_idx].is_none(),
                    "push_tail_in_place: leaf slot already occupied"
                );
                children[sub_idx] = Some(leaf);
            } else if children[sub_idx].is_none() {
                children[sub_idx] = Some(new_path(level - SHIFT_STEP, leaf));
            } else {
                let child = children[sub_idx].as_mut().unwrap();
                push_tail_in_place_pub(child, level - SHIFT_STEP, count, leaf);
            }
        }
        PVNode::Leaf { .. } => unreachable!("push_tail above leaf level on Leaf"),
    }
}

/// In-place pop-tail. Removes and returns the rightmost leaf-block.
/// `count` is the total element count *before* the pop. The trie may
/// have empty intermediate nodes after the pop; the caller handles
/// any height-shrink decision.
pub(crate) fn pop_tail_in_place_pub(
    arc: &mut Arc<PVNode>,
    level: i32,
    count: i64,
) -> Arc<PVNode> {
    let node = Arc::make_mut(arc);
    let sub_idx = (((count - 2) >> level) & MASK) as usize;
    match node {
        PVNode::Internal { children } => {
            if level == SHIFT_STEP {
                children[sub_idx]
                    .take()
                    .expect("trie invariant: pop path exists")
            } else {
                let child = children[sub_idx]
                    .as_mut()
                    .expect("trie invariant: pop path exists");
                let popped = pop_tail_in_place_pub(child, level - SHIFT_STEP, count);
                let child_empty = match child.as_ref() {
                    PVNode::Internal { children } => children.iter().all(|c| c.is_none()),
                    PVNode::Leaf { children } => children.iter().all(|v| v.is_nil()),
                };
                if child_empty {
                    children[sub_idx] = None;
                }
                popped
            }
        }
        PVNode::Leaf { .. } => unreachable!("pop_tail above leaf level on Leaf"),
    }
}

/// Build the empty-internal singleton used by transient pop when the
/// trie shrinks back to having no populated children.
pub(crate) fn empty_internal_pub() -> Arc<PVNode> {
    PVNode::empty_internal()
}

/// `new_path` exposed crate-private for transient root-growth path.
pub(crate) fn new_path_pub(level: i32, tail_node: Arc<PVNode>) -> Arc<PVNode> {
    new_path(level, tail_node)
}

/// `MASK` exposed crate-private (= 0x1f) so transient_vector.rs can
/// share the same per-level index extraction without redeclaring.
pub(crate) const MASK_PUB: i64 = MASK;
pub(crate) const SHIFT_STEP_PUB: i32 = SHIFT_STEP;
pub(crate) const BRANCHING_PUB: usize = BRANCHING;

/// Borrow-traversal of the trie to the leaf-block containing element
/// index `i`. Returns a reference to the underlying `[Value; 32]`,
/// alive for the lifetime of `root`. No Arc cloning, no dups —
/// purely chained `Arc::as_ref()` and `Option::as_ref()` borrows.
fn leaf_block_at<'a>(root: &'a Arc<PVNode>, shift: i32, i: i64) -> &'a [Value; BRANCHING] {
    let mut node: &'a PVNode = root.as_ref();
    let mut s = shift;
    while s > 0 {
        let idx = ((i >> s) & MASK) as usize;
        node = match node {
            PVNode::Internal { children } => children[idx]
                .as_ref()
                .expect("trie invariant: child present")
                .as_ref(),
            PVNode::Leaf { .. } => unreachable!("trie invariant: leaf above level 0"),
        };
        s -= SHIFT_STEP;
    }
    match node {
        PVNode::Leaf { children } => children,
        PVNode::Internal { .. } => unreachable!("trie invariant: internal at level 0"),
    }
}

// ============================================================================
// Internal helpers — hash + equiv
// ============================================================================

// Per-element hash and equiv walks use the same borrow-by-leaf-block
// shape as `IReduce::reduce_*`: each element is read from the leaf
// node's `[Value; 32]` directly, passed through dispatch with no
// dup/drop pair. The trie descent itself is zero-atomic.
//
// The hash is mixed in-place — running `hash` accumulator + count,
// finalized via `mix_coll_hash` at the end. Mirrors JVM Clojure's
// `Murmur3.hashOrdered` shape; no intermediate `Vec<i32>` allocation.

fn compute_vector_hash(this: Value) -> i32 {
    let body = unsafe { PersistentVector::body(this) };
    let count = body.count;
    let tail_off = tail_offset(body.count, body.tail.len());

    let mut hash: i32 = 1;
    let mut i: i64 = 0;
    while i < tail_off {
        let leaf = leaf_block_at(&body.root, body.shift, i);
        let block_start = i & !MASK;
        let block_end = (block_start + BRANCHING as i64).min(tail_off);
        for j in (i - block_start) as usize..(block_end - block_start) as usize {
            let h = clojure_rt_macros::dispatch!(IHash::hash, &[leaf[j]])
                .as_int().unwrap_or(0) as i32;
            hash = hash.wrapping_mul(31).wrapping_add(h);
        }
        i = block_end;
    }
    while i < count {
        let v = body.tail[(i - tail_off) as usize];
        let h = clojure_rt_macros::dispatch!(IHash::hash, &[v]).as_int().unwrap_or(0) as i32;
        hash = hash.wrapping_mul(31).wrapping_add(h);
        i += 1;
    }
    murmur3::mix_coll_hash(hash, count as i32)
}

fn vectors_equiv(a: Value, b: Value) -> bool {
    let ab = unsafe { PersistentVector::body(a) };
    let bb = unsafe { PersistentVector::body(b) };
    if ab.count != bb.count {
        return false;
    }
    // Lockstep walk: pull each element via `nth_borrowed` (zero-
    // atomic) and pass straight to IEquiv. The two vectors may have
    // different shift / tail-len, so we walk by index and let
    // `nth_borrowed` route each access to its own leaf or tail.
    let mut i: i64 = 0;
    while i < ab.count {
        let x = PersistentVector::nth_borrowed(a, i).expect("range");
        let y = PersistentVector::nth_borrowed(b, i).expect("range");
        let eq = clojure_rt_macros::dispatch!(IEquiv::equiv, &[x, y])
            .as_bool()
            .unwrap_or(false);
        if !eq {
            return false;
        }
        i += 1;
    }
    true
}
