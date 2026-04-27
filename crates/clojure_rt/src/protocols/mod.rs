//! Ports of Clojure's Java interfaces (under
//! `src/jvm/clojure/lang/` in the Clojure JVM source tree) as protocols.
//! Names follow the ClojureScript convention (I-prefix, lowercase
//! method names matching the cljs `-method` form).
//!
//! Each child module declares one protocol via `protocol!` and any
//! built-in fallback semantics for primitive Value tags. Per-type impls
//! for concrete types (PersistentList, etc.) live alongside those types.

pub mod counted;
pub mod equiv;
pub mod hash;
pub mod meta;
pub mod named;
