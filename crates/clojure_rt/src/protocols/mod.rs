//! Ports of Clojure's Java interfaces (under
//! `src/jvm/clojure/lang/` in the Clojure JVM source tree) as protocols.
//! Names follow the ClojureScript convention (I-prefix, lowercase
//! method names matching the cljs `-method` form).
//!
//! One protocol per file. Per-type implementations live alongside the
//! concrete types they extend (`types/<name>.rs`).

pub mod associative;
pub mod chunk;
pub mod chunked_seq;
pub mod collection;
pub mod counted;
pub mod deref;
pub mod editable_collection;
pub mod emptyable_collection;
pub mod equiv;
pub mod hash;
pub mod ifn;
pub mod indexed;
pub mod lookup;
pub mod map;
pub mod map_entry;
pub mod meta;
pub mod named;
pub mod persistent_map;
pub mod persistent_vector;
pub mod reduce;
pub mod reversible;
pub mod seq;
pub mod sequential;
pub mod stack;
pub mod transient_associative;
pub mod transient_collection;
pub mod transient_map;
pub mod transient_vector;
