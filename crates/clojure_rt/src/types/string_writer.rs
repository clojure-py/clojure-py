//! `StringWriter` — in-memory character sink. `write` appends the
//! string to an internal buffer; `flush` is a no-op (nothing to
//! commit). `to_string(this)` snapshots the accumulated buffer.
//! Mirrors `java.io.StringWriter` / cljs `cljs.core.StringBufferWriter`.

use parking_lot::Mutex;

use crate::protocols::writer::IWriter;
use crate::types::string::StringObj;
use crate::value::Value;

pub(crate) struct StringWriterInner {
    buf: String,
}

clojure_rt_macros::register_type! {
    pub struct StringWriter {
        inner: Mutex<StringWriterInner>,
    }
}

impl StringWriter {
    /// Build a fresh empty `StringWriter`.
    pub fn new() -> Value {
        StringWriter::alloc(Mutex::new(StringWriterInner {
            buf: String::new(),
        }))
    }

    /// Snapshot the accumulated contents as a fresh `StringObj`
    /// `Value`. The internal buffer is left intact — callers can
    /// continue writing.
    pub fn to_string(this: Value) -> Value {
        let body = unsafe { StringWriter::body(this) };
        let g = body.inner.lock();
        crate::rt::str_new(&g.buf)
    }
}

clojure_rt_macros::implements! {
    impl IWriter for StringWriter {
        fn write(this: Value, s: Value) -> Value {
            // Accept anything we can read as a UTF-8 str — for
            // now, just StringObj. Once we add IString or similar
            // we'll widen the input. char Values are common too;
            // write-char is the JVM analog and we'd add it as a
            // separate protocol method.
            let string_tid = match crate::types::string::STRINGOBJ_TYPE_ID.get() {
                Some(&id) => id,
                None => return crate::exception::make_foreign(
                    "StringWriter::write: string type not initialized".to_string(),
                ),
            };
            if s.tag != string_tid {
                return crate::exception::make_foreign(
                    "StringWriter::write: argument must be a String".to_string(),
                );
            }
            let body = unsafe { StringWriter::body(this) };
            let s_ref = unsafe { StringObj::as_str_unchecked(s) };
            body.inner.lock().buf.push_str(s_ref);
            Value::NIL
        }

        fn flush(_this: Value) -> Value {
            // In-memory writer — nothing to commit.
            Value::NIL
        }
    }
}
