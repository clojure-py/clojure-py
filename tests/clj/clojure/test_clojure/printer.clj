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
;;   * Dropped `print-throwable` (uses `Throwable->map` and JVM-only
;;     exception chaining).
;;   * Dropped `print-dup` cases involving `1N`, `1M`, `BigInteger.` —
;;     reader doesn't parse those literals.
;;   * Dropped `pprint`-driven tests in `print-ns-maps` — `clojure.pprint`
;;     isn't ported.
;;   * Kept the portable subset: var printing and symbolic-value (`##Inf`,
;;     `##-Inf`, `##NaN`) printing.

(ns clojure.test-clojure.printer
  (:use clojure.test)
  (:require [clojure.string :as str]))

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


(deftest print-length-empty-seq
  (let [coll () val "()"]
    (is (= val (binding [*print-length* 0] (print-str coll))))
    (is (= val (binding [*print-length* 1] (print-str coll))))))

(deftest print-length-seq
  (let [coll (range 5)
        length-val '((0 "(...)")
                     (1 "(0 ...)")
                     (2 "(0 1 ...)")
                     (3 "(0 1 2 ...)")
                     (4 "(0 1 2 3 ...)")
                     (5 "(0 1 2 3 4)"))]
    (doseq [[length val] length-val]
      (binding [*print-length* length]
        (is (= val (print-str coll)))))))

(deftest print-length-empty-vec
  (let [coll [] val "[]"]
    (is (= val (binding [*print-length* 0] (print-str coll))))
    (is (= val (binding [*print-length* 1] (print-str coll))))))

(deftest print-length-vec
  (let [coll [0 1 2 3 4]
        length-val '((0 "[...]")
                     (1 "[0 ...]")
                     (2 "[0 1 ...]")
                     (3 "[0 1 2 ...]")
                     (4 "[0 1 2 3 ...]")
                     (5 "[0 1 2 3 4]"))]
    (doseq [[length val] length-val]
      (binding [*print-length* length]
        (is (= val (print-str coll)))))))

(deftest print-level-seq
  (let [coll '(0 (1 (2 (3 (4)))))
        level-val '((0 "#")
                    (1 "(0 #)")
                    (2 "(0 (1 #))")
                    (3 "(0 (1 (2 #)))")
                    (4 "(0 (1 (2 (3 #))))")
                    (5 "(0 (1 (2 (3 (4)))))"))]
    (doseq [[level val] level-val]
      (binding [*print-level* level]
        (is (= val (print-str coll)))))))

(deftest print-level-length-coll
  (let [coll '(if (member x y) (+ (first x) 3) (foo (a b c d "Baz")))
        level-length-val
        '((0 1 "#")
          (1 1 "(if ...)")
          (1 2 "(if # ...)")
          (1 3 "(if # # ...)")
          (1 4 "(if # # #)")
          (2 1 "(if ...)")
          (2 2 "(if (member x ...) ...)")
          (2 3 "(if (member x y) (+ # 3) ...)")
          (3 2 "(if (member x ...) ...)")
          (3 3 "(if (member x y) (+ (first x) 3) ...)")
          (3 4 "(if (member x y) (+ (first x) 3) (foo (a b c d ...)))")
          (3 5 "(if (member x y) (+ (first x) 3) (foo (a b c d Baz)))"))]
    (doseq [[level length val] level-length-val]
      (binding [*print-level* level
                *print-length* length]
        (is (= val (print-str coll)))))))

(deftest print-meta
  ;; When *print-meta* is true, pr-str of a var should start with "^" and
  ;; end with the var's name string, containing the meta map in between.
  (are [x s] (binding [*print-meta* true]
               (let [pstr (pr-str x)]
                 (and (str/ends-with? pstr s)
                      (str/starts-with? pstr "^")
                      (str/includes? pstr (pr-str (meta x))))))
       #'pr-str        "#'clojure.core/pr-str"
       #'var-with-meta "#'clojure.test-clojure.printer/var-with-meta"
       #'var-with-type "#'clojure.test-clojure.printer/var-with-type"))

(deftest print-namespace-maps
  ;; pr-str rows only — pprint rows dropped (clojure.pprint not ported).
  ;; The mixed keyword+symbol key row ({:user/a 1, 'user/b 2}) is dropped:
  ;; hash-map iteration order differs from JVM, so the namespace-map
  ;; compression shortcut for mixed-type-key maps is not portable here.
  (are [m s-on s-off]
    (and (= s-on  (binding [*print-namespace-maps* true]  (pr-str m)))
         (= s-off (binding [*print-namespace-maps* false] (pr-str m))))
    {} "{}" "{}"
    {:a 1, :b 2} "{:a 1, :b 2}" "{:a 1, :b 2}"
    {:user/a 1} "#:user{:a 1}" "{:user/a 1}"
    {:user/a 1, :user/b 2} "#:user{:a 1, :b 2}" "{:user/a 1, :user/b 2}"
    {:user/a 1, :b 2} "{:user/a 1, :b 2}" "{:user/a 1, :b 2}"
    {:user/a 1, :foo/b 2} "{:user/a 1, :foo/b 2}" "{:user/a 1, :foo/b 2}"

    {:user/a 1, :user/b 2, 100 200}
    "{:user/a 1, :user/b 2, 100 200}"
    "{:user/a 1, :user/b 2, 100 200}"

    ;; CLJ-2537
    {:x.y/a {:rem 0}, :x.y/b {:rem 1}}
    "#:x.y{:a {:rem 0}, :b {:rem 1}}"
    "{:x.y/a {:rem 0}, :x.y/b {:rem 1}}"))
