//! `IFn` — the "this is callable" protocol. Dispatches `(f a₁ … aₙ)`
//! to a per-arity slot. Following the project memo: Python callables
//! are Clojure callables and vice-versa, so anything implementing
//! `__call__` (i.e. subclasses of `collections.abc.Callable`) gets
//! IFn for free via the ABC inheritance walk in `clojure_py`.
//!
//! **Naming**: each slot's suffix is the *total Rust arity* (= the
//! number of positional `Value`s passed through dispatch), which is
//! the user's Clojure-visible arity **plus one** for the receiver
//! `this`. So `invoke_1(this)` is `(f)`, `invoke_2(this, a)` is
//! `(f a)`, `invoke_3(this, a, b)` is `(f a b)`, and so on. Call sites
//! use the un-suffixed stem (`dispatch!(IFn::invoke, &[f, a, b])`)
//! and the multi-arity macro routes to the right slot.
//!
//! The current cap is six arities (0..=5 user args). That covers
//! `reduce`, `map`, `filter`, and the bulk of `clojure.core`'s
//! higher-order fns. Extending the cap is a copy-paste of one trait
//! method line plus one Python-side per-arity adapter; we'll grow it
//! as soon as something needs more.
//!
//! Variadic invocation (`apply`) lands separately when we wire up
//! `apply_to(this, args_seq)` — not strictly needed yet because the
//! initial consumers (reduce, map, filter) call known fixed arities.

clojure_rt_macros::protocol! {
    pub trait IFn {
        fn invoke_1(this: ::clojure_rt::Value) -> ::clojure_rt::Value;
        fn invoke_2(this: ::clojure_rt::Value, a1: ::clojure_rt::Value)
            -> ::clojure_rt::Value;
        fn invoke_3(this: ::clojure_rt::Value, a1: ::clojure_rt::Value, a2: ::clojure_rt::Value)
            -> ::clojure_rt::Value;
        fn invoke_4(
            this: ::clojure_rt::Value,
            a1: ::clojure_rt::Value, a2: ::clojure_rt::Value, a3: ::clojure_rt::Value,
        ) -> ::clojure_rt::Value;
        fn invoke_5(
            this: ::clojure_rt::Value,
            a1: ::clojure_rt::Value, a2: ::clojure_rt::Value,
            a3: ::clojure_rt::Value, a4: ::clojure_rt::Value,
        ) -> ::clojure_rt::Value;
        fn invoke_6(
            this: ::clojure_rt::Value,
            a1: ::clojure_rt::Value, a2: ::clojure_rt::Value, a3: ::clojure_rt::Value,
            a4: ::clojure_rt::Value, a5: ::clojure_rt::Value,
        ) -> ::clojure_rt::Value;
    }
}
