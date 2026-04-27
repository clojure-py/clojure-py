//! Native UTF-8 string heap type. Counterpart to `java.lang.String`
//! in JVM Clojure, but stored as UTF-8 (Rust-native) rather than
//! UTF-16. Hashing iterates the UTF-16 code-unit form lazily so
//! `IHash` agrees with Symbol/Keyword's name-hashing without
//! doubling memory for ASCII content.
//!
//! `Counted/count` returns the **codepoint count** (Pythonic
//! semantics). For BMP-only content this matches JVM Clojure's
//! `(count "...")`; for supplementary-plane codepoints we return
//! the codepoint count where JVM returns the UTF-16 code-unit
//! count. Documented deviation.

use core::sync::atomic::{AtomicI32, Ordering};

use crate::hash::murmur3;
use crate::protocols::counted::ICounted;
use crate::protocols::equiv::IEquiv;
use crate::protocols::hash::IHash;
use crate::value::Value;

clojure_rt_macros::register_type! {
    pub struct StringObj {
        data: Box<str>,
        hash: AtomicI32,
    }
}

impl StringObj {
    /// Allocate a fresh `StringObj` carrying `s`'s bytes. Returns an
    /// opaque `Value` (the heap-tagged handle) rather than `Self` —
    /// `Self` is the body type, not the user-facing reference. The
    /// `new_ret_no_self` lint expects `Self`; we don't follow it
    /// because heap types are addressed via `Value` everywhere.
    #[inline]
    #[allow(clippy::new_ret_no_self)]
    pub fn new(s: &str) -> Value {
        Self::alloc(s.to_string().into_boxed_str(), AtomicI32::new(0))
    }

    /// Borrow the underlying `&str` from a `Value(StringObj)`.
    ///
    /// # Safety
    /// The caller must guarantee:
    /// - `v` is a live `Value` whose tag is the `StringObj` TypeId.
    /// - The returned reference is not used past the lifetime of any
    ///   copy of `v` reaching zero refcount.
    #[inline]
    pub unsafe fn as_str_unchecked<'a>(v: Value) -> &'a str {
        let h = v.as_heap().expect("StringObj::as_str_unchecked: not a heap Value");
        let body = unsafe { h.add(1) } as *const StringObj;
        unsafe { &(*body).data }
    }
}

clojure_rt_macros::implements! {
    impl ICounted for StringObj {
        fn count(this: Value) -> Value {
            unsafe {
                let s = StringObj::as_str_unchecked(this);
                Value::int(s.chars().count() as i64)
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl IHash for StringObj {
        fn hash(this: Value) -> Value {
            unsafe {
                let body = this.as_heap().unwrap().add(1) as *const StringObj;
                let cached = (*body).hash.load(Ordering::Relaxed);
                if cached != 0 {
                    return Value::int(cached as i64);
                }
                let h = murmur3::hash_unencoded_chars(&(*body).data);
                // 0 is the "uncomputed" sentinel. If the algorithm
                // produces 0 (only for the empty string under Murmur3
                // mixed via hash_unencoded_chars's fmix(0,0)), we still
                // store it; subsequent reads will recompute, which is
                // idempotent.
                (*body).hash.store(h, Ordering::Relaxed);
                Value::int(h as i64)
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl IEquiv for StringObj {
        fn equiv(this: Value, other: Value) -> Value {
            unsafe {
                let body_a = this.as_heap().unwrap().add(1) as *const StringObj;
                // Other must be a StringObj for byte-equality to mean
                // anything. Tag check first.
                if other.tag != this.tag {
                    return Value::FALSE;
                }
                let body_b = other.as_heap().unwrap().add(1) as *const StringObj;
                if (*body_a).data == (*body_b).data {
                    Value::TRUE
                } else {
                    Value::FALSE
                }
            }
        }
    }
}
