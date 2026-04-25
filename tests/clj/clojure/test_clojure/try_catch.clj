;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

;; Adapted from clojure/test/clojure/test_clojure/try_catch.clj.
;;
;; Vanilla's two tests use a JVM-only `ReflectorTryCatchFixture` Java class
;; to exercise checked-exception propagation through reflective method
;; dispatch. We have neither Java reflection nor checked exceptions, so we
;; port only the underlying property: `try`/`catch` inside `eval` must
;; propagate the exception thrown by the evaluated form.

(ns clojure.test-clojure.try-catch
  (:use clojure.test))

(defn- get-exception [expression]
  (try (eval expression)
       nil
       (catch builtins/Exception t
         t)))

(deftest catch-receives-exception-from-eval
  ;; A safe form returns nil — no exception.
  (is (nil? (get-exception "Eh, I'm pretty safe")))

  ;; Opening a non-existent file raises Python's FileNotFoundError, which our
  ;; runtime surfaces through `slurp`. The exception propagates out of eval,
  ;; gets caught, and we see it as a non-nil throwable.
  (is (some? (get-exception '(slurp "CAFEBABEx0/idonotexist")))))


(deftest catch-via-explicit-class
  ;; Vanilla's second deftest exercises Java reflection's exception
  ;; unwrapping. We don't reflect, but we can verify that `(throw x)` of
  ;; a specific exception type is caught by `(catch <Type> ...)`.
  (is (thrown-with-msg? clojure._core/IllegalStateException #"boom"
        (throw (clojure._core/IllegalStateException "boom"))))

  (is (thrown-with-msg? clojure._core/IllegalArgumentException #"bad"
        (throw (clojure._core/IllegalArgumentException "bad"))))

  ;; Catching a more specific class: throw the concrete IllegalStateException,
  ;; catch IllegalStateException — works because of exact match.
  (let [caught (try
                 (throw (clojure._core/IllegalStateException "x"))
                 (catch clojure._core/IllegalStateException e
                   (.-args e)))]
    (is (= "x" (first (vec caught))))))
