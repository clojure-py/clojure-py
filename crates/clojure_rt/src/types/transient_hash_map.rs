//! `TransientHashMap` — single-thread mutable view of a
//! `PersistentHashMap`. Mirrors the JVM Clojure transient hash-map.
//!
//! Storage matches the persistent shape (count + Arc<HAMTNode>), but
//! mutations use `Arc::make_mut` to edit the trie in place where
//! the node is uniquely owned. First touch on a shared subtree
//! path-copies that node only; subsequent mutations along the same
//! path are zero-allocation. Variant changes (entry-split,
//! collision-wrap) construct a fresh node and swap the Arc.
//!
//! `persistent!` flips `alive` to false and converts the body back
//! to a `PersistentHashMap`. Subsequent mutation calls throw.

use core::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::Arc;

use crate::protocols::associative::IAssociative;
use crate::protocols::counted::ICounted;
use crate::protocols::lookup::ILookup;
use crate::protocols::transient_associative::ITransientAssociative;
use crate::protocols::transient_collection::ITransientCollection;
use crate::protocols::transient_map::ITransientMap;
use crate::types::hash_map::{
    self, HAMTNode, PersistentHashMap,
};
use crate::types::map_entry::MapEntry;
use crate::value::Value;

clojure_rt_macros::register_type! {
    pub struct TransientHashMap {
        count: i64,
        root:  Arc<HAMTNode>,
        alive: AtomicBool,
    }
}

impl TransientHashMap {
    /// Build a fresh transient from a persistent. Trie root Arc is
    /// shared (cheap clone); first mutation along a path will
    /// `Arc::make_mut` that path's nodes. Caller's ref to the
    /// persistent is unchanged.
    pub fn from_persistent(p: Value) -> Value {
        let count = PersistentHashMap::count_of(p);
        let root = PersistentHashMap::root_of(p).clone();
        TransientHashMap::alloc(count, root, AtomicBool::new(true))
    }

    fn ensure_alive(this: Value) -> Result<(), Value> {
        let body = unsafe { TransientHashMap::body(this) };
        if !body.alive.load(Ordering::Relaxed) {
            return Err(crate::exception::make_foreign(
                "Transient used after persistent!".to_string(),
            ));
        }
        Ok(())
    }
}

clojure_rt_macros::implements! {
    impl ITransientCollection for TransientHashMap {
        fn conj_bang(this: Value, x: Value) -> Value {
            // (conj! t e) for a map: e must be MapEntry-shaped.
            if let Err(e) = TransientHashMap::ensure_alive(this) { return e; }
            let me_id = crate::types::map_entry::MAPENTRY_TYPE_ID
                .get().copied().unwrap_or(0);
            if x.tag == me_id {
                let k = MapEntry::key_borrowed(x);
                let v = MapEntry::val_borrowed(x);
                crate::rt::assoc_bang(this, k, v)
            } else {
                crate::exception::make_foreign(format!(
                    "conj! on transient hash-map requires a MapEntry, got tag {}",
                    x.tag
                ))
            }
        }
        fn persistent_bang(this: Value) -> Value {
            if let Err(e) = TransientHashMap::ensure_alive(this) { return e; }
            let body = unsafe { TransientHashMap::body_mut(this) };
            body.alive.store(false, Ordering::Relaxed);
            let count = body.count;
            let root = body.root.clone();
            crate::types::hash_map::PersistentHashMap::from_owned_parts(count, root)
        }
    }
}

clojure_rt_macros::implements! {
    impl ITransientAssociative for TransientHashMap {
        fn assoc_bang(this: Value, k: Value, v: Value) -> Value {
            if let Err(e) = TransientHashMap::ensure_alive(this) { return e; }
            let body = unsafe { TransientHashMap::body_mut(this) };
            let hash = crate::rt::hash(k).as_int().unwrap_or(0) as i32;
            let added = hash_map::assoc_in_place(&mut body.root, 0, hash, k, v);
            if added {
                body.count += 1;
            }
            crate::rc::dup(this);
            this
        }
    }
}

clojure_rt_macros::implements! {
    impl ITransientMap for TransientHashMap {
        fn dissoc_bang(this: Value, k: Value) -> Value {
            if let Err(e) = TransientHashMap::ensure_alive(this) { return e; }
            let body = unsafe { TransientHashMap::body_mut(this) };
            let hash = crate::rt::hash(k).as_int().unwrap_or(0) as i32;
            let removed = hash_map::dissoc_in_place(&mut body.root, 0, hash, k);
            if removed {
                body.count -= 1;
            }
            crate::rc::dup(this);
            this
        }
    }
}

// Read-side: count + lookup work mid-session.

clojure_rt_macros::implements! {
    impl ICounted for TransientHashMap {
        fn count(this: Value) -> Value {
            let body = unsafe { TransientHashMap::body(this) };
            Value::int(body.count)
        }
    }
}

clojure_rt_macros::implements! {
    impl ILookup for TransientHashMap {
        fn lookup_2(this: Value, k: Value) -> Value {
            let body = unsafe { TransientHashMap::body(this) };
            let hash = crate::rt::hash(k).as_int().unwrap_or(0) as i32;
            match crate::types::hash_map::lookup_in_node(&body.root, 0, hash, k) {
                Some(v) => { crate::rc::dup(v); v }
                None => Value::NIL,
            }
        }
        fn lookup_3(this: Value, k: Value, not_found: Value) -> Value {
            let body = unsafe { TransientHashMap::body(this) };
            let hash = crate::rt::hash(k).as_int().unwrap_or(0) as i32;
            match crate::types::hash_map::lookup_in_node(&body.root, 0, hash, k) {
                Some(v) => { crate::rc::dup(v); v }
                None => { crate::rc::dup(not_found); not_found }
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl IAssociative for TransientHashMap {
        fn assoc(this: Value, k: Value, v: Value) -> Value {
            crate::rt::assoc_bang(this, k, v)
        }
        fn contains_key(this: Value, k: Value) -> Value {
            let body = unsafe { TransientHashMap::body(this) };
            let hash = crate::rt::hash(k).as_int().unwrap_or(0) as i32;
            if crate::types::hash_map::lookup_in_node(&body.root, 0, hash, k).is_some() {
                Value::TRUE
            } else {
                Value::FALSE
            }
        }
        fn find(this: Value, k: Value) -> Value {
            let body = unsafe { TransientHashMap::body(this) };
            let hash = crate::rt::hash(k).as_int().unwrap_or(0) as i32;
            match crate::types::hash_map::lookup_in_node(&body.root, 0, hash, k) {
                Some(v) => MapEntry::new(k, v),
                None => Value::NIL,
            }
        }
    }
}

#[allow(dead_code)]
fn _atomic_i32_holder() -> AtomicI32 { AtomicI32::new(0) }
