//! Substrate error helpers. Currently panic-only; later rounds will route
//! these to a Clojure-level exception via the `error_value` Value tag.

use crate::protocol::ProtocolMethod;
use crate::type_registry;
use crate::value::TypeId;

/// Panic with a uniform "no impl" message. Centralized so we can swap to
/// raise-as-Value later without touching call sites.
#[cold]
#[inline(never)]
pub fn resolution_failure(method: &ProtocolMethod, type_id: TypeId) -> ! {
    let type_name = type_registry::try_get(type_id)
        .map(|m| m.name)
        .unwrap_or("<unregistered>");
    panic!("clojure_rt: no impl of {} for type {} (id={type_id})",
           method.name, type_name);
}
