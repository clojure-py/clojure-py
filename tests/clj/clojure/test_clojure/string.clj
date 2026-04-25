;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

;; Adapted from clojure/test/clojure/test_clojure/string.clj.
;;
;; Adaptations from vanilla:
;;   * Dropped tests for fns we don't yet implement: `re-quote-replacement`,
;;     `trim-newline`, `escape`, `last-index-of`.
;;   * Dropped `nil-handling` (NullPointerException) — our string fns don't
;;     uniformly raise on nil, and Python has no NullPointerException.
;;   * Dropped `char-sequence-handling` (StringBuffer / CharSequence) — no
;;     CharSequence abstraction here.
;;   * `s/replace ... \o \a` — vanilla uses Char-Char form. Our `replace`
;;     accepts strings; we adapt those rows to single-char strings, which
;;     is what `\o` / `\a` round-trip to via `(str \o)`.

(ns clojure.test-clojure.string
  (:require [clojure.string :as s])
  (:use clojure.test))


(deftest t-split
  (is (= ["a" "b"] (s/split "a-b" #"-")))
  (is (= ["a" "b-c"] (s/split "a-b-c" #"-" 2)))
  (is (vector? (s/split "abc" #"-"))))


(deftest t-reverse
  (is (= "tab" (s/reverse "bat"))))


(deftest t-replace
  ;; String replacement (vanilla also covers \o → \a; our `replace` doesn't
  ;; accept Char inputs, so use the equivalent string form).
  (is (= "barbarbar" (s/replace "foobarfoo" "foo" "bar")))
  (is (= "foobarfoo" (s/replace "foobarfoo" "baz" "bar")))
  (is (= "f$$d"      (s/replace "food" "o" "$")))
  ;; Regex replacement
  (is (= "barbarbar" (s/replace "foobarfoo" #"foo" "bar")))
  (is (= "foobarfoo" (s/replace "foobarfoo" #"baz" "bar"))))


(deftest t-replace-first
  (is (= "barbarfoo" (s/replace-first "foobarfoo" "foo" "bar")))
  (is (= "foobarfoo" (s/replace-first "foobarfoo" "baz" "bar")))
  (is (= "f$od"      (s/replace-first "food" "o" "$")))
  (is (= "barbarfoo" (s/replace-first "foobarfoo" #"foo" "bar")))
  (is (= "foobarfoo" (s/replace-first "foobarfoo" #"baz" "bar"))))


(deftest t-join
  (are [x coll] (= x (s/join coll))
       "" nil
       "" []
       "1" [1]
       "12" [1 2])
  (are [x sep coll] (= x (s/join sep coll))
       "1,2,3" "," [1 2 3]
       "" "," []
       "1" "," [1]
       "1 and-a 2 and-a 3" " and-a " [1 2 3]))


(deftest t-capitalize
  (is (= "Foobar" (s/capitalize "foobar")))
  ;; vanilla: "Foobar" — Java capitalize lower-cases the rest. Python's
  ;; .capitalize() does the same. Verify we match.
  (is (= "Foobar" (s/capitalize "FOOBAR"))))


(deftest t-triml
  (is (= "foo " (s/triml " foo ")))
  (is (= "" (s/triml "   ")))
  ;;   is a Unicode whitespace; Python's str.lstrip strips all
  ;; whitespace by default — verify.
  (is (= "bar" (s/triml "  \tbar"))))


(deftest t-trimr
  (is (= " foo" (s/trimr " foo ")))
  (is (= "" (s/trimr "   ")))
  (is (= "bar" (s/trimr "bar\t  "))))


(deftest t-trim
  (is (= "foo" (s/trim "  foo  \r\n")))
  (is (= "bar" (s/trim " bar\t  "))))


(deftest t-upper-case
  (is (= "FOOBAR" (s/upper-case "Foobar"))))


(deftest t-lower-case
  (is (= "foobar" (s/lower-case "FooBar"))))


(deftest t-blank
  (is (s/blank? nil))
  (is (s/blank? ""))
  (is (s/blank? " "))
  (is (s/blank? " \t \n  \r "))
  (is (not (s/blank? "  foo  "))))


(deftest t-split-lines
  (let [result (s/split-lines "one\ntwo\r\nthree")]
    (is (= ["one" "two" "three"] result))
    (is (vector? result)))
  ;; Vanilla returns `(list "foo")`; our split-lines may return a vector for
  ;; the single-line case too. Verify content rather than concrete type.
  (is (= ["foo"] (vec (s/split-lines "foo")))))


(deftest t-index-of
  (is (= 2  (s/index-of "tacos" "c")))
  (is (= 1  (s/index-of "tacos" "ac")))
  (is (= 3  (s/index-of "tacos" "o" 2)))
  (is (= nil (s/index-of "tacos" "z")))
  (is (= nil (s/index-of "tacos" "z" 2))))


(deftest t-starts-with?
  (is (s/starts-with? "clojure west" "clojure"))
  (is (not (s/starts-with? "conj" "clojure"))))


(deftest t-ends-with?
  (is (s/ends-with? "Clojure West" "West"))
  (is (not (s/ends-with? "Conj" "West"))))


(deftest t-includes?
  (is (s/includes? "Clojure Applied Book" "Applied"))
  (is (not (s/includes? "Clojure Applied Book" "Living"))))


(deftest empty-collections
  ;; Not specific to clojure.string but in the file: `(str ())` → "()"
  (is (= "()" (str ())))
  (is (= "{}" (str {})))
  (is (= "[]" (str []))))
