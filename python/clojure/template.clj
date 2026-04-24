;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

;;; clojure.template — port of Stuart Sierra's clojure/template.clj.
;;; Vanilla depends on clojure.walk for postwalk-replace. clojure.walk isn't
;;; ported yet, so we inline the minimal postwalk-replace needed here.

(ns clojure.template)

(defn- postwalk-replace
  "Walk `form` depth-first; replace any subform `x` with `(smap x)` when
  `(contains? smap x)`. Preserves collection types (list/vector/map/set).
  Private to this ns — swap for clojure.walk/postwalk-replace once ported."
  [smap form]
  (let [walk (fn walk [x]
               (let [x' (cond
                          (list? x)    (apply list (map walk x))
                          (vector? x)  (into [] (map walk x))
                          (map? x)     (into {} (map (fn [[k v]] [(walk k) (walk v)]) x))
                          (set? x)     (into #{} (map walk x))
                          (seq? x)     (doall (map walk x))
                          :else        x)]
                 (if (contains? smap x')
                   (smap x')
                   x')))]
    (walk form)))

(defn apply-template
  "For use in macros. `argv` is an argument list, as in defn. `expr` is
  a quoted expression using the symbols in argv. `values` is a sequence
  of values to be used for the arguments.

  apply-template will recursively replace argument symbols in expr
  with their corresponding values, returning a modified expr.

  Example: (apply-template '[x] '(+ x x) '[2])
           ;=> (+ 2 2)"
  [argv expr values]
  (assert (vector? argv))
  (assert (every? symbol? argv))
  (postwalk-replace (zipmap argv values) expr))

(defmacro do-template
  "Repeatedly copies expr (in a do block) for each group of arguments
  in values. values are automatically partitioned by the number of
  arguments in argv, an argument vector as in defn.

  Example: (macroexpand '(do-template [x y] (+ y x) 2 4 3 5))
           ;=> (do (+ 4 2) (+ 5 3))"
  [argv expr & values]
  (let [c (count argv)]
    `(do ~@(map (fn [a] (apply-template argv expr a))
                (partition c values)))))
