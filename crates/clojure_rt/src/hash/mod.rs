//! Hash primitives.
//!
//! `murmur3` is a literal port of `clojure.lang.Murmur3`
//! (`src/jvm/clojure/lang/Murmur3.java` in the Clojure JVM source).
//! Output is bit-compatible with JVM Clojure's `(hash …)`, which the
//! protocol-level `IHash` impls use for primitives. Pure function;
//! no allocator, no global state.

pub mod murmur3;
pub mod util;
