//! Port of Clojure's `=` (`clojure.lang.Util.equiv`) as a per-class
//! protocol on the receiver. Each primitive's impl checks "is `other`
//! my type? if so, do my type's value comparison."
//!
//! Category-discriminated: `(= 1 1.0)` is *false* — `Numbers.equal`
//! requires `category(x) == category(y)` before doing the numeric
//! comparison, so Int64 vs Float64 never crosses. This is what
//! preserves the contract `equiv(a, b) ⟹ hash(a) == hash(b)`,
//! since Int64 and Float64 hash differently for the same numeric
//! value.
//!
//! Floats use `==` semantics (matching Java's `==` on doubles, which
//! Numbers.equal routes through): `(= 0.0 -0.0)` is *true*, but
//! `(= ##NaN ##NaN)` is *false*. Bool / Char / Int64 use payload
//! equality.

use crate::primitives::*;
use crate::value::{Value, TYPE_BOOL, TYPE_CHAR, TYPE_FLOAT64, TYPE_INT64};

clojure_rt_macros::protocol! {
    pub trait IEquiv {
        fn equiv(this: ::clojure_rt::Value, other: ::clojure_rt::Value)
            -> ::clojure_rt::Value;
    }
}

clojure_rt_macros::implements! {
    impl IEquiv for Nil {
        fn equiv(this: Value, other: Value) -> Value {
            let _ = this;
            if other.is_nil() { Value::TRUE } else { Value::FALSE }
        }
    }
}

clojure_rt_macros::implements! {
    impl IEquiv for Bool {
        fn equiv(this: Value, other: Value) -> Value {
            if other.tag == TYPE_BOOL && this.payload == other.payload {
                Value::TRUE
            } else {
                Value::FALSE
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl IEquiv for Int64 {
        fn equiv(this: Value, other: Value) -> Value {
            if other.tag == TYPE_INT64 && this.payload == other.payload {
                Value::TRUE
            } else {
                Value::FALSE
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl IEquiv for Float64 {
        fn equiv(this: Value, other: Value) -> Value {
            if other.tag != TYPE_FLOAT64 {
                return Value::FALSE;
            }
            let a = f64::from_bits(this.payload);
            let b = f64::from_bits(other.payload);
            // `==` on f64: NaN ≠ NaN, +0.0 == -0.0. Matches Numbers.equal.
            if a == b { Value::TRUE } else { Value::FALSE }
        }
    }
}

clojure_rt_macros::implements! {
    impl IEquiv for Char {
        fn equiv(this: Value, other: Value) -> Value {
            if other.tag == TYPE_CHAR && this.payload == other.payload {
                Value::TRUE
            } else {
                Value::FALSE
            }
        }
    }
}
