//! Throwable Values.
//!
//! Dispatch failures and similar recoverable errors are represented as
//! heap-allocated `ExceptionObject`s, returned as ordinary `Value`s.
//! Callers detect them via `Value::is_exception()` and treat them like
//! any other Value — they propagate through `rt::*` helpers transparently
//! and form the basis for future Clojure-level `try/catch`.
//!
//! This is the value-level analogue of Clojure-JVM's `Throwable`. Future
//! work can grow `ExceptionObject` to carry a Clojure-side `ex-info` map
//! once `IPersistentMap` exists; for now the payload is a kind enum + a
//! human-readable message.

use crate::protocol::ProtocolMethod;
use crate::type_registry;
use crate::value::{TypeId, Value};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ExceptionKind {
    NoProtocolImpl,
    /// Raised by a foreign-language embedding (PyO3 today, possibly
    /// other hosts later) when the embedding language's exception
    /// machinery throws inside a protocol impl. The message carries the
    /// embedding-language type name and details.
    Foreign,
}

clojure_rt_macros::register_type! {
    pub struct ExceptionObject {
        kind:    ExceptionKind,
        message: Box<str>,
    }
}

/// Construct a `NoProtocolImpl` exception Value naming the protocol
/// method and the offending type.
pub fn make_no_impl(method: &ProtocolMethod, type_id: TypeId) -> Value {
    let type_name = type_registry::try_get(type_id)
        .map(|m| m.name)
        .unwrap_or("<primitive>");
    let message = format!(
        "No matching impl of protocol method `{}` for type `{}` (id={})",
        method.name, type_name, type_id,
    );
    ExceptionObject::alloc(ExceptionKind::NoProtocolImpl, message.into_boxed_str())
}

/// Construct a `Foreign` exception Value carrying a message produced by
/// an embedding-language exception bridge (e.g. PyO3's `PyErr` →
/// throwable Value path in `clojure_core`).
pub fn make_foreign(message: String) -> Value {
    ExceptionObject::alloc(ExceptionKind::Foreign, message.into_boxed_str())
}

/// Borrow the message of an exception Value, copied to an owned `String`
/// for caller convenience. Returns `None` for non-exception Values.
pub fn message(v: Value) -> Option<String> {
    if !v.is_exception() {
        return None;
    }
    Some(unsafe { ExceptionObject::body(v) }.message.to_string())
}

/// Borrow the kind of an exception Value. Returns `None` for non-exception
/// Values.
pub fn kind(v: Value) -> Option<ExceptionKind> {
    if !v.is_exception() {
        return None;
    }
    Some(unsafe { ExceptionObject::body(v) }.kind)
}
