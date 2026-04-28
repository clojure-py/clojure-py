//! `BigDecimalObj` — arbitrary-precision decimal. Mirrors JVM
//! `java.math.BigDecimal` (the `3.14M` literal target).
//!
//! Storage is the `bigdecimal::BigDecimal` crate type — unscaled
//! `BigInt` mantissa plus an `i64` scale. JVM Clojure uses
//! `BigDecimal` directly (no wrapper); we wrap it in our heap type
//! to fit into the `Value` system.
//!
//! Equality preserves scale (matches JVM `equals` rather than
//! `compareTo == 0`): `(= 1.0M 1.00M)` is false because the two
//! literals have different scales — they're different
//! `BigDecimal` instances even though they're numerically equal.
//! For numeric equality across types/scales, use the future
//! `num-equiv` path.

use core::sync::atomic::{AtomicI32, Ordering};

use bigdecimal::BigDecimal;

use crate::hash::murmur3;
use crate::protocols::equiv::IEquiv;
use crate::protocols::hash::IHash;
use crate::value::Value;

clojure_rt_macros::register_type! {
    pub struct BigDecimalObj {
        d:    BigDecimal,
        hash: AtomicI32,
    }
}

impl BigDecimalObj {
    pub fn new(d: BigDecimal) -> Value {
        BigDecimalObj::alloc(d, AtomicI32::new(0))
    }

    /// Parse a decimal-literal string (e.g. `"3.14"`, `"-0.001"`,
    /// `"1.5e10"`). Exception value on parse failure.
    pub fn from_str(s: &str) -> Value {
        match s.parse::<BigDecimal>() {
            Ok(d) => BigDecimalObj::new(d),
            Err(e) => crate::exception::make_foreign(format!(
                "BigDecimal parse failure for {s:?}: {e}"
            )),
        }
    }

    /// Borrow the inner `BigDecimal`.
    ///
    /// # Safety
    /// `v` must be a live `BigDecimalObj`-tagged `Value`.
    pub unsafe fn as_bigdecimal(v: Value) -> &'static BigDecimal {
        let body = unsafe { BigDecimalObj::body(v) };
        &body.d
    }
}

clojure_rt_macros::implements! {
    impl IEquiv for BigDecimalObj {
        fn equiv(this: Value, other: Value) -> Value {
            if other.tag != this.tag {
                return Value::FALSE;
            }
            let a = unsafe { BigDecimalObj::body(this) };
            let b = unsafe { BigDecimalObj::body(other) };
            // The `bigdecimal` crate's `PartialEq` is value-equal
            // (Java's `compareTo == 0`); we want JVM `equals` —
            // distinct iff mantissa OR scale differs. Decompose
            // and compare both.
            let (a_m, a_s) = a.d.as_bigint_and_exponent();
            let (b_m, b_s) = b.d.as_bigint_and_exponent();
            if a_s == b_s && a_m == b_m { Value::TRUE } else { Value::FALSE }
        }
    }
}

clojure_rt_macros::implements! {
    impl IHash for BigDecimalObj {
        fn hash(this: Value) -> Value {
            let body = unsafe { BigDecimalObj::body(this) };
            let cached = body.hash.load(Ordering::Relaxed);
            if cached != 0 {
                return Value::int(cached as i64);
            }
            let (mantissa, scale) = body.d.as_bigint_and_exponent();
            let m_bytes = mantissa.to_signed_bytes_be();
            let m_hash = murmur3::hash_ordered(m_bytes.iter().map(|b| *b as i32));
            let s_hash = murmur3::hash_long(scale);
            let h = murmur3::hash_ordered([m_hash, s_hash]);
            body.hash.store(h, Ordering::Relaxed);
            Value::int(h as i64)
        }
    }
}
