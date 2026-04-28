//! `MapEntry` — a key/value pair vended by `find` and the seq view of
//! a map. Behaves as a 2-vector for `nth`/`count`/`hash`/`equiv`, so
//! destructuring `[k v]` and round-trips through `vector?`-style
//! predicates work transparently. Also exposes the `IMapEntry`
//! protocol (`key` / `val`) for the direct-access path.
//!
//! Mirrors `clojure.lang.MapEntry` (JVM, extends APersistentVector +
//! Map.Entry) and cljs's `MapEntry` (deftype with IMapEntry +
//! IIndexed + IMeta + …).

use core::sync::atomic::{AtomicI32, Ordering};

use crate::hash::murmur3;
use crate::protocols::counted::ICounted;
use crate::protocols::equiv::IEquiv;
use crate::protocols::hash::IHash;
use crate::protocols::indexed::IIndexed;
use crate::protocols::map_entry::IMapEntry;
use crate::protocols::meta::{IMeta, IWithMeta};
use crate::protocols::sequential::ISequential;
use crate::value::Value;

clojure_rt_macros::register_type! {
    pub struct MapEntry {
        key:  Value,
        val:  Value,
        meta: Value,
        hash: AtomicI32,    // 0 = uncomputed
    }
}

impl MapEntry {
    /// Build a `MapEntry` from a key/value pair. **Borrow semantics**:
    /// the caller's refs to `k` and `v` are unchanged; the entry
    /// dups both for its own storage. Caller is still responsible
    /// for `drop_value`-ing `k` and `v` when it's done with them.
    pub fn new(k: Value, v: Value) -> Value {
        crate::rc::dup(k);
        crate::rc::dup(v);
        MapEntry::alloc(k, v, Value::NIL, AtomicI32::new(0))
    }

    /// Borrowed read of the key. Caller must NOT `drop_value` the
    /// returned value — its ref is owned by the entry.
    #[inline]
    pub(crate) fn key_borrowed(this: Value) -> Value {
        unsafe { MapEntry::body(this) }.key
    }

    /// Borrowed read of the value. Same caller discipline as `key_borrowed`.
    #[inline]
    pub(crate) fn val_borrowed(this: Value) -> Value {
        unsafe { MapEntry::body(this) }.val
    }
}

clojure_rt_macros::implements! {
    impl IMapEntry for MapEntry {
        fn key(this: Value) -> Value {
            let v = unsafe { MapEntry::body(this) }.key;
            crate::rc::dup(v);
            v
        }
        fn val(this: Value) -> Value {
            let v = unsafe { MapEntry::body(this) }.val;
            crate::rc::dup(v);
            v
        }
    }
}

clojure_rt_macros::implements! {
    impl ICounted for MapEntry {
        fn count(this: Value) -> Value {
            let _ = this;
            Value::int(2)
        }
    }
}

clojure_rt_macros::implements! {
    impl IIndexed for MapEntry {
        fn nth_2(this: Value, n: Value) -> Value {
            let body = unsafe { MapEntry::body(this) };
            match n.as_int() {
                Some(0) => { crate::rc::dup(body.key); body.key }
                Some(1) => { crate::rc::dup(body.val); body.val }
                _ => crate::exception::make_foreign(
                    format!("Index out of bounds for MapEntry: {}",
                            n.as_int().map(|i| i.to_string()).unwrap_or_else(|| "?".into()))
                ),
            }
        }
        fn nth_3(this: Value, n: Value, not_found: Value) -> Value {
            let body = unsafe { MapEntry::body(this) };
            match n.as_int() {
                Some(0) => { crate::rc::dup(body.key); body.key }
                Some(1) => { crate::rc::dup(body.val); body.val }
                _ => { crate::rc::dup(not_found); not_found }
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl IHash for MapEntry {
        fn hash(this: Value) -> Value {
            let body = unsafe { MapEntry::body(this) };
            let cached = body.hash.load(Ordering::Relaxed);
            if cached != 0 {
                return Value::int(cached as i64);
            }
            // Same shape as `(hash [k v])` — `hash_ordered` of two
            // element hashes mixed via mix_coll_hash with count=2.
            let kh = crate::rt::hash(body.key).as_int().unwrap_or(0) as i32;
            let vh = crate::rt::hash(body.val).as_int().unwrap_or(0) as i32;
            let mut acc: i32 = 1;
            acc = acc.wrapping_mul(31).wrapping_add(kh);
            acc = acc.wrapping_mul(31).wrapping_add(vh);
            let h = murmur3::mix_coll_hash(acc, 2);
            body.hash.store(h, Ordering::Relaxed);
            Value::int(h as i64)
        }
    }
}

clojure_rt_macros::implements! {
    impl IEquiv for MapEntry {
        fn equiv(this: Value, other: Value) -> Value {
            // MapEntry equiv with another MapEntry (same key+val) and
            // with a 2-element PersistentVector. Cross-type sequential
            // equiv beyond that is deferred (matches the Vector-vs-
            // List note in types/list.rs).
            let body = unsafe { MapEntry::body(this) };
            if other.tag == this.tag {
                let ob = unsafe { MapEntry::body(other) };
                let k_eq = crate::rt::equiv(body.key, ob.key).as_bool().unwrap_or(false);
                if !k_eq { return Value::FALSE; }
                let v_eq = crate::rt::equiv(body.val, ob.val).as_bool().unwrap_or(false);
                return if v_eq { Value::TRUE } else { Value::FALSE };
            }
            // Compare to a 2-element vector via the IIndexed protocol —
            // works for any IIndexed of count 2 with matching nth.
            if !crate::protocol::satisfies(&ICounted::COUNT_1, other) {
                return Value::FALSE;
            }
            let cnt = crate::rt::count(other).as_int().unwrap_or(-1);
            if cnt != 2 { return Value::FALSE; }
            if !crate::protocol::satisfies(&IIndexed::NTH_2, other) {
                return Value::FALSE;
            }
            let ok = crate::rt::nth(other, Value::int(0));
            let ov = crate::rt::nth(other, Value::int(1));
            let k_eq = crate::rt::equiv(body.key, ok).as_bool().unwrap_or(false);
            let v_eq = crate::rt::equiv(body.val, ov).as_bool().unwrap_or(false);
            crate::rc::drop_value(ok);
            crate::rc::drop_value(ov);
            if k_eq && v_eq { Value::TRUE } else { Value::FALSE }
        }
    }
}

clojure_rt_macros::implements! {
    impl IMeta for MapEntry {
        fn meta(this: Value) -> Value {
            let m = unsafe { MapEntry::body(this) }.meta;
            crate::rc::dup(m);
            m
        }
    }
}

clojure_rt_macros::implements! {
    impl IWithMeta for MapEntry {
        fn with_meta(this: Value, meta: Value) -> Value {
            let body = unsafe { MapEntry::body(this) };
            crate::rc::dup(body.key);
            crate::rc::dup(body.val);
            crate::rc::dup(meta);
            MapEntry::alloc(body.key, body.val, meta, AtomicI32::new(0))
        }
    }
}

clojure_rt_macros::implements! { impl ISequential for MapEntry {} }
