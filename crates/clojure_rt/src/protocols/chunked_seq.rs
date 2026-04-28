//! `IChunkedSeq` — seqs that vend whole `IChunk`s at a time. Consumers
//! that walk many elements (reduce, transduce, into-style ops) can
//! pull entire blocks rather than allocating one seq cell per
//! element. Element-by-element walks via `ISeq::first`/`rest` still
//! work — the chunked API is an optional accelerator.

clojure_rt_macros::protocol! {
    pub trait IChunkedSeq {
        fn chunked_first(this: ::clojure_rt::Value) -> ::clojure_rt::Value;
        fn chunked_rest(this: ::clojure_rt::Value)  -> ::clojure_rt::Value;
        fn chunked_next(this: ::clojure_rt::Value)  -> ::clojure_rt::Value;
    }
}
