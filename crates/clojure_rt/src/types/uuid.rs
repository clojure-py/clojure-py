//! `UUIDObj` — RFC-4122 universally-unique identifier. Mirrors
//! JVM `java.util.UUID` (the `#uuid "..."` reader literal target).
//!
//! Stored as the `uuid` crate's `Uuid` (16 bytes inline). Two
//! UUIDs are equal iff their 128-bit values match; hash is
//! Murmur3 over the 16-byte big-endian representation.

use core::sync::atomic::{AtomicI32, Ordering};

use uuid::Uuid;

use crate::hash::murmur3;
use crate::protocols::equiv::IEquiv;
use crate::protocols::hash::IHash;
use crate::value::Value;

clojure_rt_macros::register_type! {
    pub struct UUIDObj {
        uuid: Uuid,
        hash: AtomicI32,
    }
}

impl UUIDObj {
    /// Build from an existing `Uuid` value.
    pub fn new(uuid: Uuid) -> Value {
        UUIDObj::alloc(uuid, AtomicI32::new(0))
    }

    /// Parse a UUID string in any of the formats the `uuid` crate
    /// accepts (hyphenated, simple, urn:uuid, braced). Exception
    /// value on parse failure — the reader's `#uuid "..."` path
    /// uses this.
    pub fn from_str(s: &str) -> Value {
        match Uuid::parse_str(s) {
            Ok(u) => UUIDObj::new(u),
            Err(e) => crate::exception::make_foreign(format!(
                "UUID parse failure for {s:?}: {e}"
            )),
        }
    }

    /// Read back the underlying `Uuid`.
    pub fn as_uuid(this: Value) -> Uuid {
        let body = unsafe { UUIDObj::body(this) };
        body.uuid
    }
}

clojure_rt_macros::implements! {
    impl IEquiv for UUIDObj {
        fn equiv(this: Value, other: Value) -> Value {
            if other.tag != this.tag {
                return Value::FALSE;
            }
            let a = unsafe { UUIDObj::body(this) };
            let b = unsafe { UUIDObj::body(other) };
            if a.uuid == b.uuid { Value::TRUE } else { Value::FALSE }
        }
    }
}

clojure_rt_macros::implements! {
    impl IHash for UUIDObj {
        fn hash(this: Value) -> Value {
            let body = unsafe { UUIDObj::body(this) };
            let cached = body.hash.load(Ordering::Relaxed);
            if cached != 0 {
                return Value::int(cached as i64);
            }
            let bytes = body.uuid.as_bytes();
            let h = murmur3::hash_ordered(bytes.iter().map(|b| *b as i32));
            body.hash.store(h, Ordering::Relaxed);
            Value::int(h as i64)
        }
    }
}
