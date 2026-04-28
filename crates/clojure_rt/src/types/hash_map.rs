//! `PersistentHashMap` — Bagwell HAMT (Hash Array Mapped Trie).
//!
//! Mirrors `clojure.lang.PersistentHashMap` (JVM) and cljs's
//! `PersistentHashMap`. Lookup is O(log32 N); in practice constant
//! for any realistic map size since the trie depth tops out at 7.
//!
//! Two node variants:
//! - **BitmapIndexed**: sparse storage. A 32-bit `bitmap` tracks
//!   which of the 32 possible hash-bit positions at this level are
//!   populated; `slots` holds exactly `popcount(bitmap)` slots in
//!   bitmap-bit-order. Each slot is either a key/value pair or a
//!   sub-node.
//! - **HashCollision**: linear-scan list of (k, v) pairs whose keys
//!   all hash to the same `i32` value. Used at the bottom of the
//!   trie when hash bits run out.
//!
//! Skipping the `ArrayNode` (dense-when-popcount-exceeds-16) variant
//! that JVM uses — `BitmapIndexed` scales correctly without it; the
//! ArrayNode variant is purely a perf optimization for memory layout.
//! Can be added later if profiling shows benefit.
//!
//! `HAMTSlot` is a Rust `enum` rather than the JVM's "nil-key marker
//! distinguishes entry from sub-node in a flat Object[]" trick — the
//! enum is type-safer, costs the same memory shape (Arc is 8B,
//! Value pair is 32B, the discriminant is free under niche
//! optimization for the Arc variant), and reads more clearly.

use core::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, OnceLock};

use crate::hash::murmur3;
use crate::protocols::associative::IAssociative;
use crate::protocols::collection::ICollection;
use crate::protocols::counted::ICounted;
use crate::protocols::emptyable_collection::IEmptyableCollection;
use crate::protocols::equiv::IEquiv;
use crate::protocols::hash::IHash;
use crate::protocols::indexed::IIndexed;
use crate::protocols::lookup::ILookup;
use crate::protocols::map::IMap;
use crate::protocols::meta::{IMeta, IWithMeta};
use crate::protocols::persistent_map::IPersistentMap;
use crate::protocols::seq::ISeqable;
use crate::types::map_entry::MapEntry;
use crate::value::Value;

const SHIFT: i32 = 5;
const MASK: u32 = 0x1f;

/// Maximum trie depth. With 5 bits/level and 32-bit hashes, levels
/// 0..6 use bits 0..29 plus a 2-bit residue at level 6. Beyond level
/// 6 we *must* use `HashCollision`.
const MAX_LEVEL: i32 = 7;

// ============================================================================
// HAMT internal types
// ============================================================================

pub(crate) enum HAMTSlot {
    Entry(Value, Value),
    Inner(Arc<HAMTNode>),
}

impl Drop for HAMTSlot {
    fn drop(&mut self) {
        match self {
            HAMTSlot::Entry(k, v) => {
                crate::rc::drop_value(*k);
                crate::rc::drop_value(*v);
            }
            HAMTSlot::Inner(_) => {} // Arc::drop chains
        }
    }
}

impl Clone for HAMTSlot {
    fn clone(&self) -> Self {
        match self {
            HAMTSlot::Entry(k, v) => {
                crate::rc::dup(*k);
                crate::rc::dup(*v);
                HAMTSlot::Entry(*k, *v)
            }
            HAMTSlot::Inner(arc) => HAMTSlot::Inner(arc.clone()),
        }
    }
}

pub(crate) enum HAMTNode {
    BitmapIndexed { bitmap: u32, slots: Vec<HAMTSlot> },
    HashCollision { hash: i32, kvs: Vec<Value> },
}

impl Clone for HAMTNode {
    fn clone(&self) -> Self {
        match self {
            HAMTNode::BitmapIndexed { bitmap, slots } => {
                // HAMTSlot::Clone dups Values for Entry slots and
                // Arc-clones for Inner slots — perfect for path-copy.
                HAMTNode::BitmapIndexed {
                    bitmap: *bitmap,
                    slots: slots.clone(),
                }
            }
            HAMTNode::HashCollision { hash, kvs } => {
                let mut new_kvs: Vec<Value> = Vec::with_capacity(kvs.len());
                for &v in kvs.iter() {
                    crate::rc::dup(v);
                    new_kvs.push(v);
                }
                HAMTNode::HashCollision { hash: *hash, kvs: new_kvs }
            }
        }
    }
}

impl Drop for HAMTNode {
    fn drop(&mut self) {
        if let HAMTNode::HashCollision { kvs, .. } = self {
            for v in kvs.iter() {
                crate::rc::drop_value(*v);
            }
        }
        // BitmapIndexed: HAMTSlot::Drop fires per slot via Vec drop.
    }
}

// ============================================================================
// HAMT operations — borrow semantics throughout. Caller's refs to k/v
// are unchanged; the trie dups what it stores.
// ============================================================================

/// Bit position within `bitmap` for `hash` at `level`.
#[inline]
fn bit_for(hash: i32, level: i32) -> u32 {
    1u32 << ((hash as u32 >> (level * SHIFT)) & MASK)
}

/// Storage-index in `slots` for the given bit. Equals
/// `popcount(bitmap & (bit - 1))`.
#[inline]
fn index_for(bitmap: u32, bit: u32) -> usize {
    (bitmap & (bit - 1)).count_ones() as usize
}

/// Build a sub-node containing two entries with given hashes. Used
/// when assoc on a BitmapIndexed slot collides with an existing
/// entry (different key, possibly same level-bits).
fn merge_entries(
    level: i32,
    hash0: i32, k0: Value, v0: Value,
    hash1: i32, k1: Value, v1: Value,
) -> Arc<HAMTNode> {
    if level >= MAX_LEVEL {
        // Hash bits exhausted — collision node.
        crate::rc::dup(k0); crate::rc::dup(v0);
        crate::rc::dup(k1); crate::rc::dup(v1);
        return Arc::new(HAMTNode::HashCollision {
            hash: hash0,
            kvs: vec![k0, v0, k1, v1],
        });
    }
    if hash0 == hash1 {
        // Same hash, different keys — recurse won't help. Collision.
        crate::rc::dup(k0); crate::rc::dup(v0);
        crate::rc::dup(k1); crate::rc::dup(v1);
        return Arc::new(HAMTNode::HashCollision {
            hash: hash0,
            kvs: vec![k0, v0, k1, v1],
        });
    }
    let bit0 = bit_for(hash0, level);
    let bit1 = bit_for(hash1, level);
    if bit0 == bit1 {
        // Same level-bits, different overall hash → deeper.
        let inner = merge_entries(level + 1, hash0, k0, v0, hash1, k1, v1);
        Arc::new(HAMTNode::BitmapIndexed {
            bitmap: bit0,
            slots: vec![HAMTSlot::Inner(inner)],
        })
    } else {
        // Different level-bits → both as entries at this level.
        crate::rc::dup(k0); crate::rc::dup(v0);
        crate::rc::dup(k1); crate::rc::dup(v1);
        let (lo_k, lo_v, hi_k, hi_v) = if bit0 < bit1 {
            (k0, v0, k1, v1)
        } else {
            (k1, v1, k0, v0)
        };
        Arc::new(HAMTNode::BitmapIndexed {
            bitmap: bit0 | bit1,
            slots: vec![
                HAMTSlot::Entry(lo_k, lo_v),
                HAMTSlot::Entry(hi_k, hi_v),
            ],
        })
    }
}

/// `(assoc node k v)` — returns a possibly-different node. Returns
/// (new_node, added_new_entry_flag). Borrow semantics on k and v.
fn assoc_in_node(
    node: &Arc<HAMTNode>, level: i32, hash: i32, k: Value, v: Value,
) -> (Arc<HAMTNode>, bool) {
    match node.as_ref() {
        HAMTNode::BitmapIndexed { bitmap, slots } => {
            let bit = bit_for(hash, level);
            if (*bitmap & bit) == 0 {
                // Empty slot — insert.
                let idx = index_for(*bitmap, bit);
                let mut new_slots: Vec<HAMTSlot> = Vec::with_capacity(slots.len() + 1);
                for (i, s) in slots.iter().enumerate() {
                    if i == idx {
                        crate::rc::dup(k); crate::rc::dup(v);
                        new_slots.push(HAMTSlot::Entry(k, v));
                    }
                    new_slots.push(s.clone());
                }
                if idx == slots.len() {
                    crate::rc::dup(k); crate::rc::dup(v);
                    new_slots.push(HAMTSlot::Entry(k, v));
                }
                (Arc::new(HAMTNode::BitmapIndexed {
                    bitmap: *bitmap | bit, slots: new_slots,
                }), true)
            } else {
                let idx = index_for(*bitmap, bit);
                match &slots[idx] {
                    HAMTSlot::Entry(stored_k, stored_v) => {
                        let stored_k = *stored_k;
                        let stored_v = *stored_v;
                        if crate::rt::equiv(stored_k, k).as_bool().unwrap_or(false) {
                            // Replace value.
                            let mut new_slots = slots.clone();
                            crate::rc::dup(k); crate::rc::dup(v);
                            new_slots[idx] = HAMTSlot::Entry(k, v);
                            (Arc::new(HAMTNode::BitmapIndexed {
                                bitmap: *bitmap, slots: new_slots,
                            }), false)
                        } else {
                            // Different key — split into sub-node.
                            let stored_hash = crate::rt::hash(stored_k)
                                .as_int().unwrap_or(0) as i32;
                            let inner = merge_entries(
                                level + 1,
                                stored_hash, stored_k, stored_v,
                                hash, k, v,
                            );
                            let mut new_slots = slots.clone();
                            new_slots[idx] = HAMTSlot::Inner(inner);
                            (Arc::new(HAMTNode::BitmapIndexed {
                                bitmap: *bitmap, slots: new_slots,
                            }), true)
                        }
                    }
                    HAMTSlot::Inner(child) => {
                        let (new_child, added) = assoc_in_node(child, level + 1, hash, k, v);
                        let mut new_slots = slots.clone();
                        new_slots[idx] = HAMTSlot::Inner(new_child);
                        (Arc::new(HAMTNode::BitmapIndexed {
                            bitmap: *bitmap, slots: new_slots,
                        }), added)
                    }
                }
            }
        }
        HAMTNode::HashCollision { hash: stored_hash, kvs } => {
            if hash != *stored_hash {
                // New entry collides at this level but has different
                // overall hash. Wrap in a BitmapIndexed at this level
                // containing the existing collision node + the new
                // entry.
                let bit_collision = bit_for(*stored_hash, level);
                let bit_new = bit_for(hash, level);
                if bit_collision == bit_new {
                    // Same level-bits despite different hashes —
                    // shouldn't happen if hashes differ at this
                    // level, but defensively recurse.
                    let bumped = Arc::new(HAMTNode::BitmapIndexed {
                        bitmap: bit_collision,
                        slots: vec![HAMTSlot::Inner(node.clone())],
                    });
                    return assoc_in_node(&bumped, level, hash, k, v);
                }
                crate::rc::dup(k); crate::rc::dup(v);
                let new_node = Arc::new(HAMTNode::BitmapIndexed {
                    bitmap: bit_collision | bit_new,
                    slots: if bit_collision < bit_new {
                        vec![HAMTSlot::Inner(node.clone()), HAMTSlot::Entry(k, v)]
                    } else {
                        vec![HAMTSlot::Entry(k, v), HAMTSlot::Inner(node.clone())]
                    },
                });
                return (new_node, true);
            }
            // Same hash — scan for matching key.
            let mut i = 0;
            while i < kvs.len() {
                if crate::rt::equiv(kvs[i], k).as_bool().unwrap_or(false) {
                    // Replace value.
                    let mut new_kvs: Vec<Value> = Vec::with_capacity(kvs.len());
                    for (j, &x) in kvs.iter().enumerate() {
                        if j == i + 1 {
                            crate::rc::dup(v);
                            new_kvs.push(v);
                        } else {
                            crate::rc::dup(x);
                            new_kvs.push(x);
                        }
                    }
                    return (Arc::new(HAMTNode::HashCollision {
                        hash: *stored_hash, kvs: new_kvs,
                    }), false);
                }
                i += 2;
            }
            // No match — append.
            let mut new_kvs: Vec<Value> = Vec::with_capacity(kvs.len() + 2);
            for &x in kvs.iter() {
                crate::rc::dup(x);
                new_kvs.push(x);
            }
            crate::rc::dup(k);
            crate::rc::dup(v);
            new_kvs.push(k);
            new_kvs.push(v);
            (Arc::new(HAMTNode::HashCollision {
                hash: *stored_hash, kvs: new_kvs,
            }), true)
        }
    }
}

/// `(dissoc node k)` — returns (new_node_or_none, removed_flag).
/// `None` means the node is now empty after removal.
fn dissoc_in_node(
    node: &Arc<HAMTNode>, level: i32, hash: i32, k: Value,
) -> (Option<Arc<HAMTNode>>, bool) {
    match node.as_ref() {
        HAMTNode::BitmapIndexed { bitmap, slots } => {
            let bit = bit_for(hash, level);
            if (*bitmap & bit) == 0 {
                return (Some(node.clone()), false);
            }
            let idx = index_for(*bitmap, bit);
            match &slots[idx] {
                HAMTSlot::Entry(stored_k, _) => {
                    if !crate::rt::equiv(*stored_k, k).as_bool().unwrap_or(false) {
                        return (Some(node.clone()), false);
                    }
                    // Remove this slot, clear bit.
                    let new_bitmap = *bitmap & !bit;
                    if new_bitmap == 0 {
                        return (None, true);
                    }
                    let mut new_slots: Vec<HAMTSlot> = Vec::with_capacity(slots.len() - 1);
                    for (i, s) in slots.iter().enumerate() {
                        if i == idx { continue; }
                        new_slots.push(s.clone());
                    }
                    (Some(Arc::new(HAMTNode::BitmapIndexed {
                        bitmap: new_bitmap, slots: new_slots,
                    })), true)
                }
                HAMTSlot::Inner(child) => {
                    let (new_child, removed) = dissoc_in_node(child, level + 1, hash, k);
                    if !removed {
                        return (Some(node.clone()), false);
                    }
                    match new_child {
                        Some(nc) => {
                            let mut new_slots = slots.clone();
                            new_slots[idx] = HAMTSlot::Inner(nc);
                            (Some(Arc::new(HAMTNode::BitmapIndexed {
                                bitmap: *bitmap, slots: new_slots,
                            })), true)
                        }
                        None => {
                            let new_bitmap = *bitmap & !bit;
                            if new_bitmap == 0 {
                                return (None, true);
                            }
                            let mut new_slots: Vec<HAMTSlot> = Vec::with_capacity(slots.len() - 1);
                            for (i, s) in slots.iter().enumerate() {
                                if i == idx { continue; }
                                new_slots.push(s.clone());
                            }
                            (Some(Arc::new(HAMTNode::BitmapIndexed {
                                bitmap: new_bitmap, slots: new_slots,
                            })), true)
                        }
                    }
                }
            }
        }
        HAMTNode::HashCollision { hash: stored_hash, kvs } => {
            if hash != *stored_hash {
                return (Some(node.clone()), false);
            }
            let mut found_at: Option<usize> = None;
            let mut i = 0;
            while i < kvs.len() {
                if crate::rt::equiv(kvs[i], k).as_bool().unwrap_or(false) {
                    found_at = Some(i);
                    break;
                }
                i += 2;
            }
            let Some(idx) = found_at else {
                return (Some(node.clone()), false);
            };
            if kvs.len() == 2 {
                return (None, true);
            }
            let mut new_kvs: Vec<Value> = Vec::with_capacity(kvs.len() - 2);
            for (i, &x) in kvs.iter().enumerate() {
                if i == idx || i == idx + 1 { continue; }
                crate::rc::dup(x);
                new_kvs.push(x);
            }
            (Some(Arc::new(HAMTNode::HashCollision {
                hash: *stored_hash, kvs: new_kvs,
            })), true)
        }
    }
}

/// `(get node k)` — returns the value, or `Value::NIL` on miss with
/// `false` second tuple flag. The flag distinguishes
/// "key-present-with-nil-value" from "key-missing" for callers that
/// need that.
pub(crate) fn lookup_in_node(
    node: &Arc<HAMTNode>, level: i32, hash: i32, k: Value,
) -> Option<Value> {
    match node.as_ref() {
        HAMTNode::BitmapIndexed { bitmap, slots } => {
            let bit = bit_for(hash, level);
            if (*bitmap & bit) == 0 {
                return None;
            }
            let idx = index_for(*bitmap, bit);
            match &slots[idx] {
                HAMTSlot::Entry(stored_k, stored_v) => {
                    if crate::rt::equiv(*stored_k, k).as_bool().unwrap_or(false) {
                        Some(*stored_v)
                    } else {
                        None
                    }
                }
                HAMTSlot::Inner(child) => lookup_in_node(child, level + 1, hash, k),
            }
        }
        HAMTNode::HashCollision { hash: stored_hash, kvs } => {
            if hash != *stored_hash {
                return None;
            }
            let mut i = 0;
            while i < kvs.len() {
                if crate::rt::equiv(kvs[i], k).as_bool().unwrap_or(false) {
                    return Some(kvs[i + 1]);
                }
                i += 2;
            }
            None
        }
    }
}

// ============================================================================
// PersistentHashMap Value type
// ============================================================================

clojure_rt_macros::register_type! {
    pub struct PersistentHashMap {
        count: i64,
        root:  Arc<HAMTNode>,
        meta:  Value,
        hash:  AtomicI32,
    }
}

static EMPTY_HASH_MAP_SINGLETON: OnceLock<Value> = OnceLock::new();

/// Canonical empty hash-map. Same publication discipline as the
/// other singletons.
pub fn empty_hash_map() -> Value {
    let v = *EMPTY_HASH_MAP_SINGLETON.get_or_init(|| {
        let v = PersistentHashMap::alloc(
            0,
            Arc::new(HAMTNode::BitmapIndexed { bitmap: 0, slots: Vec::new() }),
            Value::NIL,
            AtomicI32::new(0),
        );
        crate::rc::share(v);
        v
    });
    crate::rc::dup(v);
    v
}

fn hash_map_type_id() -> crate::value::TypeId {
    *PERSISTENTHASHMAP_TYPE_ID
        .get()
        .expect("PersistentHashMap: clojure_rt::init() not called")
}

impl PersistentHashMap {
    pub fn count_of(this: Value) -> i64 {
        unsafe { PersistentHashMap::body(this) }.count
    }

    /// Crate-private root accessor for HashMapSeq.
    pub(crate) fn root_of<'a>(this: Value) -> &'a Arc<HAMTNode> {
        unsafe { &PersistentHashMap::body(this).root }
    }

    /// Borrow-semantics assoc.
    pub fn assoc_kv(this: Value, k: Value, v: Value) -> Value {
        let body = unsafe { PersistentHashMap::body(this) };
        let hash = crate::rt::hash(k).as_int().unwrap_or(0) as i32;
        let (new_root, added) = assoc_in_node(&body.root, 0, hash, k, v);
        crate::rc::dup(body.meta);
        PersistentHashMap::alloc(
            body.count + if added { 1 } else { 0 },
            new_root,
            body.meta,
            AtomicI32::new(0),
        )
    }

    /// Borrow-semantics dissoc. Returns `this` (with a fresh ref) if
    /// `k` not present.
    pub fn dissoc_k(this: Value, k: Value) -> Value {
        let body = unsafe { PersistentHashMap::body(this) };
        let hash = crate::rt::hash(k).as_int().unwrap_or(0) as i32;
        let (new_root, removed) = dissoc_in_node(&body.root, 0, hash, k);
        if !removed {
            crate::rc::dup(this);
            return this;
        }
        let root = new_root.unwrap_or_else(|| {
            Arc::new(HAMTNode::BitmapIndexed { bitmap: 0, slots: Vec::new() })
        });
        crate::rc::dup(body.meta);
        PersistentHashMap::alloc(
            body.count - 1,
            root,
            body.meta,
            AtomicI32::new(0),
        )
    }

    /// Build from a flat `[k0, v0, …]` slice. Borrow semantics.
    pub fn from_kvs(items: &[Value]) -> Value {
        debug_assert!(items.len() % 2 == 0);
        let mut m = empty_hash_map();
        let mut i = 0;
        while i < items.len() {
            let nm = PersistentHashMap::assoc_kv(m, items[i], items[i + 1]);
            crate::rc::drop_value(m);
            m = nm;
            i += 2;
        }
        m
    }
}

// ============================================================================
// Protocol impls
// ============================================================================

clojure_rt_macros::implements! {
    impl ICounted for PersistentHashMap {
        fn count(this: Value) -> Value {
            Value::int(PersistentHashMap::count_of(this))
        }
    }
}

clojure_rt_macros::implements! {
    impl ILookup for PersistentHashMap {
        fn lookup_2(this: Value, k: Value) -> Value {
            let body = unsafe { PersistentHashMap::body(this) };
            let hash = crate::rt::hash(k).as_int().unwrap_or(0) as i32;
            match lookup_in_node(&body.root, 0, hash, k) {
                Some(v) => { crate::rc::dup(v); v }
                None => Value::NIL,
            }
        }
        fn lookup_3(this: Value, k: Value, not_found: Value) -> Value {
            let body = unsafe { PersistentHashMap::body(this) };
            let hash = crate::rt::hash(k).as_int().unwrap_or(0) as i32;
            match lookup_in_node(&body.root, 0, hash, k) {
                Some(v) => { crate::rc::dup(v); v }
                None => { crate::rc::dup(not_found); not_found }
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl IAssociative for PersistentHashMap {
        fn assoc(this: Value, k: Value, v: Value) -> Value {
            PersistentHashMap::assoc_kv(this, k, v)
        }
        fn contains_key(this: Value, k: Value) -> Value {
            let body = unsafe { PersistentHashMap::body(this) };
            let hash = crate::rt::hash(k).as_int().unwrap_or(0) as i32;
            if lookup_in_node(&body.root, 0, hash, k).is_some() {
                Value::TRUE
            } else {
                Value::FALSE
            }
        }
        fn find(this: Value, k: Value) -> Value {
            let body = unsafe { PersistentHashMap::body(this) };
            let hash = crate::rt::hash(k).as_int().unwrap_or(0) as i32;
            match lookup_in_node(&body.root, 0, hash, k) {
                Some(v) => MapEntry::new(k, v),
                None => Value::NIL,
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl IMap for PersistentHashMap {
        fn dissoc(this: Value, k: Value) -> Value {
            PersistentHashMap::dissoc_k(this, k)
        }
    }
}

clojure_rt_macros::implements! {
    impl ICollection for PersistentHashMap {
        fn conj(this: Value, x: Value) -> Value {
            // (conj m e) for a map: e must be MapEntry-shaped.
            let me_id = crate::types::map_entry::MAPENTRY_TYPE_ID
                .get().copied().unwrap_or(0);
            if x.tag == me_id {
                let k = MapEntry::key_borrowed(x);
                let v = MapEntry::val_borrowed(x);
                return PersistentHashMap::assoc_kv(this, k, v);
            }
            if !crate::protocol::satisfies(&IIndexed::NTH_2, x) {
                return crate::exception::make_foreign(format!(
                    "Don't know how to conj {} onto a map",
                    if x.is_heap() { "<heap>" } else { "<primitive>" }
                ));
            }
            let k = crate::rt::nth(x, Value::int(0));
            let v = crate::rt::nth(x, Value::int(1));
            let r = PersistentHashMap::assoc_kv(this, k, v);
            crate::rc::drop_value(k);
            crate::rc::drop_value(v);
            r
        }
    }
}

clojure_rt_macros::implements! {
    impl IEmptyableCollection for PersistentHashMap {
        fn empty(this: Value) -> Value {
            let _ = this;
            empty_hash_map()
        }
    }
}

clojure_rt_macros::implements! {
    impl ISeqable for PersistentHashMap {
        fn seq(this: Value) -> Value {
            if PersistentHashMap::count_of(this) == 0 {
                Value::NIL
            } else {
                crate::types::hash_map_seq::HashMapSeq::start(this)
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl IHash for PersistentHashMap {
        fn hash(this: Value) -> Value {
            let body = unsafe { PersistentHashMap::body(this) };
            let cached = body.hash.load(Ordering::Relaxed);
            if cached != 0 {
                return Value::int(cached as i64);
            }
            // Same shape as PAM: sum of (hash(k) ^ hash(v)) per entry,
            // mixed via mix_coll_hash with count.
            let mut acc: i32 = 0;
            walk_entries(&body.root, &mut |k, v| {
                let kh = crate::rt::hash(k).as_int().unwrap_or(0) as i32;
                let vh = crate::rt::hash(v).as_int().unwrap_or(0) as i32;
                acc = acc.wrapping_add(kh ^ vh);
            });
            let h = murmur3::mix_coll_hash(acc, body.count as i32);
            body.hash.store(h, Ordering::Relaxed);
            Value::int(h as i64)
        }
    }
}

clojure_rt_macros::implements! {
    impl IEquiv for PersistentHashMap {
        fn equiv(this: Value, other: Value) -> Value {
            // Same-type (HM == HM) or cross-type (HM == AM): compare
            // count + each-key-lookup. Maps are equal iff same key
            // set and same values per key.
            let am_id = crate::types::array_map::PERSISTENTARRAYMAP_TYPE_ID
                .get().copied().unwrap_or(0);
            if other.tag != hash_map_type_id() && other.tag != am_id {
                return Value::FALSE;
            }
            let a_count = PersistentHashMap::count_of(this);
            let b_count = crate::rt::count(other).as_int().unwrap_or(-1);
            if a_count != b_count {
                return Value::FALSE;
            }
            let body = unsafe { PersistentHashMap::body(this) };
            let mut equal = true;
            walk_entries(&body.root, &mut |k, v| {
                if !equal { return; }
                // Use lookup_3 with a sentinel via Value::NIL and a
                // contains-key check to disambiguate
                // missing-vs-present-nil.
                let cf = crate::rt::contains_key(other, k).as_bool().unwrap_or(false);
                if !cf { equal = false; return; }
                let other_v = crate::rt::get(other, k);
                if !crate::rt::equiv(v, other_v).as_bool().unwrap_or(false) {
                    equal = false;
                }
                crate::rc::drop_value(other_v);
            });
            if equal { Value::TRUE } else { Value::FALSE }
        }
    }
}

clojure_rt_macros::implements! {
    impl IMeta for PersistentHashMap {
        fn meta(this: Value) -> Value {
            let m = unsafe { PersistentHashMap::body(this) }.meta;
            crate::rc::dup(m);
            m
        }
    }
}

clojure_rt_macros::implements! {
    impl IWithMeta for PersistentHashMap {
        fn with_meta(this: Value, meta: Value) -> Value {
            let body = unsafe { PersistentHashMap::body(this) };
            crate::rc::dup(meta);
            PersistentHashMap::alloc(
                body.count,
                body.root.clone(),
                meta,
                AtomicI32::new(0),
            )
        }
    }
}

clojure_rt_macros::implements! { impl IPersistentMap for PersistentHashMap {} }

// ============================================================================
// Internal: depth-first walk yielding (k, v) pairs by Value-borrow
// ============================================================================

pub(crate) fn walk_entries<F: FnMut(Value, Value)>(node: &Arc<HAMTNode>, f: &mut F) {
    match node.as_ref() {
        HAMTNode::BitmapIndexed { slots, .. } => {
            for slot in slots.iter() {
                match slot {
                    HAMTSlot::Entry(k, v) => f(*k, *v),
                    HAMTSlot::Inner(child) => walk_entries(child, f),
                }
            }
        }
        HAMTNode::HashCollision { kvs, .. } => {
            let mut i = 0;
            while i < kvs.len() {
                f(kvs[i], kvs[i + 1]);
                i += 2;
            }
        }
    }
}
