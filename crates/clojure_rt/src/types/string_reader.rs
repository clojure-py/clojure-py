//! `StringReader` — in-memory character source backed by an
//! immutable string and a mutable cursor. Tracks line + column for
//! source-position metadata; satisfies `IReader`,
//! `IPushbackReader`, and `IIndexingReader`.
//!
//! The cursor and line/column counters live behind
//! `parking_lot::Mutex` since the reader gets handed around as a
//! `Value` and the type system requires `Sync`. Real-world reader
//! use is single-threaded by contract — the lock is uncontended.
//!
//! Position semantics: `current_line` / `current_column` report
//! the line/col of the *next* character to be read (1-based,
//! matching JVM `LineNumberingPushbackReader`). After reading a
//! `\n`, line bumps and column resets to 1; otherwise column
//! bumps. `unread` restores the line/col to where they were
//! before the most recent `read_char`, so a single push-back
//! cycle round-trips cleanly.

use parking_lot::Mutex;

use crate::protocols::indexing_reader::IIndexingReader;
use crate::protocols::pushback_reader::IPushbackReader;
use crate::protocols::reader::IReader;
use crate::value::Value;

pub(crate) struct StringReaderInner {
    /// Source bytes, immutable. `Box<str>` so we own the buffer
    /// and aren't tied to any caller-borrowed lifetime.
    src: Box<str>,
    /// Byte position of the next char to read. Always on a UTF-8
    /// char boundary.
    pos: usize,
    /// Line of the next char (1-based).
    line: i64,
    /// Column of the next char (1-based).
    column: i64,
    /// Snapshot of `(line, column)` taken *before* the most recent
    /// `read_char` advanced them. Used by `unread` to restore
    /// position. Initialized to `(1, 1)`; only meaningful after
    /// at least one read.
    prev_line: i64,
    prev_column: i64,
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
            line: 1,
            column: 1,
            prev_line: 1,
            prev_column: 1,
            unread: None,
        }))
    }
}

/// Advance line/column past a single character read. Newline bumps
/// the line and resets column to 1; everything else bumps column.
#[inline]
fn advance_pos(inner: &mut StringReaderInner, c: char) {
    inner.prev_line = inner.line;
    inner.prev_column = inner.column;
    if c == '\n' {
        inner.line += 1;
        inner.column = 1;
    } else {
        inner.column += 1;
    }
}

clojure_rt_macros::implements! {
    impl IReader for StringReader {
        fn read_char(this: Value) -> Value {
            let body = unsafe { StringReader::body(this) };
            let mut g = body.inner.lock();
            // Pushback first.
            if let Some(c) = g.unread.take() {
                advance_pos(&mut g, c);
                return Value::char(c);
            }
            let rest = &g.src[g.pos..];
            let mut chars = rest.chars();
            match chars.next() {
                Some(c) => {
                    g.pos += c.len_utf8();
                    advance_pos(&mut g, c);
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
            // Decode the char Value. The payload is a u32 codepoint
            // stored as u64.
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
            // Restore the line/col snapshot taken at the most recent
            // read so the next `current_line` / `current_column`
            // reflects the unread char's position.
            g.line = g.prev_line;
            g.column = g.prev_column;
            g.unread = Some(ch);
            Value::NIL
        }
    }
}

clojure_rt_macros::implements! {
    impl IIndexingReader for StringReader {
        fn current_line(this: Value) -> Value {
            let body = unsafe { StringReader::body(this) };
            let g = body.inner.lock();
            Value::int(g.line)
        }

        fn current_column(this: Value) -> Value {
            let body = unsafe { StringReader::body(this) };
            let g = body.inner.lock();
            Value::int(g.column)
        }
    }
}
