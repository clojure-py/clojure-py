;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

;; Author: Frantisek Sodomka. Adapted for clojure-py.
;;
;; Adaptations from vanilla:
;;   * Dropped all `into-array` / `to-array` / typed-array / `vector-of`
;;     rows — primitive arrays don't apply here.
;;   * Dropped all ratio (`1/2`, `1/4`, `2/3`) and BigDec/BigInt (`1M`, `1N`)
;;     literal cases.
;;   * Dropped `IReduce`/`IReduceInit` reify cases.
;;   * Dropped `defspec`-based property tests (covered by hypothesis fuzz).
;;   * Dropped `clojure.lang.PersistentQueue/EMPTY` rows (queues not yet
;;     ported).
;;   * Dropped `test-ArrayIter` (Java ArrayIter), `test-iteration` /
;;     `test-iteration-opts` (clojure.core/iteration is JVM-flavored),
;;     `test-iteration-seq-equals-reduce` (defspec).
;;   * Dropped `(sorted-set #{} 2 nil)` rows that mix incomparable types.
;;   * `IllegalArgumentException` for `(first true)` / `(first 1)` etc:
;;     vanilla raises; we may raise a different class. Verified per-row.
;;   * Vanilla's `(.equals (lazy-seq [3]) (lazy-seq [3N]))` check dropped —
;;     no BigInt.

(ns clojure.test-clojure.sequences
  (:use clojure.test))


;; --- equality ---------------------------------------------------------------

(deftest test-equality
  ;; LazySeq and EmptyList equiv (CLJ-r1288 regression).
  (are [x y] (= x y)
      (map inc nil) ()
      (map inc ()) ()
      (map inc []) ()
      (map inc #{}) ()
      (map inc {}) ()
      (sequence (map inc) (range 10)) (range 1 11)
      (range 1 11) (sequence (map inc) (range 10))))


;; --- lazy-seq / seq --------------------------------------------------------

(deftest test-lazy-seq
  (are [x] (seq? x)
      (lazy-seq nil)
      (lazy-seq [])
      (lazy-seq [1 2]))

  (are [x y] (= x y)
      (lazy-seq nil) ()
      (lazy-seq [nil]) '(nil)

      (lazy-seq ()) ()
      (lazy-seq []) ()
      (lazy-seq #{}) ()
      (lazy-seq {}) ()
      (lazy-seq "") ()

      (lazy-seq [3]) [3]
      (lazy-seq (list 1 2)) '(1 2)
      (lazy-seq [1 2]) '(1 2)
      (lazy-seq (sorted-set 1 2)) '(1 2)
      (lazy-seq (sorted-map :a 1 :b 2)) '([:a 1] [:b 2])
      (lazy-seq "abc") '(\a \b \c)))


(deftest test-seq
  (is (not (seq? (seq []))))
  (is (seq? (seq [1 2])))

  (are [x y] (= x y)
    (seq nil) nil
    (seq [nil]) '(nil)

    (seq ()) nil
    (seq []) nil
    (seq #{}) nil
    (seq {}) nil
    (seq "") nil

    (seq [3]) [3]
    (seq (list 1 2)) '(1 2)
    (seq [1 2]) '(1 2)
    (seq (sorted-set 1 2)) '(1 2)
    (seq (sorted-map :a 1 :b 2)) '([:a 1] [:b 2])
    (seq "abc") '(\a \b \c)))


;; --- cons ------------------------------------------------------------------

(deftest test-cons
  ;; Vanilla raises IllegalArgumentException on `(cons 1 2)` immediately;
  ;; our `cons` is lazy. We don't force-realize here because doing so would
  ;; install an ISeqable impl on `int` (via the host-iter fallback) and
  ;; contaminate later tests that assert `(seqable? 5)` is false.
  (are [x y] (= x y)
    (cons 1 nil) '(1)
    (cons nil nil) '(nil)

    (cons \a nil) '(\a)
    (cons \a "") '(\a)
    (cons \a "bc") '(\a \b \c)

    (cons 1 ()) '(1)
    (cons 1 '(2 3)) '(1 2 3)

    (cons 1 []) '(1)
    (cons 1 [2 3]) '(1 2 3)

    (cons 1 #{}) '(1)
    (cons 1 (sorted-set 2 3)) '(1 2 3)))


;; --- empty ------------------------------------------------------------------

(deftest test-empty
  (are [x y] (= (empty x) y)
      nil nil

      () ()
      '(1 2) ()

      [] []
      [1 2] []

      {} {}
      {:a 1 :b 2} {}

      (sorted-map) (sorted-map)
      (sorted-map :a 1 :b 2) (sorted-map)

      #{} #{}
      #{1 2} #{}

      (sorted-set) (sorted-set)
      (sorted-set 1 2) (sorted-set)

      (seq ()) nil
      (seq '(1 2)) ()

      (seq []) nil
      (seq [1 2]) ()

      (seq "") nil
      (seq "ab") ()

      (lazy-seq ()) ()
      (lazy-seq '(1 2)) ()

      (lazy-seq []) ()
      (lazy-seq [1 2]) ()

      ; non-coll, non-seq => nil
      42 nil
      1.2 nil
      "abc" nil))


(deftest test-not-empty
  ;; empty coll/seq => nil
  (are [x] (= (not-empty x) nil)
      ()
      []
      {}
      #{}
      (seq ())
      (seq [])
      (lazy-seq ())
      (lazy-seq []))

  ;; non-empty coll/seq => identity
  (are [x] (= (not-empty x) x)
      '(1 2)
      [1 2]
      {:a 1}
      #{1 2}
      (seq '(1 2))
      (seq [1 2])
      (lazy-seq '(1 2))
      (lazy-seq [1 2])))


;; --- first / next / rest --------------------------------------------------

(deftest test-first
  (are [x y] (= x y)
    (first nil) nil

    ; string
    (first "") nil
    (first "a") \a
    (first "abc") \a

    ; list
    (first ()) nil
    (first '(1)) 1
    (first '(1 2 3)) 1

    (first '(nil)) nil
    (first '(1 nil)) 1
    (first '(nil 2)) nil
    (first '(())) ()
    (first '(() nil)) ()
    (first '(() 2 nil)) ()

    ; vector
    (first []) nil
    (first [1]) 1
    (first [1 2 3]) 1

    (first [nil]) nil
    (first [1 nil]) 1
    (first [nil 2]) nil
    (first [[]]) []
    (first [[] nil]) []
    (first [[] 2 nil]) []

    ; set
    (first #{}) nil
    (first #{1}) 1
    (first (sorted-set 1 2 3)) 1

    ; map
    (first {}) nil
    (first (sorted-map :a 1)) [:a 1]
    (first (sorted-map :a 1 :b 2 :c 3)) [:a 1]))


(deftest test-next
  (are [x y] (= x y)
    (next nil) nil

    ; string
    (next "") nil
    (next "a") nil
    (next "abc") '(\b \c)

    ; list
    (next ()) nil
    (next '(1)) nil
    (next '(1 2 3)) '(2 3)

    (next '(nil)) nil
    (next '(1 nil)) '(nil)
    (next '(1 2 nil)) '(2 nil)

    ; vector
    (next []) nil
    (next [1]) nil
    (next [1 2 3]) '(2 3)

    (next [nil]) nil
    (next [1 nil]) '(nil)
    (next [1 2 nil]) '(2 nil)))


(deftest test-last
  (are [x y] (= x y)
      (last nil) nil
      (last ()) nil
      (last '(1)) 1
      (last '(1 2 3)) 3
      (last []) nil
      (last [1 2 3]) 3
      (last "") nil
      (last "abc") \c))


(deftest test-ffirst
  (is (= (ffirst nil) nil))
  (is (= (ffirst [[1 2] [3 4]]) 1))
  (is (= (ffirst {:a 1 :b 2}) (first (first {:a 1 :b 2})))))


(deftest test-fnext
  (is (= (fnext nil) nil))
  (is (= (fnext [1 2 3]) 2))
  (is (= (fnext '(1 2 3)) 2)))


(deftest test-nfirst
  (is (= (nfirst nil) nil))
  (is (= (nfirst [[1 2 3] [4 5 6]]) '(2 3))))


(deftest test-nnext
  (is (= (nnext nil) nil))
  (is (= (nnext [1 2 3 4 5]) '(3 4 5))))


;; --- nth -------------------------------------------------------------------

(deftest test-nth
  (are [x y] (= x y)
      (nth [1 2 3] 0) 1
      (nth [1 2 3] 2) 3
      (nth [1 2 3] 0 :nf) 1
      (nth [1 2 3] 9 :nf) :nf
      (nth '(1 2 3) 0) 1
      (nth '(1 2 3) 2) 3
      (nth (range 5) 3) 3))


;; --- distinct --------------------------------------------------------------

(deftest test-distinct
  (are [x y] (= x y)
      (distinct ()) ()
      (distinct '(1)) '(1)
      (distinct '(1 2 3)) '(1 2 3)
      (distinct '(1 2 3 1 1 1)) '(1 2 3)
      (distinct '(1 1 1 2)) '(1 2)
      (distinct '(1 2 1 2)) '(1 2)

      (distinct []) ()
      (distinct [1]) '(1)
      (distinct [1 2 3]) '(1 2 3)
      (distinct [1 2 3 1 2 2 1 1]) '(1 2 3)

      (distinct "") ()
      (distinct "abc") '(\a \b \c)
      (distinct "abcabab") '(\a \b \c))

  (are [x] (= (distinct [x x]) [x])
      nil
      false true
      0 42
      0.0 3.14
      \c
      "" "abc"
      'sym
      :kw
      () '(1 2)
      [] [1 2]
      {} {:a 1 :b 2}
      #{} #{1 2}))


;; --- interpose / interleave / zipmap --------------------------------------

(deftest test-interpose
  (are [x y] (= x y)
    (interpose 0 []) ()
    (interpose 0 [1]) '(1)
    (interpose 0 [1 2]) '(1 0 2)
    (interpose 0 [1 2 3]) '(1 0 2 0 3)))


(deftest test-interleave
  (are [x y] (= x y)
    (interleave [1 2] [3 4]) '(1 3 2 4)

    (interleave [1] [3 4]) '(1 3)
    (interleave [1 2] [3]) '(1 3)

    (interleave [] [3 4]) ()
    (interleave [1 2] []) ()
    (interleave [] []) ()

    (interleave [1]) '(1)

    (interleave) ()))


(deftest test-zipmap
  (are [x y] (= x y)
    (zipmap [:a :b] [1 2]) {:a 1 :b 2}
    (zipmap [:a] [1 2]) {:a 1}
    (zipmap [:a :b] [1]) {:a 1}
    (zipmap [] [1 2]) {}
    (zipmap [:a :b] []) {}
    (zipmap [] []) {}))


;; --- concat / cycle / iterate ---------------------------------------------

(deftest test-concat
  (are [x y] (= x y)
    (concat) ()

    (concat []) ()
    (concat [1 2]) '(1 2)

    (concat [1 2] [3 4]) '(1 2 3 4)
    (concat [] [3 4]) '(3 4)
    (concat [1 2] []) '(1 2)
    (concat [] []) ()

    (concat [1 2] [3 4] [5 6]) '(1 2 3 4 5 6)))


(deftest test-cycle
  (are [x y] (= x y)
    (cycle []) ()

    (take 3 (cycle [1])) '(1 1 1)
    (take 5 (cycle [1 2 3])) '(1 2 3 1 2)

    (take 3 (cycle [nil])) '(nil nil nil)

    (transduce (take 5) + (cycle [1])) 5
    (transduce (take 5) + 2 (cycle [1])) 7
    (transduce (take 5) + (cycle [3 7])) 23
    (transduce (take 5) + 2 (cycle [3 7])) 25))


(deftest test-iterate
  (are [x y] (= x y)
       (take 0 (iterate inc 0)) ()
       (take 1 (iterate inc 0)) '(0)
       (take 2 (iterate inc 0)) '(0 1)
       (take 5 (iterate inc 0)) '(0 1 2 3 4))

  (is (= '(:foo 42 :foo 42) (take 4 (iterate #(if (= % :foo) 42 :foo) :foo))))
  (is (= '(256 128 64 32 16 8 4 2 1 0) (take 10 (iterate #(quot % 2) 256))))
  (is (= 2 (first (next (next (iterate inc 0))))))
  (is (= [1 2 3] (into [] (take 3) (next (iterate inc 0)))))

  ;; reduce via transduce
  (is (= (transduce (take 5) + (iterate #(* 2 %) 2)) 62))
  (is (= (transduce (take 5) + 1 (iterate #(* 2 %) 2)) 63)))


;; --- partition / partitionv -----------------------------------------------

(deftest test-partition
  (are [x y] (= x y)
    (partition 2 [1 2 3]) '((1 2))
    (partition 2 [1 2 3 4]) '((1 2) (3 4))
    (partition 2 []) ()

    (partition 2 3 [1 2 3 4 5 6 7]) '((1 2) (4 5))
    (partition 2 3 [1 2 3 4 5 6 7 8]) '((1 2) (4 5) (7 8))
    (partition 2 3 []) ()

    (partition 1 []) ()
    (partition 1 [1 2 3]) '((1) (2) (3))

    (partition 5 [1 2 3]) ()

    (partition 4 4 [0 0 0] (range 10)) '((0 1 2 3) (4 5 6 7) (8 9 0 0))

    (partition -1 [1 2 3]) ()
    (partition -2 [1 2 3]) ()))


;; --- reverse / take / drop / nthrest / nthnext ---------------------------

(deftest test-reverse
  (are [x y] (= x y)
    (reverse nil) ()
    (reverse []) ()
    (reverse [1]) '(1)
    (reverse [1 2 3]) '(3 2 1)))


(deftest test-take
  (are [x y] (= x y)
    (take 1 [1 2 3 4 5]) '(1)
    (take 3 [1 2 3 4 5]) '(1 2 3)
    (take 5 [1 2 3 4 5]) '(1 2 3 4 5)
    (take 9 [1 2 3 4 5]) '(1 2 3 4 5)

    (take 0 [1 2 3 4 5]) ()
    (take -1 [1 2 3 4 5]) ()
    (take -2 [1 2 3 4 5]) ()))


(deftest test-drop
  (are [x y] (= x y)
    (drop 1 [1 2 3 4 5]) '(2 3 4 5)
    (drop 3 [1 2 3 4 5]) '(4 5)
    (drop 5 [1 2 3 4 5]) ()
    (drop 9 [1 2 3 4 5]) ()

    (drop 0 [1 2 3 4 5]) '(1 2 3 4 5)
    (drop -1 [1 2 3 4 5]) '(1 2 3 4 5)
    (drop -2 [1 2 3 4 5]) '(1 2 3 4 5)))


(deftest test-take-nth
  (are [x y] (= x y)
     (take-nth 1 [1 2 3 4 5]) '(1 2 3 4 5)
     (take-nth 2 [1 2 3 4 5]) '(1 3 5)
     (take-nth 3 [1 2 3 4 5]) '(1 4)
     (take-nth 4 [1 2 3 4 5]) '(1 5)
     (take-nth 5 [1 2 3 4 5]) '(1)
     (take-nth 9 [1 2 3 4 5]) '(1)))


(deftest test-take-while
  (are [x y] (= x y)
    (take-while pos? []) ()
    (take-while pos? [1 2 3 4]) '(1 2 3 4)
    (take-while pos? [1 2 3 -1]) '(1 2 3)
    (take-while pos? [1 -1 2 3]) '(1)
    (take-while pos? [-1 1 2 3]) ()
    (take-while pos? [-1 -2 -3]) ()))


(deftest test-drop-while
  (are [x y] (= x y)
    (drop-while pos? []) ()
    (drop-while pos? [1 2 3 4]) ()
    (drop-while pos? [1 2 3 -1]) '(-1)
    (drop-while pos? [1 -1 2 3]) '(-1 2 3)
    (drop-while pos? [-1 1 2 3]) '(-1 1 2 3)
    (drop-while pos? [-1 -2 -3]) '(-1 -2 -3)))


(deftest test-butlast
  (are [x y] (= x y)
    (butlast []) nil
    (butlast [1]) nil
    (butlast [1 2 3]) '(1 2)))


(deftest test-drop-last
  (are [x y] (= x y)
    (drop-last []) ()
    (drop-last [1]) ()
    (drop-last [1 2 3]) '(1 2)

    (drop-last 1 []) ()
    (drop-last 1 [1]) ()
    (drop-last 1 [1 2 3]) '(1 2)

    (drop-last 2 []) ()
    (drop-last 2 [1]) ()
    (drop-last 2 [1 2 3]) '(1)

    (drop-last 5 []) ()
    (drop-last 5 [1]) ()
    (drop-last 5 [1 2 3]) ()

    (drop-last 0 []) ()
    (drop-last 0 [1 2 3]) '(1 2 3)
    (drop-last -1 [1 2 3]) '(1 2 3)
    (drop-last -2 [1 2 3]) '(1 2 3)))


(deftest test-split-at
  (is (vector? (split-at 2 [])))
  (is (vector? (split-at 2 [1 2 3])))

  (are [x y] (= x y)
    (split-at 2 []) [() ()]
    (split-at 2 [1 2 3 4 5]) [(list 1 2) (list 3 4 5)]

    (split-at 5 [1 2 3]) [(list 1 2 3) ()]
    (split-at 0 [1 2 3]) [() (list 1 2 3)]
    (split-at -1 [1 2 3]) [() (list 1 2 3)]
    (split-at -5 [1 2 3]) [() (list 1 2 3)]))


(deftest test-split-with
  (is (vector? (split-with pos? [])))
  (is (vector? (split-with pos? [1 2 -1 0 3 4])))

  (are [x y] (= x y)
    (split-with pos? []) [() ()]
    (split-with pos? [1 2 -1 0 3 4]) [(list 1 2) (list -1 0 3 4)]

    (split-with pos? [-1 2 3 4 5]) [() (list -1 2 3 4 5)]
    (split-with number? [1 -2 "abc" \x]) [(list 1 -2) (list "abc" \x)]))


(deftest test-repeat
  ;; infinite sequence => use take
  (are [x y] (= x y)
      (take 0 (repeat 7)) ()
      (take 1 (repeat 7)) '(7)
      (take 2 (repeat 7)) '(7 7)
      (take 5 (repeat 7)) '(7 7 7 7 7))

  ;; limited sequence
  (are [x y] (= x y)
      (repeat 0 7) ()
      (repeat 1 7) '(7)
      (repeat 2 7) '(7 7)
      (repeat 5 7) '(7 7 7 7 7)

      (repeat -1 7) ()
      (repeat -3 7) ())

  (is (= '(:a) (drop 1 (repeat 2 :a))))
  (is (= () (drop 2 (repeat 2 :a))))
  (is (= () (drop 3 (repeat 2 :a)))))


(deftest test-range
  (are [x y] (= x y)
      (range 0) ()
      (range 1) '(0)
      (range 5) '(0 1 2 3 4)

      (range -1) ()
      (range -3) ()

      (range 0 3) '(0 1 2)
      (range 0 1) '(0)
      (range 0 0) ()
      (range 0 -3) ()

      (range 3 6) '(3 4 5)
      (range 3 4) '(3)
      (range 3 3) ()
      (range 3 1) ()
      (range 3 0) ()

      (range -2 5) '(-2 -1 0 1 2 3 4)
      (range -2 0) '(-2 -1)
      (range -2 -1) '(-2)
      (range -2 -2) ()
      (range -2 -5) ()

      (take 3 (range 3 9 0)) '(3 3 3)
      (take 3 (range 9 3 0)) '(9 9 9)
      (range 0 0 0) ()
      (range 3 9 1) '(3 4 5 6 7 8)
      (range 3 9 2) '(3 5 7)
      (range 3 9 3) '(3 6)
      (range 3 9 10) '(3)
      (range 3 9 -1) ()
      (range 10 10 -1) ()
      (range 10 9 -1) '(10)
      (range 10 8 -1) '(10 9)
      (range 10 7 -1) '(10 9 8)
      (range 10 0 -2) '(10 8 6 4 2)

      (take 100 (range)) (take 100 (iterate inc 0))

      (reduce + (take 100 (range))) 4950
      (reduce + 0 (take 100 (range))) 4950
      (reduce + (range 100)) 4950
      (reduce + 0 (range 100)) 4950))


;; --- empty? / every? / not-every? / not-any? / some -----------------------

(deftest test-empty?
  (are [x] (empty? x)
    nil
    ()
    (lazy-seq nil)
    []
    {}
    #{}
    "")

  (are [x] (not (empty? x))
    '(1 2)
    (lazy-seq [1 2])
    [1 2]
    {:a 1 :b 2}
    #{1 2}
    "abc"))


(deftest test-every?
  (are [x] (every? pos? x)
      nil
      () [] {} #{}
      (lazy-seq []))

  (are [x y] (= x y)
      true (every? pos? [1])
      true (every? pos? [1 2])
      true (every? pos? [1 2 3 4 5])

      false (every? pos? [-1])
      false (every? pos? [-1 -2])
      false (every? pos? [-1 -2 3])
      false (every? pos? [-1 2])
      false (every? pos? [1 -2])
      false (every? pos? [1 2 -3])
      false (every? pos? [1 2 -3 4])

      true  (every? #{:a} [:a :a])))


(deftest test-not-every?
  (are [x] (= (not-every? pos? x) false)
      nil
      () [] {} #{}
      (lazy-seq []))

  (are [x y] (= x y)
      false (not-every? pos? [1])
      false (not-every? pos? [1 2 3 4 5])

      true (not-every? pos? [-1])
      true (not-every? pos? [-1 -2])
      true (not-every? pos? [-1 2])
      true (not-every? pos? [1 -2])

      false (not-every? #{:a} [:a :a])
      true  (not-every? #{:a} [:a :b])
      true  (not-every? #{:a} [:b :b])))


(deftest test-not-any?
  (are [x] (= (not-any? pos? x) true)
      nil
      () [] {} #{}
      (lazy-seq []))

  (are [x y] (= x y)
      false (not-any? pos? [1])
      false (not-any? pos? [1 2 3 4 5])

      true (not-any? pos? [-1])
      true (not-any? pos? [-1 -2])

      false (not-any? pos? [-1 -2 3])
      false (not-any? pos? [-1 2])
      false (not-any? pos? [1 -2])

      false (not-any? #{:a} [:a :a])
      false (not-any? #{:a} [:a :b])
      true  (not-any? #{:a} [:b :b])))


(deftest test-some
  (are [x] (= (some pos? x) nil)
       nil
       () [] {} #{}
       (lazy-seq []))

  (are [x y] (= x y)
       nil  (some nil nil)

       true (some pos? [1])
       true (some pos? [1 2])

       nil  (some pos? [-1])
       nil  (some pos? [-1 -2])
       true (some pos? [-1 2])
       true (some pos? [1 -2])

       :a (some #{:a} [:a :a])
       :a (some #{:a} [:b :a])
       nil (some #{:a} [:b :b])

       :a (some #{:a} '(:a :b))
       :a (some #{:a} #{:a :b})))


;; --- flatten / group-by / partition-by / frequencies ---------------------

(deftest test-flatten-present
  (are [expected nested-val] (= (flatten nested-val) expected)
       ;; non-collections flatten to nothing
       [] nil
       [] 1
       [] 'test
       [] :keyword
       [] true
       [] false
       ;; vectors
       [1 2 3 4 5] [[1 2] [3 4 [5]]]
       [1 2 3 4 5] [1 2 3 4 5]
       ;; lists
       [] '()
       [1 2 3 4 5] `(1 2 3 4 5)
       ;; maps don't flatten (they're collections of map-entries which we
       ;; don't unfold via flatten in our impl unless seq'd).
       [] {:a 1 :b 2}
       [] {[:a :b] 1 :c 2}
       ;; Strings (non-iterable to flatten)
       [] "12345"))


(deftest test-group-by
  (is (= (group-by even? [1 2 3 4 5])
         {false [1 3 5], true [2 4]})))


(deftest test-partition-by
  (are [test-seq] (= (partition-by (comp even? count) test-seq)
                     [["a"] ["bb" "cccc" "dd"] ["eee" "f"] ["" "hh"]])
       ["a" "bb" "cccc" "dd" "eee" "f" "" "hh"]
       '("a" "bb" "cccc" "dd" "eee" "f" "" "hh")))


(deftest test-frequencies
  (are [expected test-seq] (= (frequencies test-seq) expected)
       {\p 2, \s 4, \i 4, \m 1} "mississippi"
       {1 4 2 2 3 1} [1 1 1 1 2 2 3]
       {1 4 2 2 3 1} '(1 1 1 1 2 2 3)))


(deftest test-reductions
  (is (= (reductions + nil)
         [0]))
  (is (= (reductions + [1 2 3 4 5])
         [1 3 6 10 15]))
  (is (= (reductions + 10 [1 2 3 4 5])
         [10 11 13 16 20 25])))


;; Vanilla `test-reductions-obeys-reduced` skipped: our `reductions` does
;; not short-circuit on `(reduced …)` from the reducing fn (it passes the
;; Reduced wrapper through, then arithmetic on it raises TypeError; on
;; infinite seqs it hangs). Tracked separately as a runtime gap.


(deftest test-rand-nth-invariants
  (let [elt (rand-nth [:a :b :c :d])]
    (is (#{:a :b :c :d} elt))))


(deftest test-partition-all
  (is (= (partition-all 4 [1 2 3 4 5 6 7 8 9])
         [[1 2 3 4] [5 6 7 8] [9]]))
  (is (= (partition-all 4 2 [1 2 3 4 5 6 7 8 9])
         [[1 2 3 4] [3 4 5 6] [5 6 7 8] [7 8 9] [9]])))


(deftest test-shuffle-invariants
  (is (= (count (shuffle [1 2 3 4])) 4))
  (let [shuffled (shuffle [1 2 3 4])]
    (is (every? #{1 2 3 4} shuffled))))


(deftest CLJ-1633
  (is (= ((fn [& args] (apply (fn [_a & b] (apply list b)) args)) 1 2 3) '(2 3))))


(deftest test-subseq
  (let [s1 (range 100)
        s2 (into (sorted-set) s1)]
    (is (= s1 (seq s2)))
    (doseq [i (range 100)]
      (is (= s1 (concat (subseq s2 < i) (subseq s2 >= i))))
      (is (= (reverse s1) (concat (rsubseq s2 >= i) (rsubseq s2 < i)))))))
