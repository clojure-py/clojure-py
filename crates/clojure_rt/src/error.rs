//! Substrate error helpers. Dispatch failures and similar recoverable
//! errors are surfaced as throwable Values (see `crate::exception`),
//! not as panics — panics in `clojure_rt` are reserved for true bugs
//! (corrupted Value tags, broken invariants) so that an embedded Python
//! REPL stays alive across user-level errors.

use crate::exception;
use crate::protocol::ProtocolMethod;
use crate::value::{TypeId, Value};

/// Build a uniform "no impl of `proto/method` for `type`" exception Value.
/// Centralized so future error machinery (richer ex-info maps, source
/// locations, etc.) only has to grow in one place.
#[cold]
#[inline(never)]
pub fn resolution_failure(method: &ProtocolMethod, type_id: TypeId) -> Value {
    exception::make_no_impl(method, type_id)
}
