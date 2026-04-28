//! Ports of Clojure's Java interfaces (under
//! `src/jvm/clojure/lang/` in the Clojure JVM source tree) as protocols.
//! Names follow the ClojureScript convention (I-prefix, lowercase
//! method names matching the cljs `-method` form).
//!
//! One protocol per file. Per-type implementations live alongside the
//! concrete types they extend (`types/<name>.rs`).

pub mod associative;
pub mod collection;
pub mod counted;
pub mod emptyable_collection;
pub mod equiv;
pub mod hash;
pub mod ifn;
pub mod indexed;
pub mod lookup;
pub mod meta;
pub mod named;
pub mod persistent_vector;
pub mod reversible;
pub mod seq;
pub mod sequential;
pub mod stack;
