;; Sample clojure.test file. Picked up by the pytest plugin via the
;; *_test.clj glob.

(ns clojure.tests.sample-test
  (:require [clojure.test :refer [deftest is testing are]]))

(deftest add-test
  (is (= 4 (+ 2 2)))
  (is (= 5 (+ 2 3))))

(deftest test-with-context
  (testing "with positive integers"
    (is (= 4 (+ 2 2)))
    (is (= 7 (+ 3 4))))
  (testing "with negative integers"
    (is (= -4 (+ -2 -2)))
    (is (= -1 (+ 3 -4)))))

(deftest are-test
  (are [x y] (= x y)
    2 (+ 1 1)
    4 (* 2 2)
    9 (* 3 3)))

(deftest thrown-test
  (is (thrown? py.__builtins__/ZeroDivisionError (/ 1 0)))
  (is (thrown? py.__builtins__/ValueError (parse-long 42))))

(deftest non-empty-coll-test
  (is (seq [1 2 3]))
  (is (= [1 2 3] (vec '(1 2 3)))))

;; Sanity-check failure reporting. Comment out to make the file pass
;; cleanly under pytest. Left here as a manual check for the plugin's
;; failure surface.
;;
;; (deftest demo-failure
;;   (is (= 5 (+ 2 2)) "two plus two should be five (it isn't)"))
