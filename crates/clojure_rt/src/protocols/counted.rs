//! Port of `clojure.lang.Counted` (`int count()`).
//!
//! Built-in fallback handles the primitive tags Clojure defines semantics
//! for; everything else delegates to `error::resolution_failure`, which
//! returns a throwable Value.

clojure_rt_macros::protocol! {
    pub trait Counted {
        fn count(this: ::clojure_rt::Value) -> ::clojure_rt::Value {
            if this.tag == ::clojure_rt::TYPE_NIL {
                return ::clojure_rt::Value::int(0);
            }
            ::clojure_rt::error::resolution_failure(&COUNT, this.tag)
        }
    }
}
