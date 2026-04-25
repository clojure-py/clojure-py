;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

;; Adapted from clojure/test/clojure/test_clojure/numbers.clj.
;;
;; Adaptations from vanilla:
;;   * Dropped all rows containing ratio literals (`1/2`, `2/3`, etc.) and
;;     BigDecimal/BigInt (`1M`, `1N`) — reader doesn't parse these.
;;   * Dropped JVM-only blocks: BigInteger-conversions, Coerced-BigDecimal,
;;     unchecked-cast-num-obj/prim/char, test-prim-with-matching-hint,
;;     test-arbitrary-precision-subtract, test-array-types, warn-on-boxed,
;;     unchecked-{inc,dec,negate,add,subtract,multiply}-overflow,
;;     test-divide-bigint-at-edge, test-multiply-longs-at-edge.
;;   * Dropped defspec-driven property tests (commutative/associative/
;;     distributive laws, addition-undoes-subtraction, quotient-and-
;;     remainder) — covered elsewhere by hypothesis fuzz suites.
;;   * `ArithmeticException` (Java) → `builtins/ZeroDivisionError` for
;;     divide-by-zero / mod-by-zero / etc.
;;   * `ClassCastException` (Java) for `(+ "ab" "cd")` → `builtins/Exception`.
;;   * `Double/NaN`, `Double/POSITIVE_INFINITY` etc. → `##NaN`, `##Inf`,
;;     `##-Inf` reader forms (now supported).
;;   * `(bigint x)` cases use Python `int` (we have arbitrary-precision ints
;;     by default, so the rows still verify the math).

(ns clojure.test-clojure.numbers
  (:use clojure.test))

(def DELTA 1e-9)


;; --- Equality semantics ----------------------------------------------------

(deftest equality-tests
  ;; `=` distinguishes int from float (vanilla "Long ≠ Double under =").
  (is (not (= 1 1.0)))
  (is (= 1 1))
  (is (= 1.0 1.0))
  ;; `==` (numeric equality) DOES bridge int/float.
  (is (== 1 1.0))
  (is (== 1 1))
  ;; Boolean is its own thing — never equal to int.
  (is (not (= true 1)))
  (is (not (= false 0))))


;; --- Arithmetic ------------------------------------------------------------

(deftest test-add
  (are [x y] (= x y)
      (+) 0
      (+ 1) 1
      (+ 1 2) 3
      (+ 1 2 3) 6

      (+ -1) -1
      (+ -1 -2) -3
      (+ -1 +2 -3) -2

      (+ 1 -1) 0
      (+ -1 1) 0)

  (are [x y] (< (abs (- x y)) DELTA)
      (+ 1.2) 1.2
      (+ 1.1 2.4) 3.5
      (+ 1.1 2.2 3.3) 6.6)

  ;; Adding strings is not concatenation in Clojure (vanilla raises
  ;; ClassCastException; we raise IllegalArgumentException).
  (is (thrown? clojure._core/IllegalArgumentException (+ "ab" "cd"))))


(deftest test-subtract
  (is (thrown? clojure._core/ArityException (-)))
  (are [x y] (= x y)
      (- 1) -1
      (- 1 2) -1
      (- 1 2 3) -4

      (- -2) 2
      (- 1 -2) 3
      (- 1 -2 -3) 6

      (- 1 1) 0
      (- -1 -1) 0)

  (are [x y] (< (abs (- x y)) DELTA)
      (- 1.2) -1.2
      (- 2.2 1.1) 1.1
      (- 6.6 2.2 1.1) 3.3))


(deftest test-multiply
  (are [x y] (= x y)
      (*) 1
      (* 2) 2
      (* 2 3) 6
      (* 2 3 4) 24

      (* -1) -1
      (* -1 -2) 2
      (* -1 -2 -3) -6
      (* -1 2 -3) 6

      (* 0) 0
      (* 0 0) 0
      (* 0 1 2 3) 0))


(deftest test-divide
  ;; Note: in clojure-py, `/` on two integers returns a float (Python `/`)
  ;; rather than a Ratio. Ratios are deferred — vanilla returns `1/2` for
  ;; `(/ 2)` but here we get `0.5`.
  (are [x y] (= x y)
      (/ 4 2) 2
      (/ 24 3 2) 4
      (/ 24 3 2 -1) -4

      (/ -4 -2) 2
      (/ -4 2) -2)

  (are [x y] (< (abs (- x y)) DELTA)
      (/ 4.5 3) 1.5
      (/ 4.5 3.0 3.0) 0.5)

  (is (thrown? builtins/ZeroDivisionError (/ 2 0))))


;; --- mod / rem / quot ------------------------------------------------------

(deftest test-mod
  (is (thrown? builtins/ZeroDivisionError (mod 9 0)))

  (are [x y] (= x y)
    (mod 4 2) 0
    (mod 3 2) 1
    (mod 6 4) 2
    (mod 0 5) 0

    (mod 4.0 2.0) 0.0
    (mod 4.5 2.0) 0.5

    ;; |num| > |div|, num != k*div — sign of result follows divisor (vanilla).
    (mod 42 5) 2
    (mod 42 -5) -3
    (mod -42 5) 3
    (mod -42 -5) -2

    ;; |num| > |div|, exact multiple
    (mod 9 3) 0
    (mod 9 -3) 0
    (mod -9 3) 0
    (mod -9 -3) 0

    ;; |num| < |div|
    (mod 2 5) 2
    (mod 2 -5) -3
    (mod -2 5) 3
    (mod -2 -5) -2

    ;; num = 0
    (mod 0 3) 0
    (mod 0 -3) 0

    ;; large args
    (mod 3216478362187432 432143214) 120355456))


(deftest test-rem
  (is (thrown? builtins/ZeroDivisionError (rem 9 0)))

  (are [x y] (= x y)
    (rem 4 2) 0
    (rem 3 2) 1
    (rem 6 4) 2
    (rem 0 5) 0

    (rem 4.0 2.0) 0.0
    (rem 4.5 2.0) 0.5

    ;; |num| > |div|, num != k*div — sign of result follows dividend.
    (rem 42 5) 2
    (rem 42 -5) 2
    (rem -42 5) -2
    (rem -42 -5) -2

    (rem 9 3) 0
    (rem -9 3) 0
    (rem 9 -3) 0
    (rem -9 -3) 0))


(deftest test-quot
  (is (thrown? builtins/ZeroDivisionError (quot 9 0)))

  (are [x y] (= x y)
    (quot 4 2) 2
    (quot 3 2) 1
    (quot 6 4) 1
    (quot 0 5) 0

    (quot 42 5) 8
    (quot 42 -5) -8
    (quot -42 5) -8
    (quot -42 -5) 8))


;; --- pos? / zero? / neg? ---------------------------------------------------

(deftest test-pos?-zero?-neg?
  (let [nums [[(int 4) (int 0) (int -4)]
              [(long 5) (long 0) (long -5)]
              [(float 7.0) (float 0.0) (float -7.0)]
              [(double 8.0) (double 0.0) (double -8.0)]]
        pred-result [[pos?  [true  false false]]
                     [zero? [false true  false]]
                     [neg?  [false false true]]]]
    (doseq [[pred expected] pred-result]
      (doseq [n nums]
        (is (= (vec (map pred n)) expected)
          (pr-str pred n))))))


;; --- even? / odd? ----------------------------------------------------------

(deftest test-even?
  (is (even? -4))
  (is (not (even? -3)))
  (is (even? 0))
  (is (not (even? 5)))
  (is (even? 8))
  (is (thrown? clojure._core/IllegalArgumentException (even? (double 10)))))

(deftest test-odd?
  (is (not (odd? -4)))
  (is (odd? -3))
  (is (not (odd? 0)))
  (is (odd? 5))
  (is (not (odd? 8)))
  (is (thrown? clojure._core/IllegalArgumentException (odd? (double 10)))))


;; --- bit-shift / bit ops --------------------------------------------------

(deftest test-bit-shift-left
  ;; Vanilla uses radix literals (`2r10`); our reader doesn't parse those, so
  ;; we use decimal equivalents.
  (are [x y] (= x y)
       2   (bit-shift-left 1 1)
       4   (bit-shift-left 1 2)
       8   (bit-shift-left 1 3)
       46  (bit-shift-left 23 1)        ; 23 = 0b10111
       46  (apply bit-shift-left [23 1])))


(deftest test-bit-shift-right
  (are [x y] (= x y)
       0   (bit-shift-right 1 1)
       2   (bit-shift-right 4 1)
       1   (bit-shift-right 4 2)
       0   (bit-shift-right 4 3)
       11  (bit-shift-right 23 1)       ; 23 → 11
       -1  (bit-shift-right -2 1)))


;; --- min / max ------------------------------------------------------------

(deftest test-min-max
  (testing "single value"
    (is (= 0.0 (min 0.0)))
    (is (= 0.0 (max 0.0))))
  (testing "two values"
    (is (= -1.0 (min 0.0 -1.0)))
    (is (= 0.0  (max 0.0 -1.0)))
    (is (= -1.0 (min -1.0 0.0)))
    (is (= 0.0  (max -1.0 0.0)))
    (is (= 0.0  (min 0.0 1.0)))
    (is (= 1.0  (max 0.0 1.0))))
  (testing "three values"
    (is (= -1.0 (min 0.0 1.0 -1.0)))
    (is (= 1.0  (max 0.0 1.0 -1.0)))
    (is (= -1.0 (min 0.0 -1.0 1.0)))
    (is (= 1.0  (max 0.0 -1.0 1.0)))
    (is (= -1.0 (min -1.0 1.0 0.0)))
    (is (= 1.0  (max -1.0 1.0 0.0)))))


;; --- abs -------------------------------------------------------------------

(deftest test-abs
  (are [in ex] (= ex (abs in))
    -1   1
    1    1
    -1.0 1.0
    -0.0 0.0
    ##-Inf ##Inf
    ##Inf  ##Inf)
  (is (NaN? (abs ##NaN))))


;; --- Comparisons -----------------------------------------------------------

(deftest comparisons
  (testing "<"
    (is (< 1 2 3))
    (is (not (< 1 2 2)))
    (is (< 1.0 2.0))
    (is (< 1 2.0))
    (is (< 1.0 2)))
  (testing "<="
    (is (<= 1 2 2 3))
    (is (not (<= 2 1)))
    (is (<= 1.0 1.0)))
  (testing ">"
    (is (> 3 2 1))
    (is (not (> 1 2)))
    (is (> 2.0 1.0)))
  (testing ">="
    (is (>= 3 2 2 1))
    (is (not (>= 1 2)))
    (is (>= 1.0 1.0))))


;; --- NaN comparisons (always false) ---------------------------------------

(deftest test-nan-comparison
  (is (false? (< 1000 ##NaN)))
  (is (false? (<= 1000 ##NaN)))
  (is (false? (> 1000 ##NaN)))
  (is (false? (>= 1000 ##NaN)))
  (is (false? (< ##NaN 1000)))
  (is (false? (> ##NaN 1000))))


(deftest test-nan-as-operand
  (testing "All numeric operations with NaN as an operand produce NaN as a result"
    (let [nan ##NaN]
      (are [x] (NaN? x)
          (+ nan 1)
          (+ nan 0)
          (+ nan 0.0)
          (+ 1 nan)
          (+ 0 nan)
          (+ 0.0 nan)
          (+ nan nan)
          (- nan 1)
          (- nan 0)
          (- nan 0.0)
          (- 1 nan)
          (- 0 nan)
          (- 0.0 nan)
          (- nan nan)
          (* nan 1)
          (* nan 0)
          (* nan 0.0)
          (* 1 nan)
          (* 0 nan)
          (* 0.0 nan)
          (* nan nan)
          (/ nan 1)
          (/ nan 0.0)
          (/ 1 nan)
          (/ 0.0 nan)
          (/ nan nan)))))
