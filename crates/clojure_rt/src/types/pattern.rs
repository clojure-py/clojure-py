//! `PatternObj` — compiled regex. Mirrors JVM
//! `java.util.regex.Pattern` (the `#"..."` reader literal).
//!
//! Storage is a `regex::Regex` — Rust's standard regex engine. The
//! original source string is kept alongside the compiled regex for
//! `(.pattern p)` and printable round-trip. Equality is on the
//! source string (matches JVM `Pattern.equals`, which is reference-
//! equal but the user-level `=` we ship reduces to "same source"
//! since two regexes compiled from the same string are
//! interchangeable).
//!
//! Regex syntax differences vs JVM: Rust's `regex` crate uses RE2-
//! style syntax — most ASCII patterns work identically, but some
//! Java-only features (lookbehind, backreferences) aren't
//! supported. The reader will produce `PatternObj` from `#"..."`
//! verbatim; users who need Java-only constructs would need a
//! different engine, which is a future concern.

use core::sync::atomic::{AtomicI32, Ordering};

use regex::Regex;

use crate::hash::murmur3;
use crate::protocols::equiv::IEquiv;
use crate::protocols::hash::IHash;
use crate::value::Value;

clojure_rt_macros::register_type! {
    pub struct PatternObj {
        re:     Regex,
        source: Box<str>,
        hash:   AtomicI32,
    }
}

impl PatternObj {
    /// Compile a regex from its source string. Exception value on
    /// compile failure — the reader's `#"..."` path uses this.
    pub fn from_str(source: &str) -> Value {
        match Regex::new(source) {
            Ok(re) => PatternObj::alloc(
                re,
                source.to_string().into_boxed_str(),
                AtomicI32::new(0),
            ),
            Err(e) => crate::exception::make_foreign(format!(
                "Pattern compile failure for {source:?}: {e}"
            )),
        }
    }

    /// Borrow the compiled `Regex`.
    ///
    /// # Safety
    /// `v` must be a live `PatternObj`-tagged `Value`.
    pub unsafe fn as_regex(v: Value) -> &'static Regex {
        let body = unsafe { PatternObj::body(v) };
        &body.re
    }

    /// Borrow the original source string (`(.pattern p)`).
    ///
    /// # Safety
    /// `v` must be a live `PatternObj`-tagged `Value`.
    pub unsafe fn source(v: Value) -> &'static str {
        let body = unsafe { PatternObj::body(v) };
        &body.source
    }
}

clojure_rt_macros::implements! {
    impl IEquiv for PatternObj {
        fn equiv(this: Value, other: Value) -> Value {
            if other.tag != this.tag {
                return Value::FALSE;
            }
            let a = unsafe { PatternObj::body(this) };
            let b = unsafe { PatternObj::body(other) };
            if *a.source == *b.source { Value::TRUE } else { Value::FALSE }
        }
    }
}

clojure_rt_macros::implements! {
    impl IHash for PatternObj {
        fn hash(this: Value) -> Value {
            let body = unsafe { PatternObj::body(this) };
            let cached = body.hash.load(Ordering::Relaxed);
            if cached != 0 {
                return Value::int(cached as i64);
            }
            let h = murmur3::hash_unencoded_chars(&body.source);
            body.hash.store(h, Ordering::Relaxed);
            Value::int(h as i64)
        }
    }
}
