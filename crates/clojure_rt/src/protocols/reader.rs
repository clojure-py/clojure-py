//! `IReader` — character-source protocol. Mirrors cljs
//! `cljs.core/IReader` (the in-memory analog of `java.io.Reader`):
//! `-read-char` and `-peek-char` are the two primitives.
//!
//! Readers return characters as `Value::char(c)` or `Value::NIL`
//! at end-of-input. We don't differentiate "end of file" from
//! "would block" — synchronous, blocking read is the only model
//! we support; non-blocking IO would need a separate protocol.
//!
//! `IPushbackReader` (a small extension protocol below in
//! `pushback_reader.rs`) adds `-unread`, the "put one char back"
//! primitive the reader needs for one-char lookahead. JVM's
//! `PushbackReader` exposes both reading and unread; cljs splits
//! them into two protocols since some sources can support read +
//! peek but not arbitrary unread.

clojure_rt_macros::protocol! {
    pub trait IReader {
        fn read_char(this: ::clojure_rt::Value) -> ::clojure_rt::Value;
        fn peek_char(this: ::clojure_rt::Value) -> ::clojure_rt::Value;
    }
}
