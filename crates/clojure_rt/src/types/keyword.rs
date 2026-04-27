//! Port of `clojure.lang.Keyword`. Heap-allocated, wraps a SymbolObj
//! that carries the keyword's name + namespace. Globally interned:
//! two `(keyword "ns" "name")` calls return the same `Value`. Strong
//! references in v1 (leak bounded by unique-keyword count); weak-ref
//! interning is a follow-up.

use core::sync::atomic::{AtomicI32, Ordering};
use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

use crate::protocols::equiv::IEquiv;
use crate::protocols::hash::IHash;
use crate::protocols::named::INamed;
use crate::types::symbol::SymbolObj;
use crate::value::Value;

clojure_rt_macros::register_type! {
    pub struct KeywordObj {
        sym:  Value,    // SymbolObj — borrowed identity
        hash: AtomicI32, // 0 = uncomputed
    }
}

/// `(ns, name)` lookup key for the intern table. Owned strings so the
/// table doesn't depend on caller-side string lifetimes.
type InternKey = (Option<String>, String);

static KEYWORD_TABLE: LazyLock<RwLock<HashMap<InternKey, Value>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

impl KeywordObj {
    /// Intern a keyword. Returns *the same* `Value` for repeated calls
    /// with equal `(ns, name)`. The intern table holds a strong ref;
    /// the keyword therefore lives for the life of the process.
    pub fn intern(ns: Option<&str>, name: &str) -> Value {
        let key: InternKey = (ns.map(String::from), name.to_string());

        // Fast path: already interned.
        if let Some(&v) = KEYWORD_TABLE.read().unwrap().get(&key) {
            crate::rc::dup(v);
            return v;
        }

        // Slow path: write-lock, re-check (race with another thread),
        // allocate.
        let mut table = KEYWORD_TABLE.write().unwrap();
        if let Some(&v) = table.get(&key) {
            crate::rc::dup(v);
            return v;
        }
        let sym = SymbolObj::intern(ns, name);
        let kw  = Self::alloc(sym, AtomicI32::new(0));
        // Hold an extra ref for the table itself.
        crate::rc::dup(kw);
        table.insert(key, kw);
        kw
    }
}

clojure_rt_macros::implements! {
    impl IHash for KeywordObj {
        fn hash(this: Value) -> Value {
            unsafe {
                let body = this.as_heap().unwrap().add(1) as *const KeywordObj;
                let cached = (*body).hash.load(Ordering::Relaxed);
                if cached != 0 {
                    return Value::int(cached as i64);
                }
                // hash = sym.hash() + 0x9e3779b9 (matches JVM Keyword).
                let sym_h = clojure_rt_macros::dispatch!(
                    IHash::hash, &[(*body).sym]
                ).as_int().unwrap() as i32;
                let h = sym_h.wrapping_add(0x9e3779b9_u32 as i32);
                (*body).hash.store(h, Ordering::Relaxed);
                Value::int(h as i64)
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl IEquiv for KeywordObj {
        fn equiv(this: Value, other: Value) -> Value {
            // Interning means same (ns, name) ⇒ same Value pointer.
            // Identity check is sufficient *and* fastest.
            if other.tag != this.tag {
                return Value::FALSE;
            }
            if this.payload == other.payload {
                Value::TRUE
            } else {
                Value::FALSE
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl INamed for KeywordObj {
        fn namespace(this: Value) -> Value {
            unsafe {
                let body = this.as_heap().unwrap().add(1) as *const KeywordObj;
                clojure_rt_macros::dispatch!(INamed::namespace, &[(*body).sym])
            }
        }
        fn name(this: Value) -> Value {
            unsafe {
                let body = this.as_heap().unwrap().add(1) as *const KeywordObj;
                clojure_rt_macros::dispatch!(INamed::name, &[(*body).sym])
            }
        }
    }
}
