//! Port of `clojure.lang.Symbol`. Heap-allocated, carries an optional
//! namespace, a required name, and a metadata `Value`. Hash combines
//! name and namespace via Murmur3 + Boost-style mix; meta does not
//! contribute (matches JVM).

use core::sync::atomic::{AtomicI32, Ordering};

use crate::hash::{murmur3, util};
use crate::protocols::equiv::IEquiv;
use crate::protocols::hash_eq::IHashEq;
use crate::protocols::meta::{IMeta, IObj};
use crate::protocols::named::Named;
use crate::types::string::StringObj;
use crate::value::Value;

clojure_rt_macros::register_type! {
    pub struct SymbolObj {
        ns:   Value,    // Value::NIL or StringObj
        name: Value,    // StringObj
        meta: Value,    // Value::NIL or any user-supplied Value
        hash: AtomicI32, // 0 = uncomputed
    }
}

impl SymbolObj {
    /// Construct a fresh `SymbolObj`. JVM `Symbol.intern` is a misnomer
    /// — it doesn't actually intern; it just allocates. Same here.
    /// Name and (optional) namespace become fresh `StringObj`s.
    pub fn intern(ns: Option<&str>, name: &str) -> Value {
        let ns_val = match ns {
            Some(s) => StringObj::new(s),
            None    => Value::NIL,
        };
        let name_val = StringObj::new(name);
        Self::alloc(ns_val, name_val, Value::NIL, AtomicI32::new(0))
    }

    /// Allocate a new SymbolObj sharing `ns` and `name` with `this`
    /// (their refcounts get bumped) but with a fresh `meta`. Used by
    /// the IObj impl.
    ///
    /// # Safety
    /// `this` must be a live `Value` of `SymbolObj`.
    unsafe fn replace_meta(this: Value, new_meta: Value) -> Value {
        let body = unsafe { this.as_heap().unwrap().add(1) } as *const SymbolObj;
        let ns   = unsafe { (*body).ns };
        let name = unsafe { (*body).name };
        // Bump refcounts on the shared fields — the new SymbolObj
        // owns its own references.
        crate::rc::dup(ns);
        crate::rc::dup(name);
        Self::alloc(ns, name, new_meta, AtomicI32::new(0))
    }
}

clojure_rt_macros::implements! {
    impl IHashEq for SymbolObj {
        fn hasheq(this: Value) -> Value {
            unsafe {
                let body = this.as_heap().unwrap().add(1) as *const SymbolObj;
                let cached = (*body).hash.load(Ordering::Relaxed);
                if cached != 0 {
                    return Value::int(cached as i64);
                }
                let name_str = StringObj::as_str_unchecked((*body).name);
                let ns_v = (*body).ns;
                let name_h = murmur3::hash_unencoded_chars(name_str);
                let ns_h = if ns_v.is_nil() {
                    0
                } else {
                    let s = StringObj::as_str_unchecked(ns_v);
                    murmur3::hash_unencoded_chars(s)
                };
                let h = util::hash_combine(name_h, ns_h);
                (*body).hash.store(h, Ordering::Relaxed);
                Value::int(h as i64)
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl IEquiv for SymbolObj {
        fn equiv(this: Value, other: Value) -> Value {
            // Tag must match: both must be SymbolObj.
            if other.tag != this.tag {
                return Value::FALSE;
            }
            unsafe {
                let a = this.as_heap().unwrap().add(1) as *const SymbolObj;
                let b = other.as_heap().unwrap().add(1) as *const SymbolObj;

                // Name comparison: both are StringObj (or this is
                // malformed). Compare bytes directly.
                let a_name = StringObj::as_str_unchecked((*a).name);
                let b_name = StringObj::as_str_unchecked((*b).name);
                if a_name != b_name {
                    return Value::FALSE;
                }

                // Namespace comparison: both nil, both string-and-equal,
                // or differ → not equal.
                let a_ns = (*a).ns;
                let b_ns = (*b).ns;
                if a_ns.is_nil() && b_ns.is_nil() {
                    return Value::TRUE;
                }
                if a_ns.is_nil() || b_ns.is_nil() {
                    return Value::FALSE;
                }
                let a_ns_s = StringObj::as_str_unchecked(a_ns);
                let b_ns_s = StringObj::as_str_unchecked(b_ns);
                if a_ns_s == b_ns_s {
                    Value::TRUE
                } else {
                    Value::FALSE
                }
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl Named for SymbolObj {
        fn get_namespace(this: Value) -> Value {
            unsafe {
                let body = this.as_heap().unwrap().add(1) as *const SymbolObj;
                let v = (*body).ns;
                crate::rc::dup(v);
                v
            }
        }
        fn get_name(this: Value) -> Value {
            unsafe {
                let body = this.as_heap().unwrap().add(1) as *const SymbolObj;
                let v = (*body).name;
                crate::rc::dup(v);
                v
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl IMeta for SymbolObj {
        fn meta(this: Value) -> Value {
            unsafe {
                let body = this.as_heap().unwrap().add(1) as *const SymbolObj;
                let v = (*body).meta;
                crate::rc::dup(v);
                v
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl IObj for SymbolObj {
        fn with_meta(this: Value, meta: Value) -> Value {
            // The new SymbolObj owns its meta reference.
            crate::rc::dup(meta);
            unsafe { SymbolObj::replace_meta(this, meta) }
        }
    }
}
