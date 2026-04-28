//! `BigIntObj` — arbitrary-precision integer. Mirrors JVM
//! `clojure.lang.BigInt` (the wrapper around BigInteger).
//!
//! Storage is `num_bigint::BigInt` — full arbitrary precision, no
//! Long fast-path optimization yet (JVM's `BigInt` keeps a `long`
//! beside the `BigInteger` so small values stay unboxed within the
//! BigInt type; we don't need that since small ints already live
//! in the inline `Value::int` tag).
//!
//! Equality + hash: equal iff the BigInt values are equal under
//! `num_bigint::BigInt::eq`. Hash uses Murmur3 over the canonical
//! signed-byte representation so two BigInts representing the same
//! number always hash the same regardless of internal limb layout.
//!
//! Cross-type numeric equality (e.g. `(== 1 1N)`) is *not* handled
//! here — that lives in the future `num-equiv` path. `IEquiv`
//! follows Clojure's `=` semantics: `(= 1 1N)` is false because
//! Long ≠ BigInt under `=`.

use core::sync::atomic::{AtomicI32, Ordering};

use num_bigint::BigInt;

use crate::hash::murmur3;
use crate::protocols::equiv::IEquiv;
use crate::protocols::hash::IHash;
use crate::value::Value;

clojure_rt_macros::register_type! {
    pub struct BigIntObj {
        // Heap-allocated BigInt value. `register_type!`'s
        // drop_in_place runs BigInt's Drop, freeing the digit
        // buffer.
        n:    BigInt,
        hash: AtomicI32,
    }
}

impl BigIntObj {
    /// Wrap a Rust `BigInt`. Caller transfers ownership.
    pub fn new(n: BigInt) -> Value {
        BigIntObj::alloc(n, AtomicI32::new(0))
    }

    /// Parse a decimal string. Returns an exception value on parse
    /// failure (matches the rest of the runtime's "errors are
    /// values" convention).
    pub fn from_str(s: &str) -> Value {
        match s.parse::<BigInt>() {
            Ok(n) => BigIntObj::new(n),
            Err(e) => crate::exception::make_foreign(format!(
                "BigInt parse failure for {s:?}: {e}"
            )),
        }
    }

    /// Build from a Rust `i64` — useful in tests and for the
    /// reader's BigInt-suffix path on small literals.
    pub fn from_i64(n: i64) -> Value {
        BigIntObj::new(BigInt::from(n))
    }

    /// Borrow the inner BigInt. Lifetime is tied to the Value's
    /// liveness — caller must keep the originating Value live for
    /// the duration of the borrow.
    ///
    /// # Safety
    /// `v` must be a live `BigIntObj`-tagged `Value`.
    pub unsafe fn as_bigint(v: Value) -> &'static BigInt {
        let body = unsafe { BigIntObj::body(v) };
        &body.n
    }
}

clojure_rt_macros::implements! {
    impl IEquiv for BigIntObj {
        fn equiv(this: Value, other: Value) -> Value {
            if other.tag != this.tag {
                return Value::FALSE;
            }
            let a = unsafe { BigIntObj::body(this) };
            let b = unsafe { BigIntObj::body(other) };
            if a.n == b.n { Value::TRUE } else { Value::FALSE }
        }
    }
}

clojure_rt_macros::implements! {
    impl IHash for BigIntObj {
        fn hash(this: Value) -> Value {
            let body = unsafe { BigIntObj::body(this) };
            let cached = body.hash.load(Ordering::Relaxed);
            if cached != 0 {
                return Value::int(cached as i64);
            }
            // Hash the canonical signed-bytes-big-endian rep so
            // identical numerical values produce identical hashes
            // regardless of internal limb width. NOTE: not bit-
            // compatible with JVM Clojure's BigInteger.hashCode
            // (which uses Java's polynomial-31 byte hash) — match
            // is on the to-do list.
            let bytes = body.n.to_signed_bytes_be();
            let h = murmur3::hash_ordered(bytes.iter().map(|b| *b as i32));
            body.hash.store(h, Ordering::Relaxed);
            Value::int(h as i64)
        }
    }
}
