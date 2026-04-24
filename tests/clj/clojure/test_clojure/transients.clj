;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.

;; Adaptations from vanilla:
;;   * `IllegalStateException` / `IllegalAccessError` → runtime maps both
;;     to our Clojure-side `clojure._core/IllegalStateException`. Our check
;;     uses one class for both "after persistent!" and misc transient misuse.
;;   * `#"regex"` literals work now — kept as vanilla.
;;   * `(.contains (transient #{}) k)` → `(contains? …)`. Transient sets
;;     expose `contains?` via Associative; there's no `.contains` method.
;;   * `(reify Object (hashCode [_] 42))` in `dissocing` and
;;     `persistent-assoc-on-collision` — those tests manufacture hash
;;     collisions via Java-Object hashCode overrides. Our `reify` doesn't
;;     have a Java-Object parent; tests skipped.

(ns clojure.test-clojure.transients
  (:use clojure.test))

(deftest popping-off
  (testing "across a node boundary"
    (are [n]
      (let [v (-> (range n) vec)]
        (= (subvec v 0 (- n 2)) (-> v transient pop! pop! persistent!)))
      33 (+ 32 (inc (* 32 32))) (+ 32 (inc (* 32 32 32)))))
  (testing "off the end"
    (is (thrown-with-msg? clojure._core/IllegalStateException
                          #"Can't pop empty vector"
                          (-> [] transient pop!))))
  (testing "copying array from a non-editable when put in tail position")
    (is (= 31 (let [pv (vec (range 34))]
                (-> pv transient pop! pop! pop! (conj! 42))
                (nth pv 31)))))

;; Skipped vanilla `dissocing` — requires `(reify Object (hashCode [_] …))`
;; to manufacture hash collisions.

(deftest test-disj!
  (testing "disjoin multiple items in one call"
    (is (= #{5 20} (-> #{5 10 15 20} transient (disj! 10 15) persistent!)))))

(deftest empty-transient
  (is (= false (contains? (transient #{}) :bogus-key))))

;; Skipped vanilla `persistent-assoc-on-collision` — same reify-Object reason.

(deftest transient-mod-after-persistent
  (let [v [1 2 3]
        t (transient v)
        t2 (conj! t 4)
        p (persistent! t2)]
    (is (= [1 2 3 4] p))
    (is (thrown? clojure._core/IllegalStateException (conj! t2 5)))))

(deftest transient-mod-ok-across-threads
  (let [v [1 2 3]
        t (transient v)
        t2 @(future (conj! t 4))
        p (persistent! t2)]
    (is (= [1 2 3 4] p))))

(deftest transient-lookups
  (let [tv (transient [1 2 3])]
    (is (= 1 (get tv 0)))
    (is (= :foo (get tv 4 :foo)))
    (is (= true (contains? tv 0)))
    (is (= [0 1] (find tv 0)))
    (is (= nil (find tv -1))))
  (let [ts (transient #{1 2})]
    (is (= true (contains? ts 1)))
    (is (= false (contains? ts 99)))
    (is (= 1 (get ts 1)))
    (is (= nil (get ts 99))))
  (let [tam (transient (array-map :a 1 :b 2))]
    (is (= true (contains? tam :a)))
    (is (= false (contains? tam :x)))
    (is (= 1 (get tam :a)))
    (is (= nil (get tam :x)))
    (is (= [:a 1] (find tam :a)))
    (is (= nil (find tam :x))))
  (let [thm (transient (hash-map :a 1 :b 2))]
    (is (= true (contains? thm :a)))
    (is (= false (contains? thm :x)))
    (is (= 1 (get thm :a)))
    (is (= nil (get thm :x)))
    (is (= [:a 1] (find thm :a)))
    (is (= nil (find thm :x)))))
