;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

; Author: Frantisek Sodomka

;;
;; Adaptations from vanilla:
;;   * Dropped sample-data entries that reference unsupported literals/types:
;;     :bigint, :bigdec,
;;     :empty-array / :array (`into-array`), :class (`java.util.Date`),
;;     :object (`(new java.util.Date)`).
;;   * Dropped `test-string?-more` (uses `java.lang.StringBuilder` /
;;     `java.lang.StringBuffer`).
;;   * `test-preds`: dropped rows that need `0.0M` / `0N` / UUID / URI /
;;     java.util.Date / byte-array; dropped columns `uuid?`, `decimal?`,
;;     `inst?`, `uri?`, `bytes?` since no row uses them after dropping.

(ns clojure.test-clojure.predicates
  (:use clojure.test))


;; *** Type predicates ***

(def myvar 42)

(def sample-data {
  :nil nil

  :bool-true true
  :bool-false false

  :byte   (byte 7)
  :short  (short 7)
  :int    (int 7)
  :long   (long 7)
  :float  (float 7)
  :double (double 7)

  :ratio 2/3

  :character \a
  :symbol 'abc
  :keyword :kw

  :empty-string ""
  :empty-regex #""
  :empty-list ()
  :empty-lazy-seq (lazy-seq nil)
  :empty-vector []
  :empty-map {}
  :empty-set #{}

  :string "abc"
  :regex #"a*b"
  :list '(1 2 3)
  :lazy-seq (lazy-seq [1 2 3])
  :vector [1 2 3]
  :map {:a 1 :b 2 :c 3}
  :set #{1 2 3}

  :fn (fn [x] (* 2 x))

  :var (var myvar)
  :delay (delay (+ 1 2))
})


(def type-preds {
  nil? [:nil]

  true?  [:bool-true]
  false? [:bool-false]
  ; boolean?

  integer?  [:byte :short :int :long]
  float?    [:float :double]
  ratio?    [:ratio]
  rational? [:byte :short :int :long :ratio]
  number?   [:byte :short :int :long :ratio :float :double]

  ; character?
  symbol?  [:symbol]
  keyword? [:keyword]

  string? [:empty-string :string]

  list?   [:empty-list   :list]
  vector? [:empty-vector :vector]
  map?    [:empty-map    :map]
  set?    [:empty-set    :set]

  coll? [:empty-list     :list
         :empty-lazy-seq :lazy-seq
         :empty-vector   :vector
         :empty-map      :map
         :empty-set      :set]

  seq?  [:empty-list     :list
         :empty-lazy-seq :lazy-seq]

  fn?  [:fn]
  ifn? [:fn
        :empty-vector :vector :empty-map :map :empty-set :set
        :keyword :symbol :var]

  var?   [:var]
  delay? [:delay]
})


;; Test all type predicates against all data types
;;
(defn- get-fn-name [f]
  ;; Vanilla reaches into the class name via `.split` / `clojure.core$`;
  ;; we just stringify the Var / Fn since `(str f)` already renders a
  ;; useful label (`clojure.core/nil?` or `#'clojure.core/nil?`).
  (str f))

(deftest test-type-preds
  (doseq [tp type-preds]
    (doseq [dt sample-data]
      (if (some #(= % (first dt)) (second tp))
        (is ((first tp) (second dt))
          (pr-str (list (first dt) :in (second tp))))
        (is (not ((first tp) (second dt)))
          (pr-str (list 'not (list (first dt) :in (second tp)))))))))


;; Additional tests — vanilla `test-string?-more` uses
;; `java.lang.StringBuilder` / `java.lang.StringBuffer`; dropped.

(def pred-val-table
  ;; Vanilla also tests: 0.0M, 0N (BigInt literal), uuid, uri, now (Date),
  ;; barray (byte-array), with columns uuid?/decimal?/inst?/uri?/bytes?.
  ;; Those rows/columns are removed for this port.
  ['
   [identity   int?  pos-int?  neg-int?  nat-int?  double? boolean? indexed? seqable? ident?]
   [0          true  false     false     true      false   false    false    false    false ]
   [1          true  true      false     true      false   false    false    false    false ]
   [-1         true  false     true      false     false   false    false    false    false ]
   [1.0        false false     false     false     true    false    false    false    false ]
   [true       false false     false     false     false   true     false    false    false ]
   [[]         false false     false     false     false   false    true     true     false ]
   [nil        false false     false     false     false   false    false    true     false ]
   [{}         false false     false     false     false   false    false    true     false ]
   [:foo       false false     false     false     false   false    false    false    true  ]
   ['foo       false false     false     false     false   false    false    false    true  ]])

(deftest test-preds
  (let [[preds & rows] pred-val-table]
    (doseq [row rows]
      (let [v (first row)]
        (dotimes [i (count row)]
          (is (= ((resolve (nth preds i)) v) (nth row i))
              (pr-str (list (nth preds i) v))))))))

;; Vanilla also exercises `(Double/parseDouble "NaN")`, `Float/NaN`,
;; `Double/POSITIVE_INFINITY`, etc. — Java interop, dropped.
;; `(thrown? Throwable (NaN? nil))` / `(NaN? :xyz)` dropped: our
;; implementations return `false` on non-numeric input rather than throw.
(deftest test-double-preds
  (is (NaN? ##NaN))
  (is (not (NaN? 5)))

  (is (infinite? ##Inf))
  (is (infinite? ##-Inf)))
