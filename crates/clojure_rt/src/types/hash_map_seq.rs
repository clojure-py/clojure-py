//! `HashMapSeq` — depth-first cursor over a `PersistentHashMap`'s
//! HAMT. Yields `MapEntry`s in trie-walk order. The order isn't
//! defined by the Clojure semantics (maps are unordered), it's just
//! deterministic for a given map.
//!
//! Implementation: walk the HAMT recursively, collecting (k, v)
//! pairs into a `Vec<(Value, Value)>` materialized at construction.
//! For `first`/`rest`/`next` we just index into this Vec.
//!
//! That's a single up-front allocation per seq instance — not the
//! most cache-friendly compared to a stack-based cursor, but it's
//! simple, correct, and matches the JVM `NodeIter` semantics. A
//! lazier walk that only materializes per chunk lands when chunked
//! seqs for maps become a hot path.

use core::sync::atomic::{AtomicI32, Ordering};

use crate::hash::murmur3;
use crate::protocols::counted::ICounted;
use crate::protocols::equiv::IEquiv;
use crate::protocols::hash::IHash;
use crate::protocols::meta::{IMeta, IWithMeta};
use crate::protocols::seq::{INext, ISeq, ISeqable};
use crate::protocols::sequential::ISequential;
use crate::types::hash_map::{walk_entries, PersistentHashMap};
use crate::types::map_entry::MapEntry;
use crate::value::Value;

clojure_rt_macros::register_type! {
    pub struct HashMapSeq {
        map:   Value,         // PersistentHashMap (ref kept alive)
        // Materialized entries: interleaved [k0, v0, k1, v1, …]. Each
        // element is dup'd into the seq's storage at construction;
        // the macro-generated destructor decrefs each on drop. (Yes,
        // this means we pay one dup per entry beyond what the HAMT
        // already holds — needed because Box<[Value]> is auto-
        // decremented on drop and we'd otherwise double-free.)
        entries: Box<[Value]>,
        index: i64,           // current pair start (0, 2, 4, …)
        meta:  Value,
        hash:  AtomicI32,
    }
}

impl HashMapSeq {
    /// Construct a seq positioned at the start of `map`.
    pub fn start(map: Value) -> Value {
        let count = PersistentHashMap::count_of(map);
        let mut entries: Vec<Value> = Vec::with_capacity((count * 2) as usize);
        let root = PersistentHashMap::root_of(map);
        // Dup each (k, v) into the seq's owned storage. The HAMT
        // still owns its refs; the seq owns its own copy. Drop on
        // either is independent.
        walk_entries(root, &mut |k, v| {
            crate::rc::dup(k);
            crate::rc::dup(v);
            entries.push(k);
            entries.push(v);
        });
        crate::rc::dup(map);
        HashMapSeq::alloc(
            map,
            entries.into_boxed_slice(),
            0,
            Value::NIL,
            AtomicI32::new(0),
        )
    }
}

clojure_rt_macros::implements! {
    impl ISeq for HashMapSeq {
        fn first(this: Value) -> Value {
            let body = unsafe { HashMapSeq::body(this) };
            let i = body.index as usize;
            // entries[i], entries[i+1] are borrowed from the HAMT
            // (kept alive by body.map). MapEntry::new dups both.
            MapEntry::new(body.entries[i], body.entries[i + 1])
        }
        fn rest(this: Value) -> Value {
            let body = unsafe { HashMapSeq::body(this) };
            let next_idx = body.index + 2;
            if (next_idx as usize) >= body.entries.len() {
                crate::types::list::empty_list()
            } else {
                crate::rc::dup(body.map);
                // Share the entries Box by cloning into a fresh one.
                // Cheap because each Value is Copy and the HAMT owns
                // the underlying refs (we don't dup them).
                let mut new_entries: Vec<Value> = Vec::with_capacity(body.entries.len());
                for &v in body.entries.iter() {
                    crate::rc::dup(v);
                    new_entries.push(v);
                }
                HashMapSeq::alloc(
                    body.map,
                    new_entries.into_boxed_slice(),
                    next_idx,
                    Value::NIL,
                    AtomicI32::new(0),
                )
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl INext for HashMapSeq {
        fn next(this: Value) -> Value {
            let body = unsafe { HashMapSeq::body(this) };
            let next_idx = body.index + 2;
            if (next_idx as usize) >= body.entries.len() {
                Value::NIL
            } else {
                crate::rc::dup(body.map);
                let mut new_entries: Vec<Value> = Vec::with_capacity(body.entries.len());
                for &v in body.entries.iter() {
                    crate::rc::dup(v);
                    new_entries.push(v);
                }
                HashMapSeq::alloc(
                    body.map,
                    new_entries.into_boxed_slice(),
                    next_idx,
                    Value::NIL,
                    AtomicI32::new(0),
                )
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl ISeqable for HashMapSeq {
        fn seq(this: Value) -> Value {
            crate::rc::dup(this);
            this
        }
    }
}

clojure_rt_macros::implements! {
    impl ICounted for HashMapSeq {
        fn count(this: Value) -> Value {
            let body = unsafe { HashMapSeq::body(this) };
            Value::int((body.entries.len() as i64 - body.index) / 2)
        }
    }
}

clojure_rt_macros::implements! {
    impl IMeta for HashMapSeq {
        fn meta(this: Value) -> Value {
            let m = unsafe { HashMapSeq::body(this) }.meta;
            crate::rc::dup(m);
            m
        }
    }
}

clojure_rt_macros::implements! {
    impl IWithMeta for HashMapSeq {
        fn with_meta(this: Value, meta: Value) -> Value {
            let body = unsafe { HashMapSeq::body(this) };
            crate::rc::dup(body.map);
            crate::rc::dup(meta);
            let mut new_entries: Vec<Value> = Vec::with_capacity(body.entries.len());
            for &v in body.entries.iter() {
                crate::rc::dup(v);
                new_entries.push(v);
            }
            HashMapSeq::alloc(
                body.map,
                new_entries.into_boxed_slice(),
                body.index,
                meta,
                AtomicI32::new(0),
            )
        }
    }
}

clojure_rt_macros::implements! {
    impl IHash for HashMapSeq {
        fn hash(this: Value) -> Value {
            let body = unsafe { HashMapSeq::body(this) };
            let cached = body.hash.load(Ordering::Relaxed);
            if cached != 0 {
                return Value::int(cached as i64);
            }
            // Hash as an ordered seq of MapEntry-shaped pairs. Same
            // shape as ArrayMapSeq's hash impl.
            let mut acc: i32 = 1;
            let mut n: i32 = 0;
            let mut i = body.index as usize;
            while i < body.entries.len() {
                let kh = crate::rt::hash(body.entries[i]).as_int().unwrap_or(0) as i32;
                let vh = crate::rt::hash(body.entries[i + 1]).as_int().unwrap_or(0) as i32;
                let mut entry_acc: i32 = 1;
                entry_acc = entry_acc.wrapping_mul(31).wrapping_add(kh);
                entry_acc = entry_acc.wrapping_mul(31).wrapping_add(vh);
                let entry_hash = murmur3::mix_coll_hash(entry_acc, 2);
                acc = acc.wrapping_mul(31).wrapping_add(entry_hash);
                n = n.wrapping_add(1);
                i += 2;
            }
            let h = murmur3::mix_coll_hash(acc, n);
            body.hash.store(h, Ordering::Relaxed);
            Value::int(h as i64)
        }
    }
}

clojure_rt_macros::implements! {
    impl IEquiv for HashMapSeq {
        fn equiv(this: Value, other: Value) -> Value {
            // Same-type only for now; cross-type sequential equiv
            // (vs PersistentList / VecSeq / ArrayMapSeq) is the same
            // deferred work as elsewhere.
            if other.tag != this.tag {
                return Value::FALSE;
            }
            let ab = unsafe { HashMapSeq::body(this) };
            let bb = unsafe { HashMapSeq::body(other) };
            let a_remaining = (ab.entries.len() as i64 - ab.index) / 2;
            let b_remaining = (bb.entries.len() as i64 - bb.index) / 2;
            if a_remaining != b_remaining {
                return Value::FALSE;
            }
            // Note: HashMap seq order is deterministic-per-map but
            // not stable across maps with the same entries (different
            // insertion histories may yield different walk orders).
            // True equiv would compare *as sets* of entries, but the
            // protocol is sequential. Two seqs from the same map at
            // the same offset should match; that's what we assert.
            let mut sa = ab.index as usize;
            let mut sb = bb.index as usize;
            while sa < ab.entries.len() {
                if !crate::rt::equiv(ab.entries[sa], bb.entries[sb]).as_bool().unwrap_or(false) {
                    return Value::FALSE;
                }
                if !crate::rt::equiv(ab.entries[sa + 1], bb.entries[sb + 1]).as_bool().unwrap_or(false) {
                    return Value::FALSE;
                }
                sa += 2;
                sb += 2;
            }
            Value::TRUE
        }
    }
}

clojure_rt_macros::implements! { impl ISequential for HashMapSeq {} }
