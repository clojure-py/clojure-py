//! `IIndexingReader` — line and column tracking. Mirrors
//! `clojure.tools.reader.reader-types/IndexingReader` (cljs's
//! reader-types namespace under tools.reader).
//!
//! Reports the position of the *next* character to be read (1-based
//! line and column, matching JVM `LineNumberingPushbackReader`).
//! The reader records `(current-line, current-column)` before each
//! form it parses and attaches the result via `with_source_pos`
//! to the form's metadata.
//!
//! Implementations track line/col across `read-char` and undo
//! the advance on `unread` so push-back doesn't corrupt the
//! position.

clojure_rt_macros::protocol! {
    pub trait IIndexingReader {
        fn current_line(this: ::clojure_rt::Value) -> ::clojure_rt::Value;
        fn current_column(this: ::clojure_rt::Value) -> ::clojure_rt::Value;
    }
}
