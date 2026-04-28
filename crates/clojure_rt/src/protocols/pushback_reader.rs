//! `IPushbackReader` — unread extension to `IReader`. Mirrors cljs
//! `cljs.core/IPushbackReader` (and JVM `java.io.PushbackReader`'s
//! `unread`). One-char pushback is enough for the LispReader; the
//! protocol could grow to multi-char unread later if needed.

clojure_rt_macros::protocol! {
    pub trait IPushbackReader {
        fn unread(this: ::clojure_rt::Value, c: ::clojure_rt::Value)
            -> ::clojure_rt::Value;
    }
}
