//! Ports of Clojure's Java interfaces (under
//! `src/jvm/clojure/lang/` in the Clojure JVM source tree) as protocols.
//!
//! Each child module declares one protocol via `protocol!` and any
//! built-in fallback semantics for primitive Value tags. Per-type impls
//! for concrete types (PersistentList, etc.) live alongside those types.

pub mod counted;
pub mod equiv;
pub mod hash_eq;
pub mod meta;
pub mod named;
