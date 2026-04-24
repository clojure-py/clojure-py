;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

;; Adaptations from vanilla:
;;   * `(.name *ns*)` → `(ns-name *ns*)` — ClojureNamespace uses `__name__`
;;     (Python convention); go through the Clojure accessor.
;;   * `IllegalArgumentException` → `builtins.Exception` for arity checks —
;;     our Keyword raises a Python-layer `TypeError` (not Clojure's
;;     `IllegalArgumentException`) on wrong-arity call. Exception TYPE
;;     differs; the fact that it throws is what the test checks.

(ns clojure.test-clojure.keywords
  (:use clojure.test))

(let [this-ns (str (ns-name *ns*))]
  (deftest test-find-keyword
    :foo
    ::foo
    (let [absent-keyword-sym (gensym "absent-keyword-sym")]
      (are [result lookup] (= result (find-keyword lookup))
           :foo :foo
           :foo 'foo
           :foo "foo"
           nil absent-keyword-sym
           nil (str absent-keyword-sym))
      (are [result lookup] (= result (find-keyword this-ns lookup))
           ::foo "foo"
           nil (str absent-keyword-sym)))))

(deftest arity-exceptions
  (is (thrown? builtins.Exception (:kw)))
  (is (thrown? builtins.Exception (apply :foo/bar (range 20))))
  (is (thrown? builtins.Exception (apply :foo/bar (range 21))))
  (is (thrown? builtins.Exception (apply :foo/bar (range 22)))))
