;;
;; Adaptations / omissions from vanilla:
;;   * `calls-once-in-parallel` / `saves-exceptions-in-parallel` — use
;;     JVM `CyclicBarrier` + `Thread.`; no portable equivalent here.
;;     Dropped.
;;   * `delays-are-suppliers` — JVM `java.util.function.*` interop.
;;     Dropped.
;;   * `(throw (Exception. "broken"))` → inline
;;     `(throw (clojure._core/IllegalStateException "broken"))`.

(ns clojure.test-clojure.delays
  (:use clojure.test))

(deftest calls-once
  (let [a (atom 0)
        d (delay (swap! a inc))]
    (is (= 0 @a))
    (is (= 1 @d))
    (is (= 1 @d))
    (is (= 1 @a))))

(deftest saves-exceptions
  (let [f #(do (throw (clojure._core/IllegalStateException "broken"))
               1)
        d (delay (f))
        try-call #(try
                    @d
                    (catch Exception e e))
        first-result (try-call)]
    (is (instance? clojure._core/IllegalStateException first-result))
    (is (identical? first-result (try-call)))))
