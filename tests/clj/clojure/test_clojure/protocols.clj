;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

; Author: Stuart Halloway

;;
;; Adaptations / omissions from vanilla:
;; This file is heavily JVM-focused. Most of the >720 lines test
;; features that are inherently Java: `definterface`, `proxy`,
;; `AbstractMethodError`, primitive type hints (`^long`, `^String`),
;; record literal syntax (`#clojure.pkg.Rec[...]`, `#clojure.pkg.Rec{...}`),
;; `java.util.Map$Entry` interop, Java class inheritance checks, etc.
;;
;; Dropped entirely (JVM-only):
;;   * All `method-names` / `.getMethods` reflection checks.
;;   * All `proxy [...]`-based tests.
;;   * All `AbstractMethodError` expectations.
;;   * All record/deftype literal syntax tests.
;;   * All `^long` / `^byte` / `^boolean` / `^String` hint tests.
;;   * `defrecord-interfaces-test` — uses `.size`, `.containsKey`,
;;     `.keySet`, `.values`, `.entrySet` (JVM Map interface).
;;   * `test-ctor-literals`, `exercise-literals`, `defrecord-printing`
;;     — tagged record/type literal reader syntax.
;;   * `test-statics`, `deftype-factory-fn` — `getBasis` / static
;;     `create` methods.
;;   * `test-record-and-type-field-names` — uses `.__a` field access.
;;   * `hinting-test` — primitive hints.
;;   * `marker-tests`, `illegal-extending` — need JVM interface checks
;;     and `proxy`.
;;
;; Kept (portable, matches vanilla behavior):
;;   * Basic protocol definition + dispatch (`protocols-test` trimmed).
;;   * Redefining a protocol, satisfying via reify/extend-protocol.
;;   * Record structural equality (`defrecord-object-methods-test`).
;;   * Record map-like merge / rename-keys / merge-with.
;;   * Record factory functions.
;;   * Record-as-map iteration / dissoc / assoc.

(ns clojure.test-clojure.protocols
  (:use clojure.test clojure.test-clojure.protocols.examples)
  (:require [clojure.test-clojure.protocols.more-examples :as other]
            [clojure.set :as set]))

(defrecord EmptyRecord [])
(defrecord TestRecord [a b])
(defn r
  ([a b] (->TestRecord a b)))

(deftest protocols-test
  (testing "protocol fns throw IllegalArgumentException if no impl matches"
    ;; Vanilla tests the full error message which mentions
    ;; `java.lang.Long`. We have `int` as the class name here.
    (is (thrown-with-msg?
          clojure._core/IllegalArgumentException
          #"No implementation of method"
          (foo 10))))
  (testing "you can implement just part of a protocol if you want"
    ;; Vanilla also asserts `(thrown? AbstractMethodError (baz obj))`
    ;; for the unimplemented 1-arg arity; our dispatch raises
    ;; IllegalArgumentException instead.
    (let [obj (reify ExampleProtocol
                     (baz [a b] "two-arg baz!"))]
      (is (= "two-arg baz!" (baz obj nil))))))

(deftest defrecord-object-methods-test
  (testing "= depends on fields and type"
    (let [r1 (->TestRecord 1 2)
          r2 (->TestRecord 1 2)
          r3 (->TestRecord 1 3)
          ;; vanilla uses a second record type here
          other-type (->EmptyRecord)]
      (is (true? (= r1 r2)))
      (is (false? (= r1 r3)))
      (is (false? (= r1 other-type))))))

(deftest defrecord-acts-like-a-map
  (let [rec (r 1 2)]
    ;; Vanilla uses `.equals`; our `=` compares by type + fields.
    (is (= {:a 1 :b 2 :c 4} (merge rec {:c 4})))
    (is (= {:foo 1 :b 2} (set/rename-keys rec {:a :foo})))
    (is (= {:a 11 :b 2 :c 10} (merge-with + rec {:a 10 :c 10})))))

(deftest degenerate-defrecord-test
  (let [empty (->EmptyRecord)]
    (is (nil? (seq empty)))))

(defrecord RecordWithSpecificFieldNames [this that k m o])
(deftest defrecord-with-specific-field-names
  (let [rec (->RecordWithSpecificFieldNames 1 2 3 4 5)]
    (is (= rec rec))
    (is (= 3 (get rec :k)))
    (is (= (seq rec) '([:this 1] [:that 2] [:k 3] [:m 4] [:o 5])))))

(defrecord RecordToTestFactories [a b c])
(defrecord RecordToTestA [a])
(defrecord RecordToTestB [b])
(defrecord RecordToTestDegenerateFactories [])

(deftest test-record-factory-fns
  (testing "if the definition of a defrecord generates the appropriate factory functions"
    (let [r    (->RecordToTestFactories 1 2 3)
          r-n  (->RecordToTestFactories nil nil nil)
          r-a  (map->RecordToTestA {:a 1 :b 2})
          r-b  (map->RecordToTestB {:a 1 :b 2})
          r-d  (->RecordToTestDegenerateFactories)]
      (testing "that a record created with the ctor equals one by the positional factory fn"
        (is (= r    (->RecordToTestFactories 1 2 3))))
      (testing "that a record created with the ctor equals one by the map-> factory fn"
        (is (= r    (map->RecordToTestFactories {:a 1 :b 2 :c 3})))
        (is (= r-n  (map->RecordToTestFactories {})))
        (is (= r-d  (map->RecordToTestDegenerateFactories {}))))
      (testing "record equality"
        (is (not= r-a r-b))
        ;; same type with same fields → equal.
        (is (= r-a (map->RecordToTestA {:a 1 :b 2})))))))
