//! `PersistentHashSet` — set-of-Values backed by a
//! `PersistentHashMap` whose entries are `(k, k)` pairs. Mirrors
//! `clojure.lang.PersistentHashSet` (JVM) and cljs's
//! `PersistentHashSet`.
//!
//! Most operations delegate to the underlying map: `contains?`/`get`
//! → `contains-key`/`get`, `conj` → `assoc` with the element as both
//! key and value, `disj` → `dissoc`. `count`, `seq`, `equiv`, `hash`
//! likewise leverage the map.
//!
//! Storing `(k, k)` rather than `(k, sentinel)` matches JVM/cljs and
//! avoids the "this isn't really part of the set" complication when
//! reading values out via `(get s x)` — the value returned IS the
//! element.

use core::sync::atomic::{AtomicI32, Ordering};
use std::sync::OnceLock;

use crate::hash::murmur3;
use crate::protocols::collection::ICollection;
use crate::protocols::counted::ICounted;
use crate::protocols::emptyable_collection::IEmptyableCollection;
use crate::protocols::equiv::IEquiv;
use crate::protocols::hash::IHash;
use crate::protocols::ifn::IFn;
use crate::protocols::lookup::ILookup;
use crate::protocols::meta::{IMeta, IWithMeta};
use crate::protocols::persistent_set::IPersistentSet;
use crate::protocols::seq::ISeqable;
use crate::protocols::set::ISet;
use crate::types::hash_map::{empty_hash_map, walk_entries, PersistentHashMap};
use crate::value::Value;

clojure_rt_macros::register_type! {
    pub struct PersistentHashSet {
        m:    Value,        // PersistentHashMap of (k, k) pairs
        meta: Value,
        hash: AtomicI32,
    }
}

static EMPTY_HASH_SET: OnceLock<Value> = OnceLock::new();

pub fn empty_hash_set() -> Value {
    let v = *EMPTY_HASH_SET.get_or_init(|| {
        let s = PersistentHashSet::alloc(
            empty_hash_map(),
            Value::NIL,
            AtomicI32::new(0),
        );
        crate::rc::share(s);
        s
    });
    crate::rc::dup(v);
    v
}

fn hash_set_type_id() -> crate::value::TypeId {
    *PERSISTENTHASHSET_TYPE_ID
        .get()
        .expect("PersistentHashSet: clojure_rt::init() not called")
}

impl PersistentHashSet {
    /// Build a set from a slice of items. Borrow semantics. Duplicate
    /// items collapse (same `IEquiv` semantics as the underlying map).
    pub fn from_items(items: &[Value]) -> Value {
        // Build the underlying map via the transient path (each
        // element appears as both key and value, so we can hand
        // PHM::from_kvs an interleaved [k, k, k, k, ...] slice).
        let mut kvs: Vec<Value> = Vec::with_capacity(items.len() * 2);
        for &x in items {
            kvs.push(x);
            kvs.push(x);
        }
        let m = PersistentHashMap::from_kvs(&kvs);
        Self::wrap_owned(m)
    }

    /// Construct from an already-owned underlying map. Caller transfers
    /// one ref of `m` to the new set.
    pub(crate) fn wrap_owned(m: Value) -> Value {
        PersistentHashSet::alloc(m, Value::NIL, AtomicI32::new(0))
    }

    pub fn count_of(this: Value) -> i64 {
        let body = unsafe { PersistentHashSet::body(this) };
        crate::rt::count(body.m).as_int().unwrap_or(0)
    }

    pub(crate) fn map_of<'a>(this: Value) -> Value {
        unsafe { PersistentHashSet::body(this) }.m
    }
}

clojure_rt_macros::implements! {
    impl ICounted for PersistentHashSet {
        fn count(this: Value) -> Value {
            Value::int(PersistentHashSet::count_of(this))
        }
    }
}

clojure_rt_macros::implements! {
    impl ILookup for PersistentHashSet {
        fn lookup_2(this: Value, k: Value) -> Value {
            let body = unsafe { PersistentHashSet::body(this) };
            if crate::rt::contains_key(body.m, k).as_bool().unwrap_or(false) {
                crate::rc::dup(k);
                k
            } else {
                Value::NIL
            }
        }
        fn lookup_3(this: Value, k: Value, not_found: Value) -> Value {
            let body = unsafe { PersistentHashSet::body(this) };
            if crate::rt::contains_key(body.m, k).as_bool().unwrap_or(false) {
                crate::rc::dup(k);
                k
            } else {
                crate::rc::dup(not_found);
                not_found
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl ICollection for PersistentHashSet {
        fn conj(this: Value, x: Value) -> Value {
            // (conj s x) — assoc x as both key and value in the underlying map.
            let body = unsafe { PersistentHashSet::body(this) };
            let new_m = crate::rt::assoc(body.m, x, x);
            // Carry meta forward (same shape as the source set).
            crate::rc::dup(body.meta);
            PersistentHashSet::alloc(new_m, body.meta, AtomicI32::new(0))
        }
    }
}

clojure_rt_macros::implements! {
    impl ISet for PersistentHashSet {
        fn disjoin(this: Value, k: Value) -> Value {
            let body = unsafe { PersistentHashSet::body(this) };
            let new_m = crate::rt::dissoc(body.m, k);
            crate::rc::dup(body.meta);
            PersistentHashSet::alloc(new_m, body.meta, AtomicI32::new(0))
        }
        fn contains(this: Value, k: Value) -> Value {
            let body = unsafe { PersistentHashSet::body(this) };
            crate::rt::contains_key(body.m, k)
        }
    }
}

clojure_rt_macros::implements! {
    impl IEmptyableCollection for PersistentHashSet {
        fn empty(this: Value) -> Value {
            let _ = this;
            empty_hash_set()
        }
    }
}

clojure_rt_macros::implements! {
    impl ISeqable for PersistentHashSet {
        fn seq(this: Value) -> Value {
            if PersistentHashSet::count_of(this) == 0 {
                Value::NIL
            } else {
                crate::types::hash_set_seq::HashSetSeq::start(this)
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl IHash for PersistentHashSet {
        fn hash(this: Value) -> Value {
            let body = unsafe { PersistentHashSet::body(this) };
            let cached = body.hash.load(Ordering::Relaxed);
            if cached != 0 {
                return Value::int(cached as i64);
            }
            // Unordered hash: sum of element hashes mixed via
            // mix_coll_hash with count. Different from the map hash
            // (which XORs k and v per entry); for sets each element
            // contributes once.
            let count = PersistentHashSet::count_of(this) as i32;
            let mut acc: i32 = 0;
            let m_root = PersistentHashMap::root_of(body.m);
            walk_entries(m_root, &mut |k, _v| {
                let kh = crate::rt::hash(k).as_int().unwrap_or(0) as i32;
                acc = acc.wrapping_add(kh);
            });
            let h = murmur3::mix_coll_hash(acc, count);
            body.hash.store(h, Ordering::Relaxed);
            Value::int(h as i64)
        }
    }
}

clojure_rt_macros::implements! {
    impl IEquiv for PersistentHashSet {
        fn equiv(this: Value, other: Value) -> Value {
            if other.tag != hash_set_type_id() {
                return Value::FALSE;
            }
            let a_m = PersistentHashSet::map_of(this);
            let b_m = PersistentHashSet::map_of(other);
            // Set equality reduces to map equality of the underlying
            // (k, k) maps — same keys + same values per key.
            crate::rt::equiv(a_m, b_m)
        }
    }
}

clojure_rt_macros::implements! {
    impl IMeta for PersistentHashSet {
        fn meta(this: Value) -> Value {
            let m = unsafe { PersistentHashSet::body(this) }.meta;
            crate::rc::dup(m);
            m
        }
    }
}

clojure_rt_macros::implements! {
    impl IWithMeta for PersistentHashSet {
        fn with_meta(this: Value, meta: Value) -> Value {
            let body = unsafe { PersistentHashSet::body(this) };
            crate::rc::dup(body.m);
            crate::rc::dup(meta);
            PersistentHashSet::alloc(body.m, meta, AtomicI32::new(0))
        }
    }
}

clojure_rt_macros::implements! { impl IPersistentSet for PersistentHashSet {} }

// Sets are callable: (s x) → x if (contains? s x), else nil.
//                    (s x not-found) → x if present, else not-found.
clojure_rt_macros::implements! {
    impl IFn for PersistentHashSet {
        fn invoke_2(this: Value, x: Value) -> Value {
            let body = unsafe { PersistentHashSet::body(this) };
            if crate::rt::contains_key(body.m, x).as_bool().unwrap_or(false) {
                crate::rc::dup(x);
                x
            } else {
                Value::NIL
            }
        }
        fn invoke_3(this: Value, x: Value, not_found: Value) -> Value {
            let body = unsafe { PersistentHashSet::body(this) };
            if crate::rt::contains_key(body.m, x).as_bool().unwrap_or(false) {
                crate::rc::dup(x);
                x
            } else {
                crate::rc::dup(not_found);
                not_found
            }
        }
    }
}
