;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(ns clojure.core.protocols)

;; Adapted from JVM clojure/core/protocols.clj.
;;
;; What changes from JVM:
;;   - JVM (.iterator coll) / (.hasNext it) / (.next it) → Python iter() /
;;     next(it, sentinel). Python iterators don't expose hasNext;
;;     we use the 2-arg next() form with a sentinel object as the
;;     end-of-iteration signal.
;;   - Skipped extensions: clojure.lang.StringSeq (no separate type;
;;     Python strings are seqable via the Object branch), and
;;     APersistentMap$KeySeq / ValSeq (those are separate JVM helper
;;     classes; our keys/vals return ordinary lazy seqs).
;;   - Skipped Iterable extension. JVM uses it as a fast path for any
;;     Java iterable; the Object branch (which calls seq-reduce →
;;     internal-reduce) handles Python iterables correctly via the
;;     existing IteratorSeq machinery, just slower than direct iter().
;;     We can revisit if profiling shows it matters.

(defprotocol CollReduce
  "Protocol for collection types that can implement reduce faster than
  first/next recursion. Called by clojure.core/reduce. Baseline
  implementation defined in terms of seq."
  (coll-reduce [coll f] [coll f val]))

(defprotocol InternalReduce
  "Protocol for concrete seq types that can reduce themselves
   faster than first/next recursion. Called by clojure.core/reduce."
  (internal-reduce [seq f start]))

;; Sentinel for next() default — distinguishes a real None value from
;; iterator exhaustion. We compare with `identical?` so subclasses /
;; equal-but-not-identical objects can never collide with it.
(def ^:private -iter-sentinel (py.__builtins__/object))

(defn- naive-seq-reduce
  "Reduces a seq, ignoring any opportunities to switch to a more
  specialized implementation."
  [s f val]
  (loop [s (seq s)
         val val]
    (if s
      (let [ret (f val (first s))]
        (if (reduced? ret)
          @ret
          (recur (next s) ret)))
      val)))

(defn- interface-or-naive-reduce
  "Reduces via IReduceInit if possible, else naively."
  [coll f val]
  (if (instance? clojure.lang.IReduceInit coll)
    (.reduce coll f val)
    (naive-seq-reduce coll f val)))

(defn- seq-reduce
  ([coll f]
   (if-let [s (seq coll)]
     (internal-reduce (next s) f (first s))
     (f)))
  ([coll f val]
   (let [s (seq coll)]
     (internal-reduce s f val))))

(defn iterator-reduce!
  "Reduces a Python iterator, mutating it as we go. Honors `reduced`."
  ([iter f]
   (let [v (py.__builtins__/next iter -iter-sentinel)]
     (if (identical? v -iter-sentinel)
       (f)
       (iterator-reduce! iter f v))))
  ([iter f val]
   (loop [ret val]
     (let [v (py.__builtins__/next iter -iter-sentinel)]
       (if (identical? v -iter-sentinel)
         ret
         (let [ret (f ret v)]
           (if (reduced? ret)
             @ret
             (recur ret))))))))

(defn- iter-reduce
  ([coll f]
   (iterator-reduce! (py.__builtins__/iter coll) f))
  ([coll f val]
   (iterator-reduce! (py.__builtins__/iter coll) f val)))

(extend-protocol CollReduce
  nil
  (coll-reduce
    ([coll f] (f))
    ([coll f val] val))

  clojure.core/Object
  (coll-reduce
    ([coll f] (seq-reduce coll f))
    ([coll f val] (seq-reduce coll f val)))

  clojure.lang.IReduceInit
  (coll-reduce
    ([coll f] (.reduce coll f))
    ([coll f val] (.reduce coll f val)))

  ;; ASeqs are iterable, but we still want the seq-reduce path because
  ;; their `seq` is identity and InternalReduce will pick the right
  ;; specialization (chunked vs naive).
  clojure.lang.ASeq
  (coll-reduce
    ([coll f] (seq-reduce coll f))
    ([coll f val] (seq-reduce coll f val)))

  clojure.lang.LazySeq
  (coll-reduce
    ([coll f] (seq-reduce coll f))
    ([coll f val] (seq-reduce coll f val)))

  ;; Vector's chunked seq is faster than its iter.
  clojure.lang.PersistentVector
  (coll-reduce
    ([coll f] (seq-reduce coll f))
    ([coll f val] (seq-reduce coll f val))))

(extend-protocol InternalReduce
  nil
  (internal-reduce
    [s f val]
    val)

  ;; Handles vectors and ranges via their chunked seq.
  clojure.lang.IChunkedSeq
  (internal-reduce
    [s f val]
    (if-let [s (seq s)]
      (if (chunked-seq? s)
        (let [ret (.reduce (chunk-first s) f val)]
          (if (reduced? ret)
            @ret
            (recur (chunk-next s)
                   f
                   ret)))
        (interface-or-naive-reduce s f val))
      val))

  clojure.core/Object
  (internal-reduce
    [s f val]
    (loop [cls (class s)
           s s
           f f
           val val]
      (if-let [s (seq s)]
        (if (identical? (class s) cls)
          (let [ret (f val (first s))]
            (if (reduced? ret)
              @ret
              (recur cls (next s) f ret)))
          (interface-or-naive-reduce s f val))
        val))))

(defprotocol IKVReduce
  "Protocol for concrete associative types that can reduce themselves
  via a function of key and val faster than first/next recursion over
  map entries. Called by clojure.core/reduce-kv, and has same
  semantics (just different arg order)."
  (kv-reduce [amap f init]))

(defprotocol Datafiable
  :extend-via-metadata true

  (datafy [o] "return a representation of o as data (default identity)"))

(extend-protocol Datafiable
  nil
  (datafy [_] nil)

  clojure.core/Object
  (datafy [x] x))

(defprotocol Navigable
  :extend-via-metadata true

  (nav [coll k v]
    "return (possibly transformed) v in the context of coll and k (a key/index or nil),
defaults to returning v."))

(extend-protocol Navigable
  clojure.core/Object
  (nav [_ _ x] x))
