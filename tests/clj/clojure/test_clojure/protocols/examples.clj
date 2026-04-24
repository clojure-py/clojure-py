;;
;; Adaptations from vanilla:
;;   * Dropped `(definterface ExampleInterface (hinted [^int i]) ...)`
;;     and `(defprotocol LongsHintedProto (^longs longs-hinted ...))` —
;;     definterface + primitive type hints are JVM-only.
;;   * Dropped the `^String` return-type hint on `baz` (JVM type hint).

(ns clojure.test-clojure.protocols.examples)

(defprotocol ExampleProtocol
  "example protocol used by clojure tests"

  (foo [a] "method with one arg")
  (bar [a b] "method with two args")
  (baz [a] [a b] "method with multiple arities")
  (with-quux [a] "method name with a hyphen"))

(defprotocol MarkerProtocol
  "a protocol with no methods")

(defprotocol MarkerProtocol2)
