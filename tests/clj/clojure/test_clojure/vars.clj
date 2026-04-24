;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.

; Author: Frantisek Sodomka, Stephen C. Gilardi

;; Adaptations from vanilla:
;;   * `(Exception. "msg")` → `(clojure._core/IllegalStateException "msg")`
;;     — no Java Exception constructor here.
;;   * `(thrown? Exception …)` → `(thrown? builtins.Exception …)`.
;;   * `test-with-redefs-fn` / `test-with-redefs` — vanilla spawns a
;;     `Thread` to observe the redef; we just call directly. The semantic
;;     being tested (root restoration) doesn't require the thread.
;;
;; Omissions:
;;   * `test-with-precision` / `test-settable-math-context` — require
;;     BigDecimal (`3.5555555M`) + `*math-context*`; not ported.
;;   * `test-vars-apply-lazily` — requires lazy `apply` on an infinite seq;
;;     our `apply` currently realizes.

(ns clojure.test-clojure.vars
  (:use clojure.test))

; http://clojure.org/vars

; def
; defn defn- defonce

; declare intern binding find-var var

(def ^:dynamic a)
(deftest test-binding
  (are [x y] (= x y)
      (eval `(binding [a 4] a)) 4     ; regression in Clojure SVN r1370
  ))

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

; set-validator get-validator

; doc find-doc test

(def stub-me :original)

(deftest test-with-redefs-fn
  (let [p (promise)]
    (with-redefs-fn {#'stub-me :temp}
      (fn []
        (deliver p stub-me)))
    (is (= :temp @p))
    (is (= :original stub-me))))

(deftest test-with-redefs
  (let [p (promise)]
    (with-redefs [stub-me :temp]
      (deliver p stub-me))
    (is (= :temp @p))
    (is (= :original stub-me))))

(deftest test-with-redefs-throw
  (let [p (promise)]
    (is (thrown? builtins.Exception
      (with-redefs [stub-me :temp]
        (deliver p stub-me)
        (throw (clojure._core/IllegalStateException "simulated failure in with-redefs")))))
    (is (= :temp @p))
    (is (= :original stub-me))))

(def ^:dynamic dynamic-var 1)

(deftest test-with-redefs-inside-binding
  (binding [dynamic-var 2]
    (is (= 2 dynamic-var))
    (with-redefs [dynamic-var 3]
      (is (= 2 dynamic-var))))
  (is (= 1 dynamic-var)))
