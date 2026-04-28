//! `Reduced` — a tiny one-field wrapper that signals early
//! termination from a reduce. Returning `(reduced x)` from the step
//! function tells the surrounding reduce to stop and yield `x`.
//!
//! Mirrors `clojure.lang.Reduced` (JVM) and `cljs.core/Reduced`.
//! Implements only `IDeref` — `(deref r)` returns the wrapped value.
//! `is-reduced?` is a tag check via `rt::is_reduced`.

use crate::protocols::deref::IDeref;
use crate::value::Value;

clojure_rt_macros::register_type! {
    pub struct Reduced {
        value: Value,
    }
}

impl Reduced {
    /// Wrap `v` as a `Reduced`. Caller transfers one ref of `v` to
    /// the new wrapper; the wrapper's destructor decrefs it.
    #[inline]
    pub fn wrap(v: Value) -> Value {
        Reduced::alloc(v)
    }

    /// Type-id of the `Reduced` heap type. Convenient for
    /// `rt::is_reduced` so it's a single tag compare.
    #[inline]
    pub fn type_id() -> crate::value::TypeId {
        *REDUCED_TYPE_ID.get().expect("Reduced: clojure_rt::init() not called")
    }
}

clojure_rt_macros::implements! {
    impl IDeref for Reduced {
        fn deref(this: Value) -> Value {
            let v = unsafe { Reduced::body(this) }.value;
            crate::rc::dup(v);
            v
        }
    }
}
