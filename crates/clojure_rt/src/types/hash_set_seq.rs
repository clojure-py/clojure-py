//! `HashSetSeq` — depth-first cursor over a `PersistentHashSet`'s
//! underlying HAMT. Yields the keys directly (not `MapEntry`s).
//!
//! Same shape as `HashMapSeq`: materialize all keys up front into an
//! owned `Box<[Value]>` at construction; first/rest/next index into
//! it. Each key is dup'd into the seq's storage; the macro
//! destructor decrefs them on drop.

use core::sync::atomic::{AtomicI32, Ordering};

use crate::hash::murmur3;
use crate::protocols::counted::ICounted;
use crate::protocols::equiv::IEquiv;
use crate::protocols::hash::IHash;
use crate::protocols::meta::{IMeta, IWithMeta};
use crate::protocols::seq::{INext, ISeq, ISeqable};
use crate::protocols::sequential::ISequential;
use crate::types::hash_map::{walk_entries, PersistentHashMap};
use crate::types::hash_set::PersistentHashSet;
use crate::value::Value;

clojure_rt_macros::register_type! {
    pub struct HashSetSeq {
        set:   Value,         // PHS, kept alive
        keys:  Box<[Value]>,
        index: i64,
        meta:  Value,
        hash:  AtomicI32,
    }
}

impl HashSetSeq {
    pub fn start(set: Value) -> Value {
        let count = PersistentHashSet::count_of(set);
        let mut keys: Vec<Value> = Vec::with_capacity(count as usize);
        let m = PersistentHashSet::map_of(set);
        let root = PersistentHashMap::root_of(m);
        walk_entries(root, &mut |k, _v| {
            crate::rc::dup(k);
            keys.push(k);
        });
        crate::rc::dup(set);
        HashSetSeq::alloc(
            set,
            keys.into_boxed_slice(),
            0,
            Value::NIL,
            AtomicI32::new(0),
        )
    }
}

clojure_rt_macros::implements! {
    impl ISeq for HashSetSeq {
        fn first(this: Value) -> Value {
            let body = unsafe { HashSetSeq::body(this) };
            let v = body.keys[body.index as usize];
            crate::rc::dup(v);
            v
        }
        fn rest(this: Value) -> Value {
            let body = unsafe { HashSetSeq::body(this) };
            let next_idx = body.index + 1;
            if (next_idx as usize) >= body.keys.len() {
                crate::types::list::empty_list()
            } else {
                clone_advanced(body, next_idx)
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl INext for HashSetSeq {
        fn next(this: Value) -> Value {
            let body = unsafe { HashSetSeq::body(this) };
            let next_idx = body.index + 1;
            if (next_idx as usize) >= body.keys.len() {
                Value::NIL
            } else {
                clone_advanced(body, next_idx)
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl ISeqable for HashSetSeq {
        fn seq(this: Value) -> Value {
            crate::rc::dup(this);
            this
        }
    }
}

clojure_rt_macros::implements! {
    impl ICounted for HashSetSeq {
        fn count(this: Value) -> Value {
            let body = unsafe { HashSetSeq::body(this) };
            Value::int(body.keys.len() as i64 - body.index)
        }
    }
}

clojure_rt_macros::implements! {
    impl IMeta for HashSetSeq {
        fn meta(this: Value) -> Value {
            let m = unsafe { HashSetSeq::body(this) }.meta;
            crate::rc::dup(m);
            m
        }
    }
}

clojure_rt_macros::implements! {
    impl IWithMeta for HashSetSeq {
        fn with_meta(this: Value, meta: Value) -> Value {
            let body = unsafe { HashSetSeq::body(this) };
            let mut new_keys: Vec<Value> = Vec::with_capacity(body.keys.len());
            for &v in body.keys.iter() {
                crate::rc::dup(v);
                new_keys.push(v);
            }
            crate::rc::dup(body.set);
            crate::rc::dup(meta);
            HashSetSeq::alloc(
                body.set,
                new_keys.into_boxed_slice(),
                body.index,
                meta,
                AtomicI32::new(0),
            )
        }
    }
}

clojure_rt_macros::implements! {
    impl IHash for HashSetSeq {
        fn hash(this: Value) -> Value {
            let body = unsafe { HashSetSeq::body(this) };
            let cached = body.hash.load(Ordering::Relaxed);
            if cached != 0 {
                return Value::int(cached as i64);
            }
            // Hash as an ordered seq.
            let mut acc: i32 = 1;
            let mut n: i32 = 0;
            let mut i = body.index as usize;
            while i < body.keys.len() {
                let h = crate::rt::hash(body.keys[i]).as_int().unwrap_or(0) as i32;
                acc = acc.wrapping_mul(31).wrapping_add(h);
                n = n.wrapping_add(1);
                i += 1;
            }
            let h = murmur3::mix_coll_hash(acc, n);
            body.hash.store(h, Ordering::Relaxed);
            Value::int(h as i64)
        }
    }
}

clojure_rt_macros::implements! {
    impl IEquiv for HashSetSeq {
        fn equiv(this: Value, other: Value) -> Value {
            if other.tag != this.tag {
                return Value::FALSE;
            }
            let ab = unsafe { HashSetSeq::body(this) };
            let bb = unsafe { HashSetSeq::body(other) };
            let a_remaining = ab.keys.len() as i64 - ab.index;
            let b_remaining = bb.keys.len() as i64 - bb.index;
            if a_remaining != b_remaining { return Value::FALSE; }
            let mut sa = ab.index as usize;
            let mut sb = bb.index as usize;
            while sa < ab.keys.len() {
                if !crate::rt::equiv(ab.keys[sa], bb.keys[sb])
                    .as_bool().unwrap_or(false) {
                    return Value::FALSE;
                }
                sa += 1; sb += 1;
            }
            Value::TRUE
        }
    }
}

clojure_rt_macros::implements! { impl ISequential for HashSetSeq {} }

fn clone_advanced(body: &HashSetSeq, next_idx: i64) -> Value {
    let mut new_keys: Vec<Value> = Vec::with_capacity(body.keys.len());
    for &v in body.keys.iter() {
        crate::rc::dup(v);
        new_keys.push(v);
    }
    crate::rc::dup(body.set);
    HashSetSeq::alloc(
        body.set,
        new_keys.into_boxed_slice(),
        next_idx,
        Value::NIL,
        AtomicI32::new(0),
    )
}
