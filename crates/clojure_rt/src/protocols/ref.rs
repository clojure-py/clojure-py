//! `IRef` — validator slot of the reference-type contract. Mirrors
//! cljs `IRef` (`-set-validator!` / `-get-validator`) and the
//! validator half of JVM `clojure.lang.IRef`.
//!
//! `set-validator!` installs the validator atomically; before the
//! install lands, the atom's *current* value is checked against the
//! new fn — if it fails, the install is rejected (the validator
//! never sees a value it would have refused). After install, every
//! subsequent value transition (reset!/swap!/compare-and-set!) runs
//! the validator on the candidate new value before commit.
//!
//! Watches are a separate protocol (`IWatchable`); ref-meta lives in
//! `IReference`.

clojure_rt_macros::protocol! {
    pub trait IRef {
        fn set_validator(this: ::clojure_rt::Value, f: ::clojure_rt::Value)
            -> ::clojure_rt::Value;
        fn get_validator(this: ::clojure_rt::Value) -> ::clojure_rt::Value;
    }
}
