//! `RatioObj` — exact rational number `n/d`. Mirrors JVM
//! `clojure.lang.Ratio`.
//!
//! Storage is `num_rational::BigRational` (= `Ratio<BigInt>`), so
//! both numerator and denominator have arbitrary precision.
//! Construction never normalizes implicitly — JVM's `Ratio`
//! likewise leaves you with the literal numerator/denominator,
//! since `(/ 6 4)` *does* normalize at the operator level (it
//! produces `3/2`) but `(Ratio. ...)` does not. We follow the same:
//! `RatioObj::new(num, den)` is the literal constructor;
//! `RatioObj::canonical` is the GCD-reducing one and is what the
//! reader will route `1/3`-style literals through.
//!
//! `IEquiv` matches Clojure's `=` semantics — two ratios are equal
//! iff their canonical forms agree (so `4/2` and `2/1` are equal).
//! `BigRational` already does this via `PartialEq`.
//!
//! Ratio vs Long / BigInt cross-type equality (`(= 1 1/1)`) is
//! false — `=` is type-strict. The future `num-equiv` path handles
//! `(== 1 1/1)`.

use core::sync::atomic::{AtomicI32, Ordering};

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{One, Zero};

use crate::hash::murmur3;
use crate::protocols::equiv::IEquiv;
use crate::protocols::hash::IHash;
use crate::value::Value;

clojure_rt_macros::register_type! {
    pub struct RatioObj {
        r:    BigRational,
        hash: AtomicI32,
    }
}

impl RatioObj {
    /// Wrap an already-built `BigRational`. Caller transfers
    /// ownership.
    pub fn new(r: BigRational) -> Value {
        RatioObj::alloc(r, AtomicI32::new(0))
    }

    /// Build from `(numerator, denominator)`, reducing to lowest
    /// terms (GCD-divided) and normalizing sign so the denominator
    /// is positive. Returns an exception value on division by zero.
    /// This is the constructor the reader uses for `n/d` literals.
    pub fn canonical(numer: BigInt, denom: BigInt) -> Value {
        if denom.is_zero() {
            return crate::exception::make_foreign(
                "Divide by zero".to_string(),
            );
        }
        // BigRational::new normalizes (gcd-reduce + denom-sign).
        RatioObj::new(BigRational::new(numer, denom))
    }

    /// Convenience: build from i64 numerator/denominator.
    pub fn from_i64s(numer: i64, denom: i64) -> Value {
        RatioObj::canonical(BigInt::from(numer), BigInt::from(denom))
    }

    /// Borrow numerator. Lifetime tied to the Value's liveness.
    ///
    /// # Safety
    /// `v` must be a live `RatioObj`-tagged `Value`.
    pub unsafe fn numerator(v: Value) -> &'static BigInt {
        let body = unsafe { RatioObj::body(v) };
        body.r.numer()
    }

    /// Borrow denominator. Lifetime tied to the Value's liveness.
    ///
    /// # Safety
    /// `v` must be a live `RatioObj`-tagged `Value`.
    pub unsafe fn denominator(v: Value) -> &'static BigInt {
        let body = unsafe { RatioObj::body(v) };
        body.r.denom()
    }

    /// Whole-number predicate — `true` iff the denominator is 1
    /// (the ratio is an integer in disguise). Useful for the
    /// reader's auto-collapse rule and for arithmetic that wants
    /// to demote.
    pub fn is_whole(v: Value) -> bool {
        let body = unsafe { RatioObj::body(v) };
        body.r.denom().is_one()
    }
}

clojure_rt_macros::implements! {
    impl IEquiv for RatioObj {
        fn equiv(this: Value, other: Value) -> Value {
            if other.tag != this.tag {
                return Value::FALSE;
            }
            let a = unsafe { RatioObj::body(this) };
            let b = unsafe { RatioObj::body(other) };
            if a.r == b.r { Value::TRUE } else { Value::FALSE }
        }
    }
}

clojure_rt_macros::implements! {
    impl IHash for RatioObj {
        fn hash(this: Value) -> Value {
            let body = unsafe { RatioObj::body(this) };
            let cached = body.hash.load(Ordering::Relaxed);
            if cached != 0 {
                return Value::int(cached as i64);
            }
            // Hash combines the canonical numerator + denominator
            // signed-byte reps. Since `BigRational::new` already
            // normalized, equal ratios produce equal hashes.
            let n_bytes = body.r.numer().to_signed_bytes_be();
            let d_bytes = body.r.denom().to_signed_bytes_be();
            let n_hash = murmur3::hash_ordered(n_bytes.iter().map(|b| *b as i32));
            let d_hash = murmur3::hash_ordered(d_bytes.iter().map(|b| *b as i32));
            let h = murmur3::hash_ordered([n_hash, d_hash]);
            body.hash.store(h, Ordering::Relaxed);
            Value::int(h as i64)
        }
    }
}
