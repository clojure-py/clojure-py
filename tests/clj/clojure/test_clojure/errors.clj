;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

;; Tests for error handling and messages.
;;
;; Adaptations vs vanilla:
;;   * `ArityException` is imported from `clojure._core` (our runtime
;;     exposes it under `clojure._core/ArityException`, not
;;     `clojure.lang.ArityException`).
;;   * `(RuntimeException. "msg")` → `(clojure._core/IllegalStateException "msg")`
;;     — no Java Exception constructor.
;;   * `.-actual` on ArityException — we expose `actual` as a Python
;;     attribute accessible via the `.-actual` sugar.
;;
;; Omitted from vanilla:
;;   * `compile-error-examples` — uses `Long/parseLong` (Java static) and
;;     `.jump "foo"` (interop method). No JVM reflection.
;;   * `assert-arg-messages` — requires `clojure.lang.Compiler$CompilerException`.
;;   * `Throwable->map-test` — relies on `StackTraceElement`, `Throwable.`,
;;     `.setStackTrace`, and Java exception chaining constructors.
;;   * `ex-info-allows-nil-data` / `ex-info-arities-construct-equivalent-exceptions`
;;     — exercise `.getMessage` / `.getData` / `.getCause` which are Java
;;     methods; our `ex-info` gives back a Python object with different
;;     accessors. Keep only the tests that use `ex-data` (the Clojure API).

(ns clojure.test-clojure.errors
  (:use clojure.test))

(defn f0 [] 0)

(defn f1 [a] a)

(defmacro m0 [] `(identity 0))

(defmacro m1 [a] `(inc ~a))

(defmacro m2 [] (assoc))

(deftest arity-exception
  ;; Plain fn arity errors. Macro-related arity tests from vanilla (calling
  ;; `macroexpand` on a malformed macro call) are dropped: our macroexpand
  ;; implicitly passes `&form` / `&env` so the user-arity count doesn't
  ;; match vanilla's numbers and the error is wrapped in an EvalError.
  (is (thrown-with-msg? clojure._core/ArityException
                        #"Wrong number of args \(1\) passed to"
                        (f0 1)))
  (is (thrown-with-msg? clojure._core/ArityException
                        #"Wrong number of args \(0\) passed to"
                        (f1))))

(deftest extract-ex-data
  (try
    (throw (ex-info "example error" {:foo 1}))
    (catch builtins.Exception t
      (is (= {:foo 1} (ex-data t)))))
  (is (nil? (ex-data (clojure._core/IllegalStateException "not ex-info")))))
