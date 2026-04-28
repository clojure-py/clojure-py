//! `PersistentArrayMap` — small-N map backed by a flat array of
//! interleaved key/value pairs (`[k0, v0, k1, v1, …]`). Linear-scan
//! lookup via `IEquiv`. Insertion order is preserved: assoc with a
//! new key appends, assoc with an existing key replaces in place.
//!
//! Mirrors `clojure.lang.PersistentArrayMap` (JVM) and cljs's
//! `PersistentArrayMap`. The promotion threshold to a HAMT-backed
//! `PersistentHashMap` is 8 entries (16 array slots) in JVM; we
//! follow that constant but the actual HAMT lands in a follow-up
//! slice — for now this map grows unbounded as an array.
//!
//! The `IPersistentMap` marker, `IAssociative` (assoc, contains_key,
//! find), `IMap` (dissoc), `ILookup` (lookup_2/3), `ICollection`
//! (conj of MapEntry-shaped), `ICounted`, `IEmptyableCollection`,
//! `ISeqable` (returns `ArrayMapSeq`), `IHash` (unordered: each
//! entry contributes `(hash k) ^ (hash v)`, summed), `IEquiv` (same
//! key set + values), `IMeta`/`IWithMeta` are all implemented.

use core::sync::atomic::{AtomicI32, Ordering};
use std::sync::OnceLock;

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

clojure_rt_macros::register_type! {
    pub struct PersistentArrayMap {
        kvs:  Box<[Value]>,   // interleaved [k0, v0, k1, v1, …]
        meta: Value,
        hash: AtomicI32,      // 0 = uncomputed
    }
}

static EMPTY_ARRAY_MAP_SINGLETON: OnceLock<Value> = OnceLock::new();

/// Canonical empty array-map. Same publication discipline as
/// `empty_vector` / `empty_list` — `share` before the OnceLock makes
/// it visible to other threads.
pub fn empty_array_map() -> Value {
    let v = *EMPTY_ARRAY_MAP_SINGLETON.get_or_init(|| {
        let v = PersistentArrayMap::alloc(
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

fn array_map_type_id() -> crate::value::TypeId {
    *PERSISTENTARRAYMAP_TYPE_ID
        .get()
        .expect("PersistentArrayMap: clojure_rt::init() not called")
}

impl PersistentArrayMap {
    /// Build a map from a flat `[k0, v0, k1, v1, …]` slice. **Borrow
    /// semantics**: caller's refs are unchanged; the map dups each
    /// element for its own storage. Duplicate keys: later occurrence
    /// wins (mirrors JVM `PersistentArrayMap.create`).
    pub fn from_kvs(items: &[Value]) -> Value {
        debug_assert!(items.len() % 2 == 0, "from_kvs: odd-length kv slice");
        let mut m = empty_array_map();
        let mut i = 0;
        while i < items.len() {
            let nm = PersistentArrayMap::assoc_kv(m, items[i], items[i + 1]);
            crate::rc::drop_value(m);
            m = nm;
            i += 2;
        }
        m
    }

    /// Number of key/value pairs.
    pub fn count_of(this: Value) -> i64 {
        let body = unsafe { PersistentArrayMap::body(this) };
        (body.kvs.len() / 2) as i64
    }

    /// Linear search by `IEquiv`-based key match. Returns the slot
    /// *index* (offset into `kvs` for the matching key, i.e. 0, 2, 4,
    /// …), or `None` on miss.
    fn index_of(this: Value, k: Value) -> Option<usize> {
        let body = unsafe { PersistentArrayMap::body(this) };
        let mut i = 0;
        while i < body.kvs.len() {
            let stored_k = body.kvs[i];
            let eq = clojure_rt_macros::dispatch!(IEquiv::equiv, &[stored_k, k])
                .as_bool().unwrap_or(false);
            if eq {
                return Some(i);
            }
            i += 2;
        }
        None
    }

    /// Path-copy assoc. **Borrow semantics**: caller's refs to `k`
    /// and `v` are unchanged; the new map dups what it stores.
    pub fn assoc_kv(this: Value, k: Value, v: Value) -> Value {
        let body = unsafe { PersistentArrayMap::body(this) };
        if let Some(idx) = Self::index_of(this, k) {
            // Replace value in place; key stays.
            let mut new_kvs: Vec<Value> = Vec::with_capacity(body.kvs.len());
            for (i, &x) in body.kvs.iter().enumerate() {
                if i == idx {
                    crate::rc::dup(x); // existing key — survives
                    new_kvs.push(x);
                } else if i == idx + 1 {
                    crate::rc::dup(v); // borrow v → dup for storage
                    new_kvs.push(v);
                } else {
                    crate::rc::dup(x);
                    new_kvs.push(x);
                }
            }
            crate::rc::dup(body.meta);
            return PersistentArrayMap::alloc(
                new_kvs.into_boxed_slice(),
                body.meta,
                AtomicI32::new(0),
            );
        }
        // Append [k, v] at the end.
        let mut new_kvs: Vec<Value> = Vec::with_capacity(body.kvs.len() + 2);
        for &x in body.kvs.iter() {
            crate::rc::dup(x);
            new_kvs.push(x);
        }
        crate::rc::dup(k);
        new_kvs.push(k);
        crate::rc::dup(v);
        new_kvs.push(v);
        crate::rc::dup(body.meta);
        PersistentArrayMap::alloc(
            new_kvs.into_boxed_slice(),
            body.meta,
            AtomicI32::new(0),
        )
    }

    /// Path-copy dissoc. Returns `this` (with a fresh ref) if `k` is
    /// not present.
    pub fn dissoc_k(this: Value, k: Value) -> Value {
        let body = unsafe { PersistentArrayMap::body(this) };
        let Some(idx) = Self::index_of(this, k) else {
            crate::rc::dup(this);
            return this;
        };
        let mut new_kvs: Vec<Value> = Vec::with_capacity(body.kvs.len() - 2);
        for (i, &x) in body.kvs.iter().enumerate() {
            if i == idx || i == idx + 1 {
                continue;
            }
            crate::rc::dup(x);
            new_kvs.push(x);
        }
        crate::rc::dup(body.meta);
        PersistentArrayMap::alloc(
            new_kvs.into_boxed_slice(),
            body.meta,
            AtomicI32::new(0),
        )
    }

    /// Borrowed read at the storage offset. Used by `ArrayMapSeq` to
    /// vend `MapEntry`s without re-walking via index_of.
    pub(crate) fn kv_at(this: Value, slot: usize) -> (Value, Value) {
        let body = unsafe { PersistentArrayMap::body(this) };
        (body.kvs[slot], body.kvs[slot + 1])
    }
}

// ============================================================================
// Protocol impls
// ============================================================================

clojure_rt_macros::implements! {
    impl ICounted for PersistentArrayMap {
        fn count(this: Value) -> Value {
            Value::int(PersistentArrayMap::count_of(this))
        }
    }
}

clojure_rt_macros::implements! {
    impl ILookup for PersistentArrayMap {
        fn lookup_2(this: Value, k: Value) -> Value {
            match PersistentArrayMap::index_of(this, k) {
                Some(idx) => {
                    let v = unsafe { PersistentArrayMap::body(this) }.kvs[idx + 1];
                    crate::rc::dup(v);
                    v
                }
                None => Value::NIL,
            }
        }
        fn lookup_3(this: Value, k: Value, not_found: Value) -> Value {
            match PersistentArrayMap::index_of(this, k) {
                Some(idx) => {
                    let v = unsafe { PersistentArrayMap::body(this) }.kvs[idx + 1];
                    crate::rc::dup(v);
                    v
                }
                None => {
                    crate::rc::dup(not_found);
                    not_found
                }
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl IAssociative for PersistentArrayMap {
        fn assoc(this: Value, k: Value, v: Value) -> Value {
            // assoc_kv borrows; no pre-dup needed.
            PersistentArrayMap::assoc_kv(this, k, v)
        }
        fn contains_key(this: Value, k: Value) -> Value {
            if PersistentArrayMap::index_of(this, k).is_some() {
                Value::TRUE
            } else {
                Value::FALSE
            }
        }
        fn find(this: Value, k: Value) -> Value {
            match PersistentArrayMap::index_of(this, k) {
                Some(idx) => {
                    let body = unsafe { PersistentArrayMap::body(this) };
                    let stored_k = body.kvs[idx];
                    let stored_v = body.kvs[idx + 1];
                    // Borrow semantics: MapEntry::new dups internally.
                    MapEntry::new(stored_k, stored_v)
                }
                None => Value::NIL,
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl IMap for PersistentArrayMap {
        fn dissoc(this: Value, k: Value) -> Value {
            PersistentArrayMap::dissoc_k(this, k)
        }
    }
}

clojure_rt_macros::implements! {
    impl ICollection for PersistentArrayMap {
        fn conj(this: Value, x: Value) -> Value {
            // `(conj m e)` for a map: e must be MapEntry-shaped — i.e.
            // either a real MapEntry or a 2-element vector. We borrow
            // k/v out of MapEntry when possible (zero-cost) or pull
            // them via IIndexed::nth (which dups; we drop after).
            if x.tag == crate::types::map_entry::MAPENTRY_TYPE_ID
                .get().copied().unwrap_or(0)
            {
                let k = MapEntry::key_borrowed(x);
                let v = MapEntry::val_borrowed(x);
                return PersistentArrayMap::assoc_kv(this, k, v);
            }
            // Fallback: assume IIndexed of count 2.
            if !crate::protocol::satisfies(&IIndexed::NTH_2, x) {
                return crate::exception::make_foreign(format!(
                    "Don't know how to conj {} onto a map",
                    if x.is_heap() { "<heap>" } else { "<primitive>" }
                ));
            }
            let k = clojure_rt_macros::dispatch!(IIndexed::nth, &[x, Value::int(0)]);
            let v = clojure_rt_macros::dispatch!(IIndexed::nth, &[x, Value::int(1)]);
            let r = PersistentArrayMap::assoc_kv(this, k, v);
            crate::rc::drop_value(k);
            crate::rc::drop_value(v);
            r
        }
    }
}

clojure_rt_macros::implements! {
    impl IEmptyableCollection for PersistentArrayMap {
        fn empty(this: Value) -> Value {
            let _ = this;
            empty_array_map()
        }
    }
}

clojure_rt_macros::implements! {
    impl ISeqable for PersistentArrayMap {
        fn seq(this: Value) -> Value {
            if PersistentArrayMap::count_of(this) == 0 {
                Value::NIL
            } else {
                crate::types::array_map_seq::ArrayMapSeq::start(this)
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl IHash for PersistentArrayMap {
        fn hash(this: Value) -> Value {
            let body = unsafe { PersistentArrayMap::body(this) };
            let cached = body.hash.load(Ordering::Relaxed);
            if cached != 0 {
                return Value::int(cached as i64);
            }
            // Unordered hash: sum of (hash(k) ^ hash(v)) over all
            // entries; finalize via mix_coll_hash with count.
            let mut acc: i32 = 0;
            let mut i = 0;
            while i < body.kvs.len() {
                let kh = clojure_rt_macros::dispatch!(IHash::hash, &[body.kvs[i]])
                    .as_int().unwrap_or(0) as i32;
                let vh = clojure_rt_macros::dispatch!(IHash::hash, &[body.kvs[i + 1]])
                    .as_int().unwrap_or(0) as i32;
                acc = acc.wrapping_add(kh ^ vh);
                i += 2;
            }
            let h = murmur3::mix_coll_hash(acc, (body.kvs.len() / 2) as i32);
            body.hash.store(h, Ordering::Relaxed);
            Value::int(h as i64)
        }
    }
}

clojure_rt_macros::implements! {
    impl IEquiv for PersistentArrayMap {
        fn equiv(this: Value, other: Value) -> Value {
            if other.tag != array_map_type_id() {
                // Cross-type map equiv (e.g. ArrayMap vs HashMap)
                // lands when HashMap arrives; for now reject.
                return Value::FALSE;
            }
            if maps_equiv(this, other) { Value::TRUE } else { Value::FALSE }
        }
    }
}

clojure_rt_macros::implements! {
    impl IMeta for PersistentArrayMap {
        fn meta(this: Value) -> Value {
            let m = unsafe { PersistentArrayMap::body(this) }.meta;
            crate::rc::dup(m);
            m
        }
    }
}

clojure_rt_macros::implements! {
    impl IWithMeta for PersistentArrayMap {
        fn with_meta(this: Value, meta: Value) -> Value {
            let body = unsafe { PersistentArrayMap::body(this) };
            crate::rc::dup(meta);
            let mut new_kvs: Vec<Value> = Vec::with_capacity(body.kvs.len());
            for &x in body.kvs.iter() {
                crate::rc::dup(x);
                new_kvs.push(x);
            }
            PersistentArrayMap::alloc(
                new_kvs.into_boxed_slice(),
                meta,
                AtomicI32::new(0),
            )
        }
    }
}

clojure_rt_macros::implements! { impl IPersistentMap for PersistentArrayMap {} }

// ============================================================================
// Internal helpers
// ============================================================================

fn maps_equiv(a: Value, b: Value) -> bool {
    let ab = unsafe { PersistentArrayMap::body(a) };
    let bb = unsafe { PersistentArrayMap::body(b) };
    if ab.kvs.len() != bb.kvs.len() {
        return false;
    }
    // For each entry in `a`, look up its key in `b` and compare
    // values. Symmetric because we already checked equal cardinality.
    let mut i = 0;
    while i < ab.kvs.len() {
        let k = ab.kvs[i];
        let av = ab.kvs[i + 1];
        let Some(bidx) = PersistentArrayMap::index_of(b, k) else {
            return false;
        };
        let bv = bb.kvs[bidx + 1];
        let v_eq = clojure_rt_macros::dispatch!(IEquiv::equiv, &[av, bv])
            .as_bool().unwrap_or(false);
        if !v_eq {
            return false;
        }
        i += 2;
    }
    true
}
