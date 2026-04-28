//! `ArrayMapSeq` — seq cursor over a `PersistentArrayMap`. `first`
//! returns a `MapEntry`, `rest` advances by one entry. Holds a
//! strong ref to the underlying map so the kv array stays valid for
//! the seq's lifetime.

use core::sync::atomic::{AtomicI32, Ordering};

use crate::hash::murmur3;
use crate::protocols::counted::ICounted;
use crate::protocols::equiv::IEquiv;
use crate::protocols::hash::IHash;
use crate::protocols::meta::{IMeta, IWithMeta};
use crate::protocols::seq::{INext, ISeq, ISeqable};
use crate::protocols::sequential::ISequential;
use crate::types::array_map::PersistentArrayMap;
use crate::types::map_entry::MapEntry;
use crate::value::Value;

clojure_rt_macros::register_type! {
    pub struct ArrayMapSeq {
        map:  Value,        // PersistentArrayMap
        slot: i64,          // 0, 2, 4, … (kv-pair start in `kvs`)
        meta: Value,
        hash: AtomicI32,    // 0 = uncomputed
    }
}

impl ArrayMapSeq {
    pub fn start(map: Value) -> Value {
        crate::rc::dup(map);
        ArrayMapSeq::alloc(map, 0, Value::NIL, AtomicI32::new(0))
    }
}

clojure_rt_macros::implements! {
    impl ISeq for ArrayMapSeq {
        fn first(this: Value) -> Value {
            let body = unsafe { ArrayMapSeq::body(this) };
            let (k, v) = PersistentArrayMap::kv_at(body.map, body.slot as usize);
            // Borrow semantics: MapEntry::new dups internally.
            MapEntry::new(k, v)
        }
        fn rest(this: Value) -> Value {
            let body = unsafe { ArrayMapSeq::body(this) };
            let kvs_len = (PersistentArrayMap::count_of(body.map) * 2) as i64;
            let next_slot = body.slot + 2;
            if next_slot >= kvs_len {
                crate::types::list::empty_list()
            } else {
                crate::rc::dup(body.map);
                ArrayMapSeq::alloc(body.map, next_slot, Value::NIL, AtomicI32::new(0))
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl INext for ArrayMapSeq {
        fn next(this: Value) -> Value {
            let body = unsafe { ArrayMapSeq::body(this) };
            let kvs_len = (PersistentArrayMap::count_of(body.map) * 2) as i64;
            let next_slot = body.slot + 2;
            if next_slot >= kvs_len {
                Value::NIL
            } else {
                crate::rc::dup(body.map);
                ArrayMapSeq::alloc(body.map, next_slot, Value::NIL, AtomicI32::new(0))
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl ISeqable for ArrayMapSeq {
        fn seq(this: Value) -> Value {
            crate::rc::dup(this);
            this
        }
    }
}

clojure_rt_macros::implements! {
    impl ICounted for ArrayMapSeq {
        fn count(this: Value) -> Value {
            let body = unsafe { ArrayMapSeq::body(this) };
            let total = PersistentArrayMap::count_of(body.map);
            Value::int(total - body.slot / 2)
        }
    }
}

clojure_rt_macros::implements! {
    impl IMeta for ArrayMapSeq {
        fn meta(this: Value) -> Value {
            let m = unsafe { ArrayMapSeq::body(this) }.meta;
            crate::rc::dup(m);
            m
        }
    }
}

clojure_rt_macros::implements! {
    impl IWithMeta for ArrayMapSeq {
        fn with_meta(this: Value, meta: Value) -> Value {
            let body = unsafe { ArrayMapSeq::body(this) };
            crate::rc::dup(body.map);
            crate::rc::dup(meta);
            ArrayMapSeq::alloc(body.map, body.slot, meta, AtomicI32::new(0))
        }
    }
}

clojure_rt_macros::implements! {
    impl IHash for ArrayMapSeq {
        fn hash(this: Value) -> Value {
            let body = unsafe { ArrayMapSeq::body(this) };
            let cached = body.hash.load(Ordering::Relaxed);
            if cached != 0 {
                return Value::int(cached as i64);
            }
            // Hash as an ordered seq of MapEntry-shaped pairs. Each
            // entry hashes the same as `[k v]` (a 2-vector hash); the
            // outer is `hash_ordered` over the per-entry hashes.
            let mut acc: i32 = 1;
            let mut n: i32 = 0;
            let total_kvs = (PersistentArrayMap::count_of(body.map) * 2) as i64;
            let mut slot = body.slot;
            while slot < total_kvs {
                let (k, v) = PersistentArrayMap::kv_at(body.map, slot as usize);
                let kh = crate::rt::hash(k).as_int().unwrap_or(0) as i32;
                let vh = crate::rt::hash(v).as_int().unwrap_or(0) as i32;
                let mut entry_acc: i32 = 1;
                entry_acc = entry_acc.wrapping_mul(31).wrapping_add(kh);
                entry_acc = entry_acc.wrapping_mul(31).wrapping_add(vh);
                let entry_hash = murmur3::mix_coll_hash(entry_acc, 2);
                acc = acc.wrapping_mul(31).wrapping_add(entry_hash);
                n = n.wrapping_add(1);
                slot += 2;
            }
            let h = murmur3::mix_coll_hash(acc, n);
            body.hash.store(h, Ordering::Relaxed);
            Value::int(h as i64)
        }
    }
}

clojure_rt_macros::implements! {
    impl IEquiv for ArrayMapSeq {
        fn equiv(this: Value, other: Value) -> Value {
            // Cross-type sequential equiv (against PersistentList /
            // VecSeq) is deferred — see the matching note in
            // types/list.rs. Same-type equiv requires identical
            // (k,v) sequence in seq-traversal order.
            if other.tag != this.tag {
                return Value::FALSE;
            }
            let ab = unsafe { ArrayMapSeq::body(this) };
            let bb = unsafe { ArrayMapSeq::body(other) };
            let a_total = (PersistentArrayMap::count_of(ab.map) * 2) as i64;
            let b_total = (PersistentArrayMap::count_of(bb.map) * 2) as i64;
            let a_remaining = a_total - ab.slot;
            let b_remaining = b_total - bb.slot;
            if a_remaining != b_remaining {
                return Value::FALSE;
            }
            let mut sa = ab.slot;
            let mut sb = bb.slot;
            while sa < a_total {
                let (ak, av) = PersistentArrayMap::kv_at(ab.map, sa as usize);
                let (bk, bv) = PersistentArrayMap::kv_at(bb.map, sb as usize);
                let k_eq = crate::rt::equiv(ak, bk).as_bool().unwrap_or(false);
                if !k_eq { return Value::FALSE; }
                let v_eq = crate::rt::equiv(av, bv).as_bool().unwrap_or(false);
                if !v_eq { return Value::FALSE; }
                sa += 2;
                sb += 2;
            }
            Value::TRUE
        }
    }
}

clojure_rt_macros::implements! { impl ISequential for ArrayMapSeq {} }
