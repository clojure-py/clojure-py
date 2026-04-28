//! `IWriter` — character-sink protocol. Mirrors cljs
//! `cljs.core/IWriter` (the in-memory analog of `java.io.Writer`):
//! `-write` accepts a string and appends it; `-flush` is a hint
//! to commit any buffered output (no-op for in-memory writers).
//!
//! Used by `print` / `pr` / `println` once those land. The reader
//! never writes — IWriter is here so the IO surface lands as one
//! coherent slice rather than getting bolted on later.

clojure_rt_macros::protocol! {
    pub trait IWriter {
        fn write(this: ::clojure_rt::Value, s: ::clojure_rt::Value)
            -> ::clojure_rt::Value;
        fn flush(this: ::clojure_rt::Value) -> ::clojure_rt::Value;
    }
}
