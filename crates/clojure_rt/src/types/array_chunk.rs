//! `ArrayChunk` — a fixed-size block of `Value`s, the standard
//! `IChunk` implementation. Held by chunked seqs; consumed by
//! reduce-style ops that want to iterate 32 elements at a pop.
//!
//! Internal storage is `Arc<ArrayChunkInner>`, shared by all chunks
//! that derive from the same source via `drop_first`. Each chunk
//! tracks its own active range `[start, end)` into the shared array,
//! so `drop_first` is O(1) — just `start + 1` and a fresh wrapper.
//! The Arc's manual `Drop` decrefs each contained `Value` exactly
//! once (when the last sharing chunk goes away), independent of how
//! many sub-chunks were created from it.

use std::sync::Arc;

use crate::protocols::chunk::IChunk;
use crate::protocols::counted::ICounted;
use crate::protocols::indexed::IIndexed;
use crate::value::Value;

pub(crate) struct ArrayChunkInner {
    arr: Box<[Value]>,
}

impl Drop for ArrayChunkInner {
    fn drop(&mut self) {
        for v in self.arr.iter() {
            crate::rc::drop_value(*v);
        }
    }
}

clojure_rt_macros::register_type! {
    pub struct ArrayChunk {
        data:  Arc<ArrayChunkInner>,
        start: i32,
        end:   i32,
    }
}

impl ArrayChunk {
    /// Build a chunk from a `Vec<Value>`. Each element's refcount is
    /// already +1 owned by the Vec; the chunk takes ownership.
    pub fn from_vec(vals: Vec<Value>) -> Value {
        let end = vals.len() as i32;
        let inner = Arc::new(ArrayChunkInner { arr: vals.into_boxed_slice() });
        ArrayChunk::alloc(inner, 0, end)
    }

    /// Number of remaining elements in this chunk (`end - start`).
    pub fn count_of(this: Value) -> i32 {
        let body = unsafe { ArrayChunk::body(this) };
        body.end - body.start
    }

    /// Read element `i` (0-indexed within the chunk's active range).
    /// Caller is responsible for bounds discipline; this is the
    /// fast-path read used by chunk consumers.
    pub fn nth_of(this: Value, i: i32) -> Value {
        let body = unsafe { ArrayChunk::body(this) };
        debug_assert!(i >= 0 && i < body.end - body.start, "ArrayChunk::nth_of OOB");
        let v = body.data.arr[(body.start + i) as usize];
        crate::rc::dup(v);
        v
    }
}

clojure_rt_macros::implements! {
    impl ICounted for ArrayChunk {
        fn count(this: Value) -> Value {
            Value::int(ArrayChunk::count_of(this) as i64)
        }
    }
}

clojure_rt_macros::implements! {
    impl IIndexed for ArrayChunk {
        fn nth_2(this: Value, n: Value) -> Value {
            let Some(i) = n.as_int() else {
                return crate::exception::make_foreign(
                    format!("nth: index must be an integer, got tag {}", n.tag),
                );
            };
            let cnt = ArrayChunk::count_of(this) as i64;
            if i < 0 || i >= cnt {
                return crate::exception::make_foreign(format!(
                    "Index {} out of bounds for chunk of size {}", i, cnt
                ));
            }
            ArrayChunk::nth_of(this, i as i32)
        }
        fn nth_3(this: Value, n: Value, not_found: Value) -> Value {
            let Some(i) = n.as_int() else {
                crate::rc::dup(not_found);
                return not_found;
            };
            let cnt = ArrayChunk::count_of(this) as i64;
            if i < 0 || i >= cnt {
                crate::rc::dup(not_found);
                return not_found;
            }
            ArrayChunk::nth_of(this, i as i32)
        }
    }
}

clojure_rt_macros::implements! {
    impl IChunk for ArrayChunk {
        fn drop_first(this: Value) -> Value {
            let body = unsafe { ArrayChunk::body(this) };
            debug_assert!(body.start < body.end, "ArrayChunk::drop_first on empty chunk");
            ArrayChunk::alloc(body.data.clone(), body.start + 1, body.end)
        }
    }
}
