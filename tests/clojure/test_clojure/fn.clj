;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

; Author: Ambrose Bonnaire-Sergeant

;; Port of clojure/test/clojure/test_clojure/fn.clj.
;;
;; Adaptations from JVM:
;;   - JVM `fn` validates its args via clojure.spec and throws
;;     ExceptionInfo with messages like "Call to clojure.core/fn did
;;     not conform to spec". We don't have clojure.spec; our `fn`
;;     uses assert-valid-fdecl which raises IllegalArgumentException
;;     with messages like "Parameter declaration X should be a
;;     vector" or "Parameter declaration missing". The test asserts
;;     that bad fn forms raise *some* exception with a recognizable
;;     message — enough to validate the fail-on-bad-form behavior
;;     without locking into a specific framework.

(ns clojure.test-clojure.fn
  (:use clojure.test
        clojure.test-helper))

(deftest fn-error-checking
  (testing "bad arglist"
    (is (thrown-with-msg?
          Exception #"Parameter declaration"
          (eval '(fn "a" a)))))

  (testing "treat first param as args"
    (is (thrown-with-msg?
          Exception #"Parameter declaration"
          (eval '(fn "a" [])))))

  (testing "looks like listy signature, but malformed declaration"
    (is (thrown-with-msg?
          Exception #"Parameter declaration"
          (eval '(fn (1))))))

  (testing "checks each signature"
    (is (thrown-with-msg?
          Exception #"Parameter declaration"
          (eval '(fn
                   ([a] 1)
                   ("a" 2))))))

  (testing "correct name but invalid args"
    (is (thrown-with-msg?
          Exception #"Parameter declaration"
          (eval '(fn a "a")))))

  (testing "first sig looks multiarity, rest of sigs should be lists"
    ;; Adaptation: assert-valid-fdecl emits "Invalid signature ..."
    ;; for this case (where one arity uses a list, another a vector).
    (is (thrown-with-msg?
          Exception #"(?:Parameter declaration|Invalid signature)"
          (eval '(fn a
                   ([a] 1)
                   [a b])))))

  (testing "missing parameter declaration"
    (is (thrown-with-msg?
          Exception #"Parameter declaration missing"
          (eval '(fn a))))
    (is (thrown-with-msg?
          Exception #"Parameter declaration missing"
          (eval '(fn))))))
