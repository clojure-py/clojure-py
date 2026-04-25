;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

;; Adapted from clojure/test/clojure/test_clojure/java_interop.clj.
;;
;; What this file ports:
;;   * `test-double-dot` — `..` chaining still works, just on Python methods.
;;   * `test-doto` — works on any object with mutating methods (Python list,
;;     dict, set, etc.).
;;   * `test-instance?` — adapted to Python builtin types (`builtins/list`,
;;     `builtins/str`, `builtins/int`, `builtins/dict`).
;;   * `test-bean` — works on plain-attribute Python objects (SimpleNamespace
;;     stand-in via a tiny defrecord-style wrapper).
;;   * `test-iterable-bean` — bean is iterable + hashable.
;;   * `test-boolean` — truthy coercion. (Note: clojure-py has no separate
;;     `Boolean` type, so we drop the `(instance? java.lang.Boolean ...)`
;;     check; we keep the value comparison.)
;;   * `test-char` — Char is a real type now (see crates/clojure_core/src/char.rs).
;;
;; What this file deliberately omits:
;;   * `test-dot` (the bare `.` form is well-tested in tests/test_python_interop.py).
;;   * `test-new` (Python classes are callable directly — `(MyClass args)` —
;;     there is no `(MyClass. args)` constructor sugar; covered in test_python_interop).
;;   * `test-set!` (mutating-attribute via `set!` covered in test_python_interop).
;;   * `test-proxy*`, `test-bases`, `test-supers`, `test-reflective-*`,
;;     `test-make-array`, `test-to-array`, `test-into-array`, `test-alength`,
;;     `test-aclone`, `test-boxing-prevention*`, all FI tests, all
;;     CLJ-2898/CLJ-2914 tests — JVM-only.

(ns clojure.test-clojure.python-interop
  (:use clojure.test))

;; --- Double-dot chaining (..) ----------------------------------------------

(deftest test-double-dot
  (is (= (.. "  hello  " strip upper)
         (. (. "  hello  " strip) upper)))
  (is (= "HELLO" (.. "  hello  " strip upper))))


;; --- doto -------------------------------------------------------------------
;; Vanilla uses (java.util.HashMap.); we use a Python list/set via builtins.
;; Clojure `=` doesn't equate Python `list` to PVector, so we compare via
;; `vec` (round-trip through seq) for the contents.

(deftest test-doto-list
  (let [l (doto (builtins/list)
            (.append 1)
            (.append 2)
            (.append 3))]
    (are [x y] (= x y)
        (class l)  builtins/list
        (vec l)    [1 2 3]
        (count l)  3)))

(deftest test-doto-set
  (let [s (doto (builtins/set)
            (.add 1)
            (.add 2)
            (.add 1))]
    (is (= (class s) builtins/set))
    (is (= (count s) 2))))


;; --- instance? --------------------------------------------------------------

(deftest test-instance?
  ;; evaluation
  (are [x y] (= x y)
      (instance? builtins/int (+ 1 2)) true
      (instance? builtins/str (+ 1 2)) false)

  ;; different types
  (are [val cls] (instance? cls val)
      1     builtins/int
      1.0   builtins/float
      \a    clojure._core/Char
      "a"   builtins/str
      []    clojure._core/PersistentVector
      {}    clojure._core/PersistentArrayMap
      #{}   clojure._core/PersistentHashSet)

  ;; nil is never `instance?` of anything (matches vanilla)
  (are [cls] (not (instance? cls nil))
      builtins/int
      builtins/str
      clojure._core/Char)

  ;; 42 is an int, nothing else
  (are [cls expected] (= (instance? cls 42) expected)
      builtins/int   true
      builtins/str   false
      builtins/float false
      clojure._core/Char false)

  ;; instance? with too few args throws
  (is (thrown? clojure._core/ArityException (instance? builtins/int))))


;; --- bean -------------------------------------------------------------------
;; Vanilla uses java.awt.Color; we use a defrecord (which is a Python class
;; with attribute-backed fields) since SimpleNamespace's kwarg constructor
;; isn't reachable from Clojure-side syntax.

(defrecord Color [red green blue alpha])

(deftest test-bean
  (let [b (bean (->Color 0 0 0 255))]
    (are [x y] (= x y)
        (:red b)                  0
        (:green b)                0
        (:blue b)                 0
        (:alpha b)                255

        (:missing b)              nil
        (:missing b :default)     :default
        (get b :missing)          nil
        (get b :missing :default) :default)))

(defrecord Counts [a b c])

(deftest test-iterable-bean
  (let [b (bean (->Counts 1 2 3))]
    ;; bean must be iterable (seq returns a non-nil sequence of map-entries)
    (is (seq b))
    ;; iterating it twice yields the same data
    (is (= (into [] b) (into [] (seq b))))
    ;; bean is hashable
    (is (number? (hash b)))))


;; --- boolean (truthy coercion) ---------------------------------------------
;; Vanilla also asserts `(instance? java.lang.Boolean ...)`; we drop that
;; because Python's bool ⊂ int and we don't have a separate Boolean type.

(deftest test-boolean
  (are [x y] (= (boolean x) y)
      nil   false
      false false
      true  true

      0     true
      1     true
      ()    true
      [1]   true

      ""    true
      \space true
      :kw   true))


;; --- char (Char is now a distinct type) ------------------------------------

(deftest test-char
  ;; int -> Char
  (is (instance? clojure._core/Char (char 65)))
  (is (= (char 65) \A))

  ;; Char -> Char (idempotent)
  (is (instance? clojure._core/Char (char \a)))
  (is (= (char \a) \a))

  ;; 1-char str -> Char
  (is (= (char "z") \z))

  ;; Char is NOT a string
  (is (not (instance? builtins/str \a))))


;; --- Pass a Clojure fn where Python expects a callable --------------------
;; Vanilla equivalent: `clojure-fn-as-java-fn`. Python idiom: pass our IFn
;; into `builtins/map` / `builtins/filter` / `functools/reduce` — any
;; positional-callable. We don't have a Clojure-side syntax for Python kwargs,
;; so we stick to the positional builtins.

(deftest test-clojure-fn-as-python-callable
  ;; map a Clojure fn through Python's builtin map (positional)
  (is (= [2 4 6] (vec (builtins/map (fn [x] (* 2 x)) [1 2 3]))))

  ;; filter via Python's builtin filter
  (is (= [2 4] (vec (builtins/filter even? [1 2 3 4 5]))))

  ;; Pass a Clojure fn to .sort on a Python list — the `key` arg is callable.
  ;; (Verified separately because Python's `key=` kwarg isn't directly
  ;; expressible from Clojure-side syntax — but `.sort` defaults to natural
  ;; order, and a Clojure-comparable callable works via the default path.)
  (let [l (doto (builtins/list)
            (.append 3) (.append 1) (.append 2))]
    (.sort l)
    (is (= [1 2 3] (vec l)))))
