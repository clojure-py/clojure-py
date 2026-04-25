;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

;; Author: Stephen C. Gilardi
;; Adapted for clojure-py.
;;
;; Adaptations from vanilla:
;;   * Dropped tests using `*print-length*`, `*print-level*`, `*print-meta*`,
;;     `*print-namespace-maps*` — these dynamic vars aren't honored by our
;;     printer yet (tracked separately).
;;   * Dropped `print-throwable` (uses `Throwable->map` and JVM-only
;;     exception chaining).
;;   * Dropped `print-dup` cases involving `1N`, `1M`, `BigInteger.` —
;;     reader doesn't parse those literals.
;;   * Dropped `pprint`-driven tests in `print-ns-maps` — `clojure.pprint`
;;     isn't ported.
;;   * Kept the portable subset: var printing and symbolic-value (`##Inf`,
;;     `##-Inf`, `##NaN`) printing.

(ns clojure.test-clojure.printer
  (:use clojure.test))

(def ^{:foo :anything} var-with-meta 42)
(def ^{:type :anything} var-with-type 666)


(deftest print-var
  (are [x s] (= s (pr-str x))
       #'pr-str        "#'clojure.core/pr-str"
       #'var-with-meta "#'clojure.test-clojure.printer/var-with-meta"
       #'var-with-type "#'clojure.test-clojure.printer/var-with-type"))


(deftest print-symbol-values
  ;; `##Inf` / `##-Inf` / `##NaN` round-trip cleanly through pr-str.
  (are [s v] (= s (pr-str v))
       "##Inf"  ##Inf
       "##-Inf" ##-Inf
       "##NaN"  ##NaN))


(deftest print-basic-readable-forms
  ;; Spot check that common values pr-str into something `read-string` can
  ;; parse back into an `=` value (via the printer / reader pair).
  (are [v] (= v (read-string (pr-str v)))
       1
       1.0
       3.14
       "hi"
       :foo
       :ns/qual
       'sym
       'ns/qual-sym
       []
       [1 2 3]
       {}
       {:a 1 :b 2}
       #{}
       #{1 2 3}
       '()
       '(1 2 3)
       \a
       \space
       nil
       true
       false))


(deftest print-collections-with-strings
  ;; Strings inside collections must be quoted/escaped on pr-str.
  (is (= "[\"hi\"]"            (pr-str ["hi"])))
  (is (= "[\"a\\nb\"]"         (pr-str ["a\nb"])))
  (is (= "{:k \"v\"}"          (pr-str {:k "v"})))
  (is (= "#{\"x\"}"            (pr-str #{"x"})))
  (is (= "(\"a\" \"b\")"       (pr-str '("a" "b")))))


(deftest print-vs-pr-str
  ;; `print-str` (non-readable) leaves strings unquoted; chars print raw.
  (is (= "hello" (print-str "hello")))
  (is (= "\"hello\"" (pr-str "hello")))
  (is (= "a"     (print-str \a)))
  (is (= "\\a"   (pr-str \a))))
