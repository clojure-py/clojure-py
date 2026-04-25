;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

;; Author: Stuart Halloway, Daniel Solano Gómez

;; Adaptations from vanilla:
;;   * Skipped tests built around `vector-of` (primitive-typed Vec) — that
;;     ctor is permanently deferred for clojure-py: `test-reversed-vec`,
;;     `test-vecseq`, `test-primitive-subvector-reduce`, `test-vec-creation`.
;;   * Skipped tests built around JVM-only APIs: `test-empty-vector-spliterator`,
;;     `test-spliterator-tryadvance-then-forEach`, `test-spliterator-trySplit`,
;;     `test-vector-parallel-stream` (java.util.stream / Spliterator), and
;;     the `(new java.util.ArrayList ...)` line in
;;     `test-vector-eqv-to-non-counted-types`.
;;   * `IndexOutOfBoundsException` (JVM) → Python's `IndexError`. We bind
;;     it as `builtins.IndexError` for `thrown?`.
;;   * `test-vec-associative` rewritten to use `contains?`/`find` (Clojure
;;     surface) instead of `.containsKey`/`.entryAt` (Java methods); same
;;     intent, no JVM interop.
;;   * Dropped the `(reify clojure.lang.IReduceInit ...)` case from
;;     `test-vec` — `reify` against a Java interface isn't a thing on Python.

(ns clojure.test-clojure.vectors
  (:use clojure.test))

(deftest empty-vector-equality
  (let [colls [[] '()]]
    (doseq [c1 colls, c2 colls]
      (is (= c1 c2)))))

(defn =vec
  [expected v] (and (vector? v) (= expected v)))

(deftest test-mapv
  (are [r c1] (=vec r (mapv + c1))
       [1 2 3] [1 2 3])
  (are [r c1 c2] (=vec r (mapv + c1 c2))
       [2 3 4] [1 2 3] (repeat 1))
  (are [r c1 c2 c3] (=vec r (mapv + c1 c2 c3))
       [3 4 5] [1 2 3] (repeat 1) (repeat 1))
  (are [r c1 c2 c3 c4] (=vec r (mapv + c1 c2 c3 c4))
       [4 5 6] [1 2 3] [1 1 1] [1 1 1] [1 1 1]))

(deftest test-filterv
  (are [r c1] (=vec r (filterv even? c1))
       [] [1 3 5]
       [2 4] [1 2 3 4 5]))

(deftest test-subvec
  (let [v1 (vec (range 100))
        v2 (subvec v1 50 57)]
    (is (thrown? builtins.IndexError (v2 -1)))
    (is (thrown? builtins.IndexError (v2 7)))
    (is (= (v1 50) (v2 0)))
    (is (= (v1 56) (v2 6)))))

(deftest test-vec
  (is (= [1 2] (vec (first {1 2}))))
  (is (= [0 1 2 3] (vec [0 1 2 3])))
  (is (= [0 1 2 3] (vec (list 0 1 2 3))))
  (is (= [0 1 2 3] (vec (sorted-set 0 1 2 3))))
  (is (= [[1 2] [3 4]] (vec (sorted-map 1 2 3 4))))
  (is (= [0 1 2 3] (vec (range 4))))
  (is (= [\a \b \c \d] (vec "abcd")))
  (is (= [0 1 2 3] (vec (object-array (range 4)))))
  (is (= [1 2 3 4] (vec (eduction (map inc) (range 4))))))

(deftest test-reduce-kv-vectors
  (is (= 25 (reduce-kv + 10 [2 4 6])))
  (is (= 25 (reduce-kv + 10 (subvec [0 2 4 6] 1)))))

(deftest test-vector-eqv-to-non-counted-types
  (is (not= (range) [0 1 2]))
  (is (not= [0 1 2] (range)))
  (is (= [0 1 2] (take 3 (range))))
  (is (not= [1 2] (take 1 (cycle [1 2]))))
  (is (= [1 2 3 nil 4 5 6 nil] (eduction cat [[1 2 3 nil] [4 5 6 nil]]))))

(deftest test-vec-associative
  ;; Adapted: vanilla uses `.containsKey` / `.entryAt` Java methods on
  ;; PersistentVector. We exercise the same semantics through the Clojure
  ;; surface: `contains?` (index in-range?) and `find` (return MapEntry
  ;; or nil).
  (let [empty-v []
        v       (vec (range 1 6))]
    (testing "contains?"
      (are [x] (contains? v x)
           0 1 2 3 4)
      (are [x] (not (contains? v x))
           -1 -100 nil "" 5 100)
      (are [x] (not (contains? empty-v x))
           0 1))
    (testing "find returns [idx val]"
      (are [idx val] (= [idx val] (find v idx))
           0 1
           2 3
           4 5)
      (are [idx] (nil? (find v idx))
           -5 -1 5 10 nil "")
      (are [idx] (nil? (find empty-v idx))
           0 1))))

;; Portable subset of vanilla's test-vec-compare: exercises compare on plain
;; PersistentVector (the vector-of / primitive-typed Vec rows remain deferred).
(deftest test-vec-compare
  (let [nums         (range 1 100)
        rand-replace (fn [val]
                       (let [r (rand-int 99)]
                         (concat (take r nums) [val] (drop (inc r) nums))))
        num-seqs     {:standard       nums
                      :empty          '()
                      :longer         (concat nums [100])
                      :shorter        (drop-last nums)
                      :first-greater  (concat [100] (next nums))
                      :last-greater   (concat (drop-last nums) [100])
                      :rand-greater-1 (rand-replace 100)
                      :rand-greater-2 (rand-replace 100)
                      :rand-greater-3 (rand-replace 100)
                      :first-lesser   (concat [0] (next nums))
                      :last-lesser    (concat (drop-last nums) [0])
                      :rand-lesser-1  (rand-replace 0)
                      :rand-lesser-2  (rand-replace 0)
                      :rand-lesser-3  (rand-replace 0)}
        vecs         (zipmap (keys num-seqs)
                             (map #(into [] %1) (vals num-seqs)))
        std          (:standard vecs)]
    (testing "compare"
      (testing "identical"
        (is (= 0 (compare std std))))
      (testing "equivalent"
        (are [x y] (= 0 (compare x y))
             std (:standard vecs)
             (:standard vecs) std
             (:empty vecs) (:empty vecs)))
      (testing "lesser"
        (are [x] (= -1 (compare std x))
             (:longer vecs)
             (:first-greater vecs)
             (:last-greater vecs)
             (:rand-greater-1 vecs)
             (:rand-greater-2 vecs)
             (:rand-greater-3 vecs))
        (are [x] (= -1 (compare x std))
             nil
             (:empty vecs)
             (:shorter vecs)
             (:first-lesser vecs)
             (:last-lesser vecs)
             (:rand-lesser-1 vecs)
             (:rand-lesser-2 vecs)
             (:rand-lesser-3 vecs)))
      (testing "greater"
        (are [x] (= 1 (compare std x))
             nil
             (:empty vecs)
             (:shorter vecs)
             (:first-lesser vecs)
             (:last-lesser vecs)
             (:rand-lesser-1 vecs)
             (:rand-lesser-2 vecs)
             (:rand-lesser-3 vecs))
        (are [x] (= 1 (compare x std))
             (:longer vecs)
             (:first-greater vecs)
             (:last-greater vecs)
             (:rand-greater-1 vecs)
             (:rand-greater-2 vecs)
             (:rand-greater-3 vecs))))))
