;; Inner-loop / VM-dispatch microbenchmarks.
;;
;; Each bench is a 0-arg thunk that the Python runner times end-to-end.
;; Pick N so one call is ~1-10ms — timer noise dominates below ~100µs.

(ns bench.inner-loop)

(defn loop-recur-sum
  "Classic tight integer loop via loop/recur. Tests the VM's recur path."
  [n]
  (loop [i 0 acc 0]
    (if (< i n)
      (recur (inc i) (+ acc i))
      acc)))

(defn reduce-sum
  "(reduce + (range n)) — exercises range seq + IFn invocation per element."
  [n]
  (reduce + 0 (range n)))

(defn transduce-sum
  "Two-stage xform pipeline on (range n). Stresses transducer composition."
  [n]
  (transduce (comp (filter even?) (map inc)) + 0 (range n)))

(defn nested-dotimes
  "Nested dotimes — m outer × n inner — mutating an atom. Stresses
  `swap!` on the hot path along with the dotimes macroexpansion."
  [m n]
  (let [a (atom 0)]
    (dotimes [_ m]
      (dotimes [_ n]
        (swap! a inc)))
    @a))

(defn doseq-walk
  "Walk a vector via doseq, incrementing an atom. Tests vector-seq
  traversal (chunked path) through doseq."
  [v]
  (let [a (atom 0)]
    (doseq [_ v] (swap! a inc))
    @a))

(defn fn-call-tight
  "Hot call-overhead loop: invoke a trivial 1-arg fn n times."
  [n]
  (let [f (fn [x] (inc x))]
    (loop [i 0 acc 0]
      (if (< i n)
        (recur (inc i) (f acc))
        acc))))

(def ^:private V100k (vec (range 100000)))

(def benchmarks
  {"inner/loop-recur-sum-100k"   (fn [] (loop-recur-sum 100000))
   "inner/reduce-sum-100k"       (fn [] (reduce-sum 100000))
   "inner/transduce-sum-100k"    (fn [] (transduce-sum 100000))
   "inner/nested-dotimes-200-1k" (fn [] (nested-dotimes 200 1000))
   "inner/doseq-vec-100k"        (fn [] (doseq-walk V100k))
   "inner/fn-call-tight-100k"    (fn [] (fn-call-tight 100000))})
