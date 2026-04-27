//! Port of `clojure.lang.IHashEq` (`int hasheq()`) plus per-primitive
//! impls. Bit-compatible with JVM Clojure's `(hash …)` for the
//! corresponding inputs.
//!
//! Numeric special cases mirror `clojure.lang.Numbers.hasheq`:
//! `Int64` → `Murmur3::hash_long`; `Float64` → `Double.hashCode`-style
//! bit fold, with `-0.0 → 0` so it agrees with the (= 0.0 -0.0) ⇒ true
//! equivalence under `IEquiv`.
//!
//! Boolean uses `1231` / `1237` to match Java's `Boolean.hashCode`.
//! Char uses the codepoint as i32 (Java `Character.hashCode`).

use crate::hash::murmur3;
use crate::primitives::*;
use crate::value::Value;

clojure_rt_macros::protocol! {
    pub trait IHashEq {
        fn hasheq(this: ::clojure_rt::Value) -> ::clojure_rt::Value;
    }
}

clojure_rt_macros::implements! {
    impl IHashEq for Nil {
        fn hasheq(this: Value) -> Value {
            let _ = this;
            Value::int(0)
        }
    }
}

clojure_rt_macros::implements! {
    impl IHashEq for Bool {
        fn hasheq(this: Value) -> Value {
            // Java's Boolean.hashCode: true → 1231, false → 1237.
            let h = if this.payload != 0 { 1231 } else { 1237 };
            Value::int(h)
        }
    }
}

clojure_rt_macros::implements! {
    impl IHashEq for Int64 {
        fn hasheq(this: Value) -> Value {
            let n = this.payload as i64;
            Value::int(murmur3::hash_long(n) as i64)
        }
    }
}

clojure_rt_macros::implements! {
    impl IHashEq for Float64 {
        fn hasheq(this: Value) -> Value {
            let x = f64::from_bits(this.payload);
            // -0.0 hashes the same as 0.0 (Numbers.hasheq override).
            // `x == 0.0` is true for both signs of zero; the
            // `is_sign_negative` selects -0.0 specifically.
            if x == 0.0 && x.is_sign_negative() {
                return Value::int(0);
            }
            // Java Double.hashCode: `(int)(bits ^ (bits >>> 32))`.
            let bits = this.payload;
            let h = ((bits >> 32) as i32) ^ (bits as i32);
            Value::int(h as i64)
        }
    }
}

clojure_rt_macros::implements! {
    impl IHashEq for Char {
        fn hasheq(this: Value) -> Value {
            // Java Character.hashCode is `(int)value` — i.e. the
            // codepoint. Our payload already holds the codepoint.
            Value::int(this.payload as i64)
        }
    }
}
