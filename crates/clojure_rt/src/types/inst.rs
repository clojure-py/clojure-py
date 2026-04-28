//! `InstObj` — instant in time, the `#inst "..."` reader literal
//! target. Mirrors JVM `java.util.Date` (which Clojure uses
//! directly for `#inst`).
//!
//! Stored as `i64` milliseconds since the Unix epoch — same
//! resolution and reference point as `Date.getTime()`. Parses
//! RFC-3339 / ISO-8601 strings via `chrono::DateTime::parse_from_rfc3339`,
//! which covers the literal forms the JVM reader accepts:
//! `"2024-01-01T00:00:00Z"`, `"2024-01-01T00:00:00.123+02:00"`,
//! etc.
//!
//! Equality is `i64`-millis equality. Two `Inst`s representing
//! the same instant — even if originally parsed from differently
//! formatted strings — compare equal. Hash is `hash_long(millis)`,
//! consistent with that.

use core::sync::atomic::{AtomicI32, Ordering};

use chrono::DateTime;

use crate::hash::murmur3;
use crate::protocols::equiv::IEquiv;
use crate::protocols::hash::IHash;
use crate::value::Value;

clojure_rt_macros::register_type! {
    pub struct InstObj {
        millis: i64,
        hash:   AtomicI32,
    }
}

impl InstObj {
    /// Build from a Unix-epoch millisecond count.
    pub fn from_millis(millis: i64) -> Value {
        InstObj::alloc(millis, AtomicI32::new(0))
    }

    /// Parse an RFC-3339 timestamp (the `#inst "..."` reader
    /// literal target). Returns an exception value on parse
    /// failure.
    pub fn from_rfc3339(s: &str) -> Value {
        match DateTime::parse_from_rfc3339(s) {
            Ok(dt) => InstObj::from_millis(dt.timestamp_millis()),
            Err(e) => crate::exception::make_foreign(format!(
                "Inst parse failure for {s:?}: {e}"
            )),
        }
    }

    /// Read back the underlying millisecond count.
    pub fn millis(this: Value) -> i64 {
        let body = unsafe { InstObj::body(this) };
        body.millis
    }
}

clojure_rt_macros::implements! {
    impl IEquiv for InstObj {
        fn equiv(this: Value, other: Value) -> Value {
            if other.tag != this.tag {
                return Value::FALSE;
            }
            let a = unsafe { InstObj::body(this) };
            let b = unsafe { InstObj::body(other) };
            if a.millis == b.millis { Value::TRUE } else { Value::FALSE }
        }
    }
}

clojure_rt_macros::implements! {
    impl IHash for InstObj {
        fn hash(this: Value) -> Value {
            let body = unsafe { InstObj::body(this) };
            let cached = body.hash.load(Ordering::Relaxed);
            if cached != 0 {
                return Value::int(cached as i64);
            }
            let h = murmur3::hash_long(body.millis);
            body.hash.store(h, Ordering::Relaxed);
            Value::int(h as i64)
        }
    }
}
