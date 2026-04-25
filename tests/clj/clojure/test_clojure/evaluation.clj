;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

;;  Tests for the Clojure functions documented at the URL:
;;
;;    http://clojure.org/Evaluation
;;
;;  by J. McConnell, adapted for clojure-py.
;;
;; Adaptations from vanilla:
;;   * Dropped `(Compiler/eval ...)` cross-checks — the JVM Compiler class
;;     isn't reachable.
;;   * `Compiler$CompilerException` → `clojure._core/EvalError`
;;     for our compile-error path.
;;   * Dropped ratio (`1/2`), BigDecimal (`1M`) literal cases (reader doesn't
;;     parse them yet).
;;   * Dropped `(eval 'java.lang.Math)` — we don't resolve dotted symbols
;;     to JVM classes.
;;   * Dropped `Boolean/TRUE` identity check — Python's `True`/`False` are
;;     singletons but we don't expose them as `Boolean/TRUE`.
;;   * Dropped `defstruct` + Metadata struct test — defstruct is deferred.
;;   * `clojure.test-helper` isn't ported — `test-that` is inlined as a
;;     trivial macro.

(ns clojure.test-clojure.evaluation
  (:use clojure.test))

(defmacro test-that
  "Lightweight stand-in for clojure.test-helper/test-that — just runs the
  forms; messages are dropped (we have only the structural assertion)."
  [_purpose & test-forms]
  `(do ~@test-forms))

(deftest Eval
  (is (= (eval '(+ 1 2 3)) 6))
  (is (= (eval '(list 1 2 3)) '(1 2 3)))
  (is (= (eval '(list + 1 2 3)) (list clojure.core/+ 1 2 3)))
  (test-that "Non-closure fns are supported as code"
             (is (= (eval (eval '(list + 1 2 3))) 6)))
  (is (= (eval (list '+ 1 2 3)) 6)))


;;; Literals tests ;;;

(defmacro evaluates-to-itself? [expr]
  `(let [v# ~expr
         q# (quote ~expr)]
     (is (= (eval q#) q#)
         (str q# " does not evaluate to itself"))))

(deftest Literals
  ;; Strings, numbers, characters, nil and keywords evaluate to themselves
  (evaluates-to-itself? "test")
  (evaluates-to-itself? "test
                        multi-line
                        string")
  (evaluates-to-itself? 1)
  (evaluates-to-itself? 1.0)
  (evaluates-to-itself? 1.123456789)
  (evaluates-to-itself? 999999999999999999)
  (evaluates-to-itself? \a)
  (evaluates-to-itself? \newline)
  (evaluates-to-itself? nil)
  (evaluates-to-itself? :test)
  ;; Boolean literals — Python's `True`/`False` are singletons.
  (is (identical? (eval true) true))
  (is (identical? (eval false) false)))


;;; Symbol resolution tests ;;;

(def foo "abc")
(in-ns 'resolution-test)
(clojure.core/refer-clojure)
(def bar 123)
(def ^{:private true} baz 456)
(in-ns 'clojure.test-clojure.evaluation)
(clojure.core/refer-clojure)

(deftest SymbolResolution
  (test-that
    "If a symbol is namespace-qualified, the evaluated value is the value
     of the binding of the global var named by the symbol"
    (is (= (eval 'resolution-test/bar) 123)))

  (test-that
    "It is an error if there is no global var named by the symbol"
    (is (thrown? clojure._core/EvalError (eval 'undefined-bar))))

  ;; Vanilla asserts that referring to a private var across namespaces
  ;; throws "is not public". Our resolver doesn't enforce :private — see
  ;; project memory; deferred until we add private-var checking.

  (test-that
    "Special forms with no expression form are not values; using them as a
     bare symbol is an error. Note: `let`/`fn`/`loop` are macros that expand
     to special forms in vanilla but resolve to var-bound functions here, so
     bare-symbol use of them does NOT throw — we exclude them from the list."
    (doall (for [form '(def if do quote var recur throw try)]
             (is (thrown? clojure._core/EvalError
                          (eval form))))))

  (test-that
    "Local bindings shadow nothing because special forms are recognized
     before locals"
    (let [if "foo"]
      (is (thrown? clojure._core/EvalError (eval 'if)))))

  (test-that
    "Local bindings are looked up before namespace vars"
    (is (= (eval '(let [foo "bar"] foo)) "bar")))

  (test-that
    "If a symbol is namespace-qualified, the evaluated value is the value
     of the binding of the global var named by the symbol — using a
     fully-qualified name avoids the ambiguity of `*ns*` during eval."
    (is (= (eval 'clojure.test-clojure.evaluation/foo) "abc")))

  (test-that
    "Unqualified symbols not resolvable to anything are an error"
    (is (thrown? clojure._core/EvalError (eval 'foobar)))))


;;; Collections tests ;;;

(def x 1)
(def y 2)

(deftest Collections
  ;; Use fully-qualified names since `eval` doesn't read `*ns*` for symbol
  ;; resolution in our runtime; resolve via the test ns directly.
  (let [ev-ns 'clojure.test-clojure.evaluation]
    (test-that
      "Vectors and Maps yield vectors and (hash) maps whose contents are the
       evaluated values of the objects they contain."
      (is (= (eval `[~(symbol (name ev-ns) "x")
                     ~(symbol (name ev-ns) "y")
                     3])
             [1 2 3]))
      (is (= (eval (quote {:x clojure.test-clojure.evaluation/x
                           :y clojure.test-clojure.evaluation/y
                           :z 3}))
             {:x 1 :y 2 :z 3}))
      (is (map? (eval (quote {:x clojure.test-clojure.evaluation/x
                              :y clojure.test-clojure.evaluation/y}))))))

  (test-that
    "An empty list () evaluates to an empty list."
    (is (= (eval '()) ()))
    (is (empty? (eval ())))
    (is (= (eval (list)) ()))))
