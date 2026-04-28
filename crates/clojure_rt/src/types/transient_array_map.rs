//! `TransientArrayMap` — single-thread mutable view of a
//! `PersistentArrayMap`. Mirrors `clojure.lang.PersistentArrayMap$
//! TransientArrayMap` (JVM) and cljs's `TransientArrayMap`.
//!
//! Storage is a flat `Vec<Value>` of interleaved `[k0, v0, k1, v1, …]`
//! — the same shape as `PersistentArrayMap`'s body, but mutable so
//! `assoc!` / `dissoc!` / `conj!` operate in place without path-copy.
//!
//! Once `persistent!` is called, `alive` flips to `false` and the
//! `Vec<Value>` is detached and handed to `PersistentArrayMap::
//! from_owned_kvs` (no re-dup'ing of stored elements). Subsequent
//! mutation calls throw "Transient used after `persistent!`".
//!
//! Single-thread contract: documented but not enforced at runtime
//! beyond what the rc::* owner-tid debug assertions catch
//! incidentally. Cross-thread use is undefined behavior; future
//! tightening can re-purpose the Header's `owner_tid` for an
//! explicit check.

use core::sync::atomic::AtomicBool;
use core::sync::atomic::Ordering;

use crate::protocols::associative::IAssociative;
use crate::protocols::counted::ICounted;
use crate::protocols::lookup::ILookup;
use crate::protocols::transient_associative::ITransientAssociative;
use crate::protocols::transient_collection::ITransientCollection;
use crate::protocols::transient_map::ITransientMap;
use crate::types::array_map::PersistentArrayMap;
use crate::types::map_entry::MapEntry;
use crate::value::Value;

clojure_rt_macros::register_type! {
    pub struct TransientArrayMap {
        kvs:   Vec<Value>,
        alive: AtomicBool,
    }
}

impl TransientArrayMap {
    /// Build a fresh transient from a persistent. Each element is
    /// dup'd into the transient's owned Vec. Caller's ref to the
    /// persistent is unchanged.
    pub fn from_persistent(p: Value) -> Value {
        let p_kvs = PersistentArrayMap::kvs_borrowed(p);
        let mut kvs: Vec<Value> = Vec::with_capacity(p_kvs.len());
        for &x in p_kvs.iter() {
            crate::rc::dup(x);
            kvs.push(x);
        }
        TransientArrayMap::alloc(kvs, AtomicBool::new(true))
    }

    fn ensure_alive(this: Value) -> Result<(), Value> {
        let body = unsafe { TransientArrayMap::body(this) };
        if !body.alive.load(Ordering::Relaxed) {
            return Err(crate::exception::make_foreign(
                "Transient used after persistent!".to_string(),
            ));
        }
        Ok(())
    }

    /// Linear scan. Returns the slot offset of the matching key, or
    /// `None`. Same semantics as `PersistentArrayMap::index_of`.
    fn index_of(this: Value, k: Value) -> Option<usize> {
        let body = unsafe { TransientArrayMap::body(this) };
        let mut i = 0;
        while i < body.kvs.len() {
            let stored_k = body.kvs[i];
            let eq = crate::rt::equiv(stored_k, k).as_bool().unwrap_or(false);
            if eq {
                return Some(i);
            }
            i += 2;
        }
        None
    }
}

clojure_rt_macros::implements! {
    impl ITransientCollection for TransientArrayMap {
        fn conj_bang(this: Value, x: Value) -> Value {
            // `(conj! t e)` requires `e` to be MapEntry-shaped.
            if let Err(e) = TransientArrayMap::ensure_alive(this) { return e; }
            let me_id = crate::types::map_entry::MAPENTRY_TYPE_ID
                .get().copied().unwrap_or(0);
            if x.tag == me_id {
                let k = MapEntry::key_borrowed(x);
                let v = MapEntry::val_borrowed(x);
                crate::rt::assoc_bang(this, k, v)
            } else {
                crate::exception::make_foreign(format!(
                    "conj! on transient map requires a MapEntry, got tag {}",
                    x.tag
                ))
            }
        }
        fn persistent_bang(this: Value) -> Value {
            if let Err(e) = TransientArrayMap::ensure_alive(this) { return e; }
            let body = unsafe { TransientArrayMap::body_mut(this) };
            body.alive.store(false, Ordering::Relaxed);
            // Drain the Vec into a Box — moves ownership of every
            // element ref to the new persistent map; no per-element
            // re-dup.
            let kvs = std::mem::take(&mut body.kvs).into_boxed_slice();
            PersistentArrayMap::from_owned_kvs(kvs)
        }
    }
}

clojure_rt_macros::implements! {
    impl ITransientAssociative for TransientArrayMap {
        fn assoc_bang(this: Value, k: Value, v: Value) -> Value {
            if let Err(e) = TransientArrayMap::ensure_alive(this) { return e; }
            match TransientArrayMap::index_of(this, k) {
                Some(idx) => {
                    let body = unsafe { TransientArrayMap::body_mut(this) };
                    let old_v = body.kvs[idx + 1];
                    crate::rc::dup(v);
                    body.kvs[idx + 1] = v;
                    crate::rc::drop_value(old_v);
                }
                None => {
                    let body = unsafe { TransientArrayMap::body_mut(this) };
                    crate::rc::dup(k);
                    crate::rc::dup(v);
                    body.kvs.push(k);
                    body.kvs.push(v);
                }
            }
            crate::rc::dup(this);
            this
        }
    }
}

clojure_rt_macros::implements! {
    impl ITransientMap for TransientArrayMap {
        fn dissoc_bang(this: Value, k: Value) -> Value {
            if let Err(e) = TransientArrayMap::ensure_alive(this) { return e; }
            if let Some(idx) = TransientArrayMap::index_of(this, k) {
                let body = unsafe { TransientArrayMap::body_mut(this) };
                let old_k = body.kvs[idx];
                let old_v = body.kvs[idx + 1];
                // Order-preserving remove: shift the suffix down by 2.
                body.kvs.drain(idx..idx + 2);
                crate::rc::drop_value(old_k);
                crate::rc::drop_value(old_v);
            }
            crate::rc::dup(this);
            this
        }
    }
}

// Read-only protocols on the transient: count + lookup work the same
// as on the persistent shape.

clojure_rt_macros::implements! {
    impl ICounted for TransientArrayMap {
        fn count(this: Value) -> Value {
            let body = unsafe { TransientArrayMap::body(this) };
            Value::int((body.kvs.len() / 2) as i64)
        }
    }
}

clojure_rt_macros::implements! {
    impl ILookup for TransientArrayMap {
        fn lookup_2(this: Value, k: Value) -> Value {
            match TransientArrayMap::index_of(this, k) {
                Some(idx) => {
                    let body = unsafe { TransientArrayMap::body(this) };
                    let v = body.kvs[idx + 1];
                    crate::rc::dup(v);
                    v
                }
                None => Value::NIL,
            }
        }
        fn lookup_3(this: Value, k: Value, not_found: Value) -> Value {
            match TransientArrayMap::index_of(this, k) {
                Some(idx) => {
                    let body = unsafe { TransientArrayMap::body(this) };
                    let v = body.kvs[idx + 1];
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
    impl IAssociative for TransientArrayMap {
        fn assoc(this: Value, k: Value, v: Value) -> Value {
            // Mirrors `assoc!` — same in-place behavior; persistent
            // `IAssociative::assoc` semantics on a transient collapse
            // to assoc!.
            crate::rt::assoc_bang(this, k, v)
        }
        fn contains_key(this: Value, k: Value) -> Value {
            if TransientArrayMap::index_of(this, k).is_some() {
                Value::TRUE
            } else {
                Value::FALSE
            }
        }
        fn find(this: Value, k: Value) -> Value {
            match TransientArrayMap::index_of(this, k) {
                Some(idx) => {
                    let body = unsafe { TransientArrayMap::body(this) };
                    MapEntry::new(body.kvs[idx], body.kvs[idx + 1])
                }
                None => Value::NIL,
            }
        }
    }
}
