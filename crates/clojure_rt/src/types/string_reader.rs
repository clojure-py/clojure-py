//! `StringReader` — in-memory character source backed by an
//! immutable string and a mutable byte-position cursor. The reader
//! consumes one Unicode scalar at a time via `read-char`, and
//! `unread` pushes one char back onto a single-slot lookahead
//! buffer. Mirrors `java.io.StringReader` + `PushbackReader` rolled
//! into one type.
//!
//! The cursor and unread slot live behind `parking_lot::Mutex`
//! since the reader gets handed around as a `Value` and the type
//! system requires `Sync`. Real-world reader use is single-threaded
//! by contract — the lock is uncontended.

use parking_lot::Mutex;

use crate::protocols::pushback_reader::IPushbackReader;
use crate::protocols::reader::IReader;
use crate::value::Value;

pub(crate) struct StringReaderInner {
    /// Source bytes, immutable. `Box<str>` so we own the buffer
    /// and aren't tied to any caller-borrowed lifetime.
    src: Box<str>,
    /// Byte position of the next char to read. Always on a
    /// UTF-8 char boundary.
    pos: usize,
    /// One-slot pushback buffer. `Some` after `unread`; consumed
    /// by the next `read-char`.
    unread: Option<char>,
}

clojure_rt_macros::register_type! {
    pub struct StringReader {
        inner: Mutex<StringReaderInner>,
    }
}

impl StringReader {
    /// Build a fresh `StringReader` over `s`. The string is copied
    /// into the reader's storage — caller can drop the original.
    pub fn from_str(s: &str) -> Value {
        StringReader::alloc(Mutex::new(StringReaderInner {
            src: s.to_string().into_boxed_str(),
            pos: 0,
            unread: None,
        }))
    }
}

clojure_rt_macros::implements! {
    impl IReader for StringReader {
        fn read_char(this: Value) -> Value {
            let body = unsafe { StringReader::body(this) };
            let mut g = body.inner.lock();
            // Pushback first.
            if let Some(c) = g.unread.take() {
                return Value::char(c);
            }
            let rest = &g.src[g.pos..];
            let mut chars = rest.chars();
            match chars.next() {
                Some(c) => {
                    g.pos += c.len_utf8();
                    Value::char(c)
                }
                None => Value::NIL,
            }
        }

        fn peek_char(this: Value) -> Value {
            let body = unsafe { StringReader::body(this) };
            let g = body.inner.lock();
            if let Some(c) = g.unread {
                return Value::char(c);
            }
            let rest = &g.src[g.pos..];
            match rest.chars().next() {
                Some(c) => Value::char(c),
                None => Value::NIL,
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl IPushbackReader for StringReader {
        fn unread(this: Value, c: Value) -> Value {
            let body = unsafe { StringReader::body(this) };
            let mut g = body.inner.lock();
            // Decode the char Value. The payload is a u32
            // codepoint stored as u64.
            let codepoint = c.payload as u32;
            let ch = match char::from_u32(codepoint) {
                Some(ch) => ch,
                None => return crate::exception::make_foreign(
                    format!("StringReader::unread: invalid codepoint {codepoint:#x}"),
                ),
            };
            if g.unread.is_some() {
                return crate::exception::make_foreign(
                    "StringReader::unread: pushback buffer is full".to_string(),
                );
            }
            g.unread = Some(ch);
            Value::NIL
        }
    }
}
