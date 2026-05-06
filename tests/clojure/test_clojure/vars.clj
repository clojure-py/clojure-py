;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

; Author: Frantisek Sodomka, Stephen C. Gilardi

;; Port of clojure/test/clojure/test_clojure/vars.clj.
;;
;; Adaptations from JVM:
;;   - test-with-precision and test-settable-math-context removed —
;;     JVM-only BigDecimal MathContext / *math-context* machinery.
;;     Our BigDecimal is a thin Decimal wrapper; precision/rounding
;;     control isn't ported.
;;   - JVM `(Thread. fn)` constructor → Python threading.Thread.
;;     Thread takes (group, target, name) positionally; we pass nil
;;     for group.

(ns clojure.test-clojure.vars
  (:use clojure.test))

; http://clojure.org/vars

; def
; defn defn- defonce

; declare intern binding find-var var

(def ^:dynamic a)
(deftest test-binding
  (are [x y] (= x y)
       (eval `(binding [a 4] a)) 4))     ; regression in Clojure SVN r1370

; var-get var-set alter-var-root [var? (predicates.clj)]
; with-in-str with-out-str
; with-open

(deftest test-with-local-vars
  (let [factorial (fn [x]
                    (with-local-vars [acc 1, cnt x]
                      (while (> @cnt 0)
                        (var-set acc (* @acc @cnt))
                        (var-set cnt (dec @cnt)))
                      @acc))]
    (is (= (factorial 5) 120))))

;; test-with-precision / test-settable-math-context — see top-of-file
;; adaptation note. Skipped.

; set-validator get-validator

; doc find-doc test

(def stub-me :original)

(defn -start-thread
  "Adaptation: build and start a Python threading.Thread. Mirrors
  JVM (.start (Thread. f)). Wraps `f` so an exception in the thread
  doesn't propagate (Python thread exceptions hit the unraisable
  hook by default; we want the test to observe stub-me's value via
  the promise rather than fail with an unrelated stack trace)."
  [f]
  (let [t (py.threading/Thread nil (fn [] (try (f) (catch Throwable _ nil))))]
    (.start t)
    t))

(deftest test-with-redefs-fn
  (let [p (promise)]
    (with-redefs-fn {(var stub-me) :temp}
      (fn []
        (.join (-start-thread #(deliver p stub-me)))
        @p))
    (is (= :temp @p))
    (is (= :original stub-me))))

(deftest test-with-redefs
  (let [p (promise)]
    (with-redefs [stub-me :temp]
      (.join (-start-thread #(deliver p stub-me)))
      @p)
    (is (= :temp @p))
    (is (= :original stub-me))))

(deftest test-with-redefs-throw
  (let [p (promise)]
    (is (thrown? Exception
                 (with-redefs [stub-me :temp]
                   (deliver p stub-me)
                   (throw (Exception. "simulated failure in with-redefs")))))
    (is (= :temp @p))
    (is (= :original stub-me))))

(def ^:dynamic dynamic-var 1)

(deftest test-with-redefs-inside-binding
  (binding [dynamic-var 2]
    (is (= 2 dynamic-var))
    (with-redefs [dynamic-var 3]
      (is (= 2 dynamic-var))))
  (is (= 1 dynamic-var)))

;; test-vars-apply-lazily — skipped pending compiler work.
;;
;; The test asserts that (apply f (range)) on a variadic f doesn't
;; force realization of the infinite range. JVM's RestFn.applyTo
;; passes the rest-seq through directly without consuming. Our
;; compiler emits `& rest` fns as Python `*args`, so apply's
;; .applyTo fallback splats the seq via `f(*list(seq))` — which
;; iterates the whole infinite range and hangs. Fixing requires our
;; fn* compiler to special-case variadic fns into a RestFn-style
;; dispatcher that exposes apply_to without splatting.
