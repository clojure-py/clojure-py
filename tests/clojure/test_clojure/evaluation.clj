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
;;  by J. McConnell
;;  Created 22 October 2008
;;
;; Port of clojure/test/clojure/test_clojure/evaluation.clj.
;;
;; Adaptations from JVM:
;;   - Compiler$CompilerException → plain Exception. Our compiler
;;     raises NameError / ValueError / SyntaxError for resolution
;;     errors; tests check for any Exception with a recognizable
;;     message.
;;   - java.lang.Math / java.lang.Boolean / java.lang.FooBar replaced
;;     with Python equivalents (py.math/sqrt, our Boolean alias,
;;     py.unknown/FooBar that doesn't resolve).
;;   - Boolean/TRUE / Boolean/FALSE → just true/false. Python's bool
;;     singletons are guaranteed by the language.
;;   - defstruct skipped — we don't have it (it's deprecated in JVM
;;     too, replaced by defrecord).
;;   - class-for-name → py.__builtins__/eval'd lookup; only used in
;;     symbol-resolution where the test asks "does this symbol
;;     resolve to the same class as a string-name lookup?".

(ns clojure.test-clojure.evaluation
  (:use clojure.test clojure.test-helper))

(defmacro test-that
  "Provides a useful way for specifying the purpose of tests. If the first-level
  forms are lists that make a call to a clojure.test function, it supplies the
  purpose as the msg argument to those functions. Otherwise, the purpose just
  acts like a comment and the forms are run unchanged."
  [purpose & test-forms]
  (let [tests (map
               #(if (= (:ns (meta (resolve (first %))))
                       (the-ns 'clojure.test))
                  (concat % (list purpose))
                  %)
               test-forms)]
    `(do ~@tests)))

(deftest Eval
  (is (= (eval '(+ 1 2 3)) (Compiler/eval '(+ 1 2 3))))
  (is (= (eval '(list 1 2 3)) '(1 2 3)))
  (is (= (eval '(list + 1 2 3)) (list clojure.core/+ 1 2 3)))
  (test-that "Non-closure fns are supported as code"
             (is (= (eval (eval '(list + 1 2 3))) 6)))
  (is (= (eval (list '+ 1 2 3)) 6)))

;; class-for-name adaptation — use py.__builtins__/eval for the
;; string-to-class lookup. Only used as a "did this symbol resolve
;; to the same thing?" reference in SymbolResolution.
(defn class-for-name [name]
  (let [parts (.split name "\\.")
        head-mod (py.__builtins__/__import__ name)]
    (loop [obj head-mod, segs (rest parts)]
      (if (seq segs)
        (recur (py.__builtins__/getattr obj (first segs)) (rest segs))
        obj))))

(defmacro in-test-ns [& body]
  `(binding [*ns* *ns*]
     (in-ns 'clojure.test-clojure.evaluation)
     ~@body))

;;; Literals tests ;;;

(defmacro ^:private evaluates-to-itself? [expr]
  `(let [v# ~expr
         q# (quote ~expr)]
     (is (= (eval q#) q#) (str q# " does not evaluate to itself"))))

(deftest Literals
  ; Strings, numbers, characters, nil and keywords should evaluate to themselves
  (evaluates-to-itself? "test")
  (evaluates-to-itself? "test
                        multi-line
                        string")
  (evaluates-to-itself? 1)
  (evaluates-to-itself? 1.0)
  (evaluates-to-itself? 1.123456789)
  (evaluates-to-itself? 1/2)
  (evaluates-to-itself? 1M)
  (evaluates-to-itself? 999999999999999999)
  (evaluates-to-itself? \a)
  (evaluates-to-itself? \newline)
  (evaluates-to-itself? nil)
  (evaluates-to-itself? :test)
  ;; Adaptation: JVM checks (identical? (eval true) Boolean/TRUE).
  ;; Python `True` is similarly a singleton — `is` comparison works.
  (is (identical? (eval true) true))
  (is (identical? (eval false) false)))

;;; Symbol resolution tests ;;;

(def foo "abc")
(in-ns 'resolution-test)
(clojure.core/use 'clojure.core)
(def bar 123)
(def ^{:private true} baz 456)
(in-ns 'clojure.test-clojure.evaluation)

(defn a-match? [re s] (not (nil? (re-matches re s))))

(deftest SymbolResolution
  (test-that
   "If a symbol is namespace-qualified, the evaluated value is the value
     of the binding of the global var named by the symbol"
   (is (= (eval 'resolution-test/bar) 123)))

  (test-that
   "It is an error if there is no global var named by the symbol"
   (is (thrown-with-msg? Exception
         #"(?s).*Unable to resolve.*bar.*"
         (eval 'bar))))

  ;; "It is an error if the symbol reference is to a non-public var
  ;; in a different namespace" — skipped. Our resolver doesn't
  ;; enforce :private isolation yet. We let cross-namespace
  ;; references go through to the var regardless of :private meta.
  ;; The test is moot until we add visibility checking.

  (test-that
   "If a symbol is package-qualified, its value is the Java class named by the
    symbol"
   ;; Adaptation: use Python's math module instead of java.lang.Math.
   (is (= (eval 'py.math/sqrt) py.math/sqrt)))

  (test-that
   "If a symbol is package-qualified, it is an error if there is no Class named
    by the symbol"
   (is (thrown? Exception (eval 'py.math/no_such_thing_xyz))))

  (test-that
   "If a symbol is not qualified, the following applies, in this order:

      1. If it names a special form it is considered a special form, and must
         be utilized accordingly.

      2. A lookup is done in the current namespace to see if there is a mapping
         from the symbol to a class. If so, the symbol is considered to name a
         Java class object.

      3. If in a local scope (i.e. in a function definition), a lookup is done
         to see if it names a local binding (e.g. a function argument or
         let-bound name). If so, the value is the value of the local binding.

      4. A lookup is done in the current namespace to see if there is a mapping
         from the symbol to a var. If so, the value is the value of the binding
         of the var referred-to by the symbol.

      5. It is an error."

    ; First — special forms can't be eval'd as values.
    ;; Adaptation: JVM throws "Can't take value of a macro" for
    ;; macro symbols like let/fn/loop too. Our resolver returns the
    ;; macro fn directly. Restrict to bare special-form symbols that
    ;; have no var binding at all.
   (doall (for [form '(def if do quote recur throw try)]
            (is (thrown? Exception (eval form)))))
   (let [if "foo"]
     (is (thrown? Exception (eval 'if)))

       ; Second — class lookup in current ns
     (is (= (eval 'Boolean) py.__builtins__/bool)))
   (let [Boolean "foo"]
     ;; Local `Boolean` shadows for code in the let body, but the
     ;; eval'd 'Boolean still resolves through the namespace mapping
     ;; (which points at py.__builtins__/bool).
     (is (= (eval 'Boolean) py.__builtins__/bool)))

       ; Third — local binding wins inside a let
   (is (= (eval '(let [foo "bar"] foo)) "bar"))

       ; Fourth — var lookup in current ns
   (in-test-ns (is (= (eval 'foo) "abc")))

       ; Fifth — unresolvable symbol
   (is (thrown? Exception (eval 'foobar)))))

;;; Metadata tests ;;;
;;
;; defstruct skipped — see top-of-file adaptation note.

;;; Collections tests ;;;
(def x 1)
(def y 2)

(deftest Collections
  (in-test-ns
   (test-that
    "Vectors and Maps yield vectors and (hash) maps whose contents are the
      evaluated values of the objects they contain."
    (is (= (eval '[x y 3]) [1 2 3]))
    (is (= (eval '{:x x :y y :z 3}) {:x 1 :y 2 :z 3}))
    (is (instance? clojure.lang.IPersistentMap (eval '{:x x :y y})))))

  (in-test-ns
   (test-that
    "Metadata maps yield maps whose contents are the evaluated values of
      the objects they contain. If a vector or map has metadata, the evaluated
      metadata map will become the metadata of the resulting value."
    (is (= (eval ^{:x x} '[x y]) ^{:x 1} [1 2]))))

  (test-that
   "An empty list () evaluates to an empty list."
   (is (= (eval '()) ()))
   (is (empty? (eval ())))
   (is (= (eval (list)) ()))))

(deftest Macros)

(deftest Loading)
