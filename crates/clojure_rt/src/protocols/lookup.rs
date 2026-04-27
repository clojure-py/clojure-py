//! `ILookup` — key-based lookup. Multi-arity:
//!
//! - `lookup_2(coll, k)` — returns `Value::NIL` on miss.
//! - `lookup_3(coll, k, not_found)` — returns `not_found` on miss
//!   (preserves the "found nil vs missing" distinction for maps).

clojure_rt_macros::protocol! {
    pub trait ILookup {
        fn lookup_2(this: ::clojure_rt::Value, k: ::clojure_rt::Value)
            -> ::clojure_rt::Value;
        fn lookup_3(
            this: ::clojure_rt::Value,
            k: ::clojure_rt::Value,
            not_found: ::clojure_rt::Value,
        ) -> ::clojure_rt::Value;
    }
}
