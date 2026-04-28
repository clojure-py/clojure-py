//! `IReference` — mutable-meta slot for reference types. Mirrors the
//! JVM `clojure.lang.IReference` interface (`alterMeta` / `resetMeta`).
//!
//! Atoms (and other ref types) carry meta that *changes in place* —
//! distinct from the value-semantic `IWithMeta::with-meta` on
//! immutable values. `reset-meta!` overwrites; `alter-meta!` applies
//! a function to the current meta plus extra args under CAS retry,
//! mirroring the multi-arity shape of `IAtom::swap_<N>`.

clojure_rt_macros::protocol! {
    pub trait IReference {
        fn reset_meta(
            this: ::clojure_rt::Value,
            m:    ::clojure_rt::Value,
        ) -> ::clojure_rt::Value;
        fn alter_meta_2(
            this: ::clojure_rt::Value,
            f:    ::clojure_rt::Value,
        ) -> ::clojure_rt::Value;
        fn alter_meta_3(
            this: ::clojure_rt::Value,
            f:    ::clojure_rt::Value,
            a1:   ::clojure_rt::Value,
        ) -> ::clojure_rt::Value;
        fn alter_meta_4(
            this: ::clojure_rt::Value,
            f:    ::clojure_rt::Value,
            a1:   ::clojure_rt::Value,
            a2:   ::clojure_rt::Value,
        ) -> ::clojure_rt::Value;
        fn alter_meta_5(
            this: ::clojure_rt::Value,
            f:    ::clojure_rt::Value,
            a1:   ::clojure_rt::Value,
            a2:   ::clojure_rt::Value,
            a3:   ::clojure_rt::Value,
        ) -> ::clojure_rt::Value;
    }
}
