;; Call / dispatch overhead microbenchmarks.

(ns bench.dispatch)

;; ---------- Baseline: direct fn call ----------

(defn direct-call [n]
  (let [f (fn [x] (inc x))]
    (loop [i 0 acc 0]
      (if (< i n) (recur (inc i) (f acc)) acc))))

;; ---------- Protocol dispatch ----------

(defprotocol BenchProto
  (bench-op [x]))

(deftype MonoType  []  BenchProto (bench-op [_] 1))
(deftype PolyA     []  BenchProto (bench-op [_] 1))
(deftype PolyB     []  BenchProto (bench-op [_] 2))
(deftype PolyC     []  BenchProto (bench-op [_] 3))
(deftype PolyD     []  BenchProto (bench-op [_] 4))
(deftype PolyE     []  BenchProto (bench-op [_] 5))

(def ^:private mono-inst (->MonoType))
(def ^:private poly-ring
  (cycle [(->PolyA) (->PolyB) (->PolyC) (->PolyD) (->PolyE)]))

(defn proto-mono
  "Call the protocol method N times against a single type — should hit
  the method cache's fast path every time."
  [n]
  (loop [i 0 s 0]
    (if (< i n) (recur (inc i) (+ s (bench-op mono-inst))) s)))

(defn proto-poly
  "Call the protocol method N times rotating through 5 types — tests
  cache stability / eviction."
  [n]
  (loop [i 0 s 0 xs poly-ring]
    (if (< i n)
      (recur (inc i) (+ s (bench-op (first xs))) (rest xs))
      s)))

;; ---------- Keyword-as-IFn ----------

(def ^:private M {:a 1 :b 2 :c 3 :d 4 :e 5})

(defn kw-as-fn [n]
  (loop [i 0 s 0]
    (if (< i n) (recur (inc i) (+ s (:a M))) s)))

;; ---------- Var deref / call ----------

(defn a-plain-fn [x] (inc x))

(defn var-call
  "Call through a plain (non-dynamic) Var n times. Tests `invoke` on Var."
  [n]
  (loop [i 0 acc 0]
    (if (< i n) (recur (inc i) (a-plain-fn acc)) acc)))

(def ^:dynamic *d-fn* (fn [x] (inc x)))

(defn dyn-var-call
  "Call through a *dynamic* Var with an active `binding` — tests the
  binding-stack lookup on each invocation."
  [n]
  (binding [*d-fn* (fn [x] (inc x))]
    (loop [i 0 acc 0]
      (if (< i n) (recur (inc i) (*d-fn* acc)) acc))))

;; ---------- Multimethod dispatch ----------

(defmulti mm-op :kind)
(defmethod mm-op :a [m] 1)
(defmethod mm-op :b [m] 2)
(defmethod mm-op :c [m] 3)
(defmethod mm-op :d [m] 4)
(defmethod mm-op :e [m] 5)

(def ^:private mm-inst {:kind :a})
(def ^:private mm-ring (cycle [{:kind :a} {:kind :b} {:kind :c} {:kind :d} {:kind :e}]))

(defn mm-mono [n]
  (loop [i 0 s 0]
    (if (< i n) (recur (inc i) (+ s (mm-op mm-inst))) s)))

(defn mm-poly [n]
  (loop [i 0 s 0 xs mm-ring]
    (if (< i n)
      (recur (inc i) (+ s (mm-op (first xs))) (rest xs))
      s)))

(def benchmarks
  {"dispatch/direct-call-100k"     (fn [] (direct-call 100000))
   "dispatch/proto-mono-100k"      (fn [] (proto-mono 100000))
   "dispatch/proto-poly-100k"      (fn [] (proto-poly 100000))
   "dispatch/kw-as-fn-100k"        (fn [] (kw-as-fn 100000))
   "dispatch/var-call-100k"        (fn [] (var-call 100000))
   "dispatch/dyn-var-call-100k"    (fn [] (dyn-var-call 100000))
   "dispatch/mm-mono-100k"         (fn [] (mm-mono 100000))
   "dispatch/mm-poly-100k"         (fn [] (mm-poly 100000))})
