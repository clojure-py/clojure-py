//! `IWatchable` — watch-list of a reference type. Mirrors cljs
//! `IWatchable` (`-add-watch` / `-remove-watch` / `-notify-watches`).
//!
//! Watches are a `key → callback` map. Each callback's signature is
//! `(fn [key ref old new])`. Installation is CAS-style (`add-watch`
//! / `remove-watch`); firing happens via `notify-watches` after a
//! committed value transition. Watch order is map-iteration order
//! and is not guaranteed by the JVM either.
//!
//! `notify-watches` is part of the protocol (rather than purely an
//! internal helper) so user types implementing this protocol on
//! their own reference shape can opt into the same firing convention.

clojure_rt_macros::protocol! {
    pub trait IWatchable {
        fn add_watch(
            this: ::clojure_rt::Value,
            key:  ::clojure_rt::Value,
            f:    ::clojure_rt::Value,
        ) -> ::clojure_rt::Value;
        fn remove_watch(
            this: ::clojure_rt::Value,
            key:  ::clojure_rt::Value,
        ) -> ::clojure_rt::Value;
        fn notify_watches(
            this: ::clojure_rt::Value,
            old:  ::clojure_rt::Value,
            new:  ::clojure_rt::Value,
        ) -> ::clojure_rt::Value;
    }
}
