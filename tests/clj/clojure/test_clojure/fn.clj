;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

; Author: Ambrose Bonnaire-Sergeant

;; Adapted from clojure/test/clojure/test_clojure/fn.clj.
;;
;; Vanilla checks each malformed `fn` form against `clojure.spec`. We don't
;; port spec; our compiler raises `IllegalArgumentException` with an
;; ad-hoc message ("Parameter declaration ... should be a vector",
;; "Parameter declaration missing"). Same property, different error class.

(ns clojure.test-clojure.fn
  (:use clojure.test))

(deftest fn-error-checking
  (testing "bad arglist"
    (is (thrown? clojure._core/IllegalArgumentException
          (eval '(fn "a" a)))))

  (testing "treat first param as args"
    (is (thrown? clojure._core/IllegalArgumentException
          (eval '(fn "a" [])))))

  (testing "looks like listy signature, but malformed declaration"
    (is (thrown? clojure._core/IllegalArgumentException
          (eval '(fn (1))))))

  (testing "checks each signature"
    (is (thrown? clojure._core/IllegalArgumentException
          (eval '(fn
                   ([a] 1)
                   ("a" 2))))))

  (testing "correct name but invalid args"
    (is (thrown? clojure._core/IllegalArgumentException
          (eval '(fn a "a")))))

  (testing "first sig looks multiarity, rest of sigs should be lists"
    (is (thrown? clojure._core/IllegalArgumentException
          (eval '(fn a
                   ([a] 1)
                   [a b])))))

  (testing "missing parameter declaration"
    (is (thrown? clojure._core/IllegalArgumentException
          (eval '(fn a))))
    (is (thrown? clojure._core/IllegalArgumentException
          (eval '(fn))))))
