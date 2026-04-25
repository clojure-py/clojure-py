;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

;; Adapted from clojure/test/clojure/test_clojure/reader.cljc.
;;
;; Adaptations from vanilla:
;;   * Dropped tests that depend on Java types (`File`, `LineNumberingPushback
;;     Reader`, `clojure.lang.Symbol`/`Keyword`/`PersistentList`/etc. via
;;     instance? against Java classes) — we use the clojure._core/* type names
;;     where it matters.
;;   * Dropped octal-escape tests (`\o000`, `\o377`, etc.) — our reader
;;     supports `\u{4hex}` but not `\o{octal}` or `\3` short-form octal.
;;   * Dropped detailed escape-error message regexes — our messages differ
;;     from vanilla's (different reader implementation).
;;   * Dropped ratio (`1/2`), BigDecimal (`1M`), BigInt (`1N`) literal tests.
;;   * Dropped `Instants` (Date), `UUID`, `t-line-column-numbers`,
;;     `set-line-number`, `reader-conditionals`, `namespaced-maps*`,
;;     `test-read+string` — JVM-only or reader-conditional features.
;;   * Dropped `defspec` property tests (covered by hypothesis fuzz).
;;   * `Anonymous-function-literal`: dropped the malformed-form error
;;     assertions (our reader emits different messages); kept the
;;     positive-form expansion checks.

(ns clojure.test-clojure.reader
  (:use clojure.test))


(deftest Symbols
  (is (= 'abc (symbol "abc")))
  (is (= '*+!-_? (symbol "*+!-_?")))
  (is (= 'abc:def:ghi (symbol "abc:def:ghi")))
  (is (= 'abc/def (symbol "abc" "def")))
  (is (= 'abc.def/ghi (symbol "abc.def" "ghi")))
  (is (= 'abc/def.ghi (symbol "abc" "def.ghi")))
  (is (instance? clojure._core/Symbol 'alphabet)))


(deftest Literals
  ;; 'nil 'false 'true are reserved by Clojure and are not symbols
  (is (= 'nil nil))
  (is (= 'false false))
  (is (= 'true true)))


(deftest Strings
  (is (= "abcde" (str \a \b \c \d \e)))
  (is (= "abc\n  def" (str \a \b \c \newline \space \space \d \e \f)))
  ;; Reader: basic string forms round-trip through `read-string`.
  (are [expected form] (= expected (read-string form))
       ""        "\"\""
       "a"       "\"a\""
       "abc"     "\"abc\""
       "a b c"   "\"a b c\""
       "\n"      "\"\\n\""
       "\t"      "\"\\t\""
       "\\"      "\"\\\\\""
       "\""      "\"\\\"\""
       "A"       "\"\\u0041\"")
  ;; Errors
  (is (thrown? clojure._core/IllegalArgumentException (read-string "\""))))


(deftest Numbers
  ;; Reader produces Python int (analog of vanilla Long).
  (is (instance? builtins/int 2147483647))
  (is (instance? builtins/int 1))
  (is (instance? builtins/int 0))
  (is (instance? builtins/int -1))
  (is (instance? builtins/int 9223372036854775807))
  (is (instance? builtins/int 999999999999999999999999999))   ; arbitrary precision
  ;; Floats
  (is (instance? builtins/float 1.0))
  (is (instance? builtins/float 1.5))
  (is (instance? builtins/float 1e3))
  (is (= 1000.0 (read-string "1e3")))
  (is (= 0.015 (read-string "1.5e-2")))
  ;; Constants of different types should not wash out (regression r1157)
  (is (not= 0 0.0))
  (is (= 0 (read-string "0")))
  (is (= 0.0 (read-string "0.0"))))


(deftest t-Characters
  ;; Basic char literals
  (are [expected form] (= expected (read-string form))
       \a       "\\a"
       \z       "\\z"
       \A       "\\A"
       \space   "\\space"
       \newline "\\newline"
       \tab     "\\tab"
       \return  "\\return"
       \backspace "\\backspace"
       \formfeed  "\\formfeed"
       \null    "\\null"
       (char 65)   "\\u0041"
       (char 0)    "\\u0000"))


(deftest t-Keywords
  (is (= :abc (keyword "abc")))
  (is (= :abc (keyword 'abc)))
  (is (= :*+!-_? (keyword "*+!-_?")))
  (is (= :abc:def:ghi (keyword "abc:def:ghi")))
  (is (= :abc/def (keyword "abc" "def")))
  (is (= :abc/def (keyword 'abc/def)))
  (is (= :abc.def/ghi (keyword "abc.def" "ghi")))
  (is (instance? clojure._core/Keyword :alphabet)))


(deftest reading-keywords
  (are [x y] (= x (read-string y))
       :foo        ":foo"
       :foo/bar    ":foo/bar"))


(deftest t-Lists
  (are [x form] (= x (read-string form))
       '()         "()"
       '(1)        "(1)"
       '(1 2 3)    "(1 2 3)"
       '(a b c)    "(a b c)"))


(deftest t-Vectors
  (are [x form] (= x (read-string form))
       []          "[]"
       [1]         "[1]"
       [1 2 3]     "[1 2 3]"
       [[1] [2 3]] "[[1] [2 3]]"))


(deftest t-Maps
  (are [x form] (= x (read-string form))
       {}                  "{}"
       {:a 1}              "{:a 1}"
       {:a 1 :b 2}         "{:a 1 :b 2}"
       {:a {:b {:c 1}}}    "{:a {:b {:c 1}}}"))


(deftest t-Sets
  (are [x form] (= x (read-string form))
       #{}             "#{}"
       #{1}            "#{1}"
       #{1 2 3}        "#{1 2 3}"))


(deftest t-Quote
  ;; 'x reads as (quote x)
  (let [r (read-string "'x")]
    (is (= 'quote (first r)))
    (is (= 'x (second r)))))


(deftest t-Var-quote
  ;; #'x reads as (var x)
  (let [r (read-string "#'x")]
    (is (= 'var (first r)))
    (is (= 'x (second r)))))


(deftest t-Deref
  ;; @x reads as (clojure.core/deref x)
  (let [r (read-string "@x")]
    (is (= 'clojure.core/deref (first r)))
    (is (= 'x (second r)))))


(deftest t-Comment
  ;; line comment skipped before form
  (is (= 42 (read-string "; comment\n42")))
  ;; #_ form-discard
  (is (= 'x (read-string "#_y x"))))


(deftest t-Regex
  (is (= "abc"  (re-find (read-string "#\"a.c\"") "abc def"))))


(deftest t-Anonymous-function-literal
  ;; Note: vanilla expands `#(...)` to `(fn* ...)` with gensym'd args; our
  ;; reader expands to `(fn ...)` with direct `%`/`%1`/`%&` arg names.
  (is (= "(fn [] (vector))" (pr-str (read-string "#(vector)"))))
  (let [s (pr-str (read-string "#(vector %)"))]
    (is (re-matches #"\(fn \[([\S]+)\] \(vector \1\)\)" s)))
  (let [s (pr-str (read-string "#(vector %1 %2)"))]
    (is (re-matches #"\(fn \[([\S]+) ([\S]+)\] \(vector \1 \2\)\)" s)))
  (let [s (pr-str (read-string "#(vector %2 %&)"))]
    (is (re-matches #"\(fn \[([\S]+) ([\S]+) & ([\S]+)\] \(vector \2 \3\)\)" s))))


(deftest t-Syntax-quote
  (is (= '() `())))


(deftest t-Metadata
  ;; `^:foo sym` reads as `(with-meta sym {:foo true})`.
  (is (= (meta '^:awesome sym) {:awesome true}))
  (is (= (meta '^{:bar :baz} sym) {:bar :baz}))
  ;; Note: vanilla merges multiple `^` annotations; ours retains only the
  ;; outermost. Documented divergence — single-tag use is what's tested.
  )
