;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

;;; clojure.test — port of Stuart Sierra's test framework, adapted for the
;;; clojure-py PyO3 runtime.
;;;
;;; Adaptations vs vanilla:
;;;   * Catch clauses use `builtins.Exception` (Python) instead of `Throwable`.
;;;   * `.getMessage` → `(str e)` (Python exceptions stringify to their msg).
;;;   * `file-position` / StackTraceElement-based reporting is dropped — we
;;;     rely on form metadata (`:line` attached by the reader) and on the
;;;     pytest plugin's own item-level file/line for user-facing location.
;;;   * `clojure.stacktrace` / `clojure.string` requires are gone — we don't
;;;     filter JVM stack frames out of reports.

(ns clojure.test
  (:require [clojure.template :as temp]))

;;; USER-MODIFIABLE GLOBALS

(def ^:dynamic *load-tests*
  "True by default.  If set to false, no test functions will be created by
   deftest, set-test, or with-test.  Use this to omit tests when compiling
   or loading production code."
  true)

(def ^:dynamic *stack-trace-depth*
  "The maximum depth of stack traces to print when an exception is thrown
   during a test.  Defaults to nil (print the complete stack trace).
   Retained as a Var for compatibility; currently a hint only."
  nil)

;;; GLOBALS USED BY THE REPORTING FUNCTIONS

(def ^:dynamic *report-counters* nil)          ; bound to a ref of a map in test-ns

(def ^:dynamic *initial-report-counters*       ; used to initialize *report-counters*
  {:test 0, :pass 0, :fail 0, :error 0})

(def ^:dynamic *testing-vars* (list))          ; hierarchy of vars being tested

(def ^:dynamic *testing-contexts* (list))      ; hierarchy of "testing" strings

(def ^:dynamic *test-out* *out*)               ; writer target for test output

(defmacro with-test-out
  "Runs body with *out* bound to the value of *test-out*."
  [& body]
  `(binding [*out* *test-out*]
     ~@body))

;;; UTILITIES FOR REPORTING FUNCTIONS

(defn file-position
  "Deprecated: stacktrace-based location info is not available in this
   runtime.  Returns [nil nil]."
  [_n]
  [nil nil])

(defn testing-vars-str
  "Returns a string representation of the current test.  Renders names in
   *testing-vars* as a list, then the source file and line if available."
  [m]
  (let [{:keys [file line]} m]
    (str
      (reverse (map (fn [v] (:name (meta v))) *testing-vars*))
      " (" file ":" line ")")))

(defn testing-contexts-str
  "Returns a string representation of the current test context.  Joins
   strings in *testing-contexts* with spaces."
  []
  (apply str (interpose " " (reverse *testing-contexts*))))

(defn inc-report-counter
  "Increments the named counter in *report-counters*, a ref to a map.
   Does nothing if *report-counters* is nil."
  [name]
  (when *report-counters*
    (dosync (commute *report-counters* update-in [name] (fnil inc 0)))))

;;; TEST RESULT REPORTING

(defmulti report
  "Generic reporting function, may be overridden to plug in different report
   formats (e.g., pytest).  Assertions such as 'is' call 'report' to indicate
   results.  The argument given to 'report' will be a map with a :type key."
  :type)

(defn do-report
  "Add file and line information to a test result and call report.  If you
   are writing a custom assert-expr method, call this function to pass test
   results to report."
  [m]
  ;; We don't have JVM stack traces; assert-predicate / assert-any pass
  ;; whatever :file / :line they can derive from form metadata.  Forward m
  ;; to report as-is.
  (report m))

(defmethod report :default [m]
  (with-test-out (prn m)))

(defmethod report :pass [_m]
  (with-test-out (inc-report-counter :pass)))

(defmethod report :fail [m]
  (with-test-out
    (inc-report-counter :fail)
    (println "\nFAIL in" (testing-vars-str m))
    (when (seq *testing-contexts*) (println (testing-contexts-str)))
    (when-let [message (:message m)] (println message))
    (println "expected:" (pr-str (:expected m)))
    (println "  actual:" (pr-str (:actual m)))))

(defmethod report :error [m]
  (with-test-out
    (inc-report-counter :error)
    (println "\nERROR in" (testing-vars-str m))
    (when (seq *testing-contexts*) (println (testing-contexts-str)))
    (when-let [message (:message m)] (println message))
    (println "expected:" (pr-str (:expected m)))
    (println "  actual:" (str (:actual m)))))

(defmethod report :summary [m]
  (with-test-out
    (println "\nRan" (:test m) "tests containing"
             (+ (:pass m) (:fail m) (:error m)) "assertions.")
    (println (:fail m) "failures," (:error m) "errors.")))

(defmethod report :begin-test-ns [m]
  (with-test-out
    (println "\nTesting" (ns-name (:ns m)))))

;; Ignore these message types:
(defmethod report :end-test-ns   [_m])
(defmethod report :begin-test-var [_m])
(defmethod report :end-test-var   [_m])


;;; UTILITIES FOR ASSERTIONS

(defn get-possibly-unbound-var
  "Like var-get but returns nil if the var is unbound."
  [v]
  (try (var-get v)
       (catch builtins.Exception _ nil)))

(defn function?
  "Returns true if argument is a function or a symbol that resolves to a
   function (not a macro)."
  [x]
  (if (symbol? x)
    (when-let [v (resolve x)]
      (when-let [value (get-possibly-unbound-var v)]
        (and (fn? value)
             (not (:macro (meta v))))))
    (fn? x)))

(defn assert-predicate
  "Returns generic assertion code for any functional predicate.  The
   'expected' argument to 'report' will contain the original form, the
   'actual' argument will contain the form with all its sub-forms
   evaluated.  If the predicate returns false, the 'actual' form will be
   wrapped in (not …)."
  [msg form]
  (let [args (rest form)
        pred (first form)]
    `(let [values# (list ~@args)
           result# (apply ~pred values#)]
       (if result#
         (clojure.test/do-report {:type :pass, :message ~msg,
                     :expected '~form, :actual (cons '~pred values#)})
         (clojure.test/do-report {:type :fail, :message ~msg,
                     :expected '~form, :actual (list '~'not (cons '~pred values#))}))
       result#)))

(defn assert-any
  "Returns generic assertion code for any test, including macros or isolated
   symbols."
  [msg form]
  `(let [value# ~form]
     (if value#
       (clojure.test/do-report {:type :pass, :message ~msg,
                   :expected '~form, :actual value#})
       (clojure.test/do-report {:type :fail, :message ~msg,
                   :expected '~form, :actual value#}))
     value#))


;;; ASSERTION METHODS

(defmulti assert-expr
  (fn [_msg form]
    (cond
      (nil? form) :always-fail
      (seq? form) (first form)
      :else       :default)))

(defmethod assert-expr :always-fail [msg _form]
  ;; nil test: always fail
  `(clojure.test/do-report {:type :fail, :message ~msg}))

(defmethod assert-expr :default [msg form]
  (if (and (sequential? form) (function? (first form)))
    (assert-predicate msg form)
    (assert-any msg form)))

(defmethod assert-expr 'instance? [msg form]
  ;; Test if x is an instance of y.
  `(let [klass# ~(nth form 1)
         object# ~(nth form 2)]
     (let [result# (instance? klass# object#)]
       (if result#
         (clojure.test/do-report {:type :pass, :message ~msg,
                     :expected '~form, :actual (class object#)})
         (clojure.test/do-report {:type :fail, :message ~msg,
                     :expected '~form, :actual (class object#)}))
       result#)))

(defmethod assert-expr 'thrown? [msg form]
  ;; (is (thrown? c expr))
  ;; Asserts that evaluating expr throws an exception of class c.
  ;; Returns the exception thrown.
  (let [klass (second form)
        body (nthnext form 2)]
    `(try ~@body
          (clojure.test/do-report {:type :fail, :message ~msg,
                      :expected '~form, :actual nil})
          (catch ~klass e#
            (clojure.test/do-report {:type :pass, :message ~msg,
                        :expected '~form, :actual e#})
            e#))))

(defmethod assert-expr 'thrown-with-msg? [msg form]
  ;; (is (thrown-with-msg? c re expr))
  ;; Asserts that evaluating expr throws an exception of class c and that
  ;; (str e) matches the regex re via re-find.
  (let [klass (nth form 1)
        re    (nth form 2)
        body  (nthnext form 3)]
    `(try ~@body
          (clojure.test/do-report {:type :fail, :message ~msg, :expected '~form, :actual nil})
          (catch ~klass e#
            (let [m# (str e#)]
              (if (re-find ~re m#)
                (clojure.test/do-report {:type :pass, :message ~msg,
                            :expected '~form, :actual e#})
                (clojure.test/do-report {:type :fail, :message ~msg,
                            :expected '~form, :actual e#})))
            e#))))


(defmacro try-expr
  "Used by the 'is' macro to catch unexpected exceptions.  You don't call
   this."
  [msg form]
  `(try ~(clojure.test/assert-expr msg form)
        (catch builtins.Exception t#
          (clojure.test/do-report {:type :error, :message ~msg,
                      :expected '~form, :actual t#}))))


;;; ASSERTION MACROS

(defmacro is
  "Generic assertion macro.  'form' is any predicate test.
   'msg' is an optional message to attach to the assertion.

   Example: (is (= 4 (+ 2 2)) \"Two plus two should be 4\")

   Special forms:

   (is (thrown? c body))  — checks that an instance of c is thrown from
   body, fails if not; returns the thing thrown.

   (is (thrown-with-msg? c re body)) — thrown? plus checks that (str e)
   matches re via re-find."
  ([form] `(is ~form nil))
  ([form msg] `(clojure.test/try-expr ~msg ~form)))

(defmacro are
  "Checks multiple assertions with a template expression.  See
   clojure.template/do-template for an explanation of templates.

   Example: (are [x y] (= x y)  2 (+ 1 1)  4 (* 2 2))

   Expands to:
     (do (is (= 2 (+ 1 1)))
         (is (= 4 (* 2 2))))

   Note: This breaks some reporting features, such as line numbers."
  [argv expr & args]
  (if (or
        ;; (are [] true) is meaningless but ok
        (and (empty? argv) (empty? args))
        ;; Catch wrong number of args
        (and (pos? (count argv))
             (pos? (count args))
             (zero? (mod (count args) (count argv)))))
    `(clojure.template/do-template ~argv (clojure.test/is ~expr) ~@args)
    (throw (clojure._core/IllegalArgumentException "The number of args doesn't match are's argv."))))

(defmacro testing
  "Adds a new string to the list of testing contexts.  May be nested, but
   must occur inside a test function (deftest)."
  [string & body]
  `(binding [clojure.test/*testing-contexts*
             (conj clojure.test/*testing-contexts* ~string)]
     ~@body))


;;; DEFINING TESTS

(defmacro with-test
  "Takes any definition form (that returns a Var) as the first argument.
   Remaining body goes in the :test metadata function for that Var.

   When *load-tests* is false, only evaluates the definition, ignoring
   the tests."
  [definition & body]
  (if *load-tests*
    `(doto ~definition
       (alter-meta! (fn [m#] (assoc (or m# {}) :test (fn [] ~@body)))))
    definition))

(defmacro deftest
  "Defines a test function with no arguments.  Test functions may call
   other tests, so tests may be composed.  If you compose tests, you
   should also define a function named test-ns-hook; run-tests will call
   test-ns-hook instead of testing all vars.

   Note: the test body goes in the :test metadata on the var, and the
   real function (the value of the var) calls test-var on itself.

   When *load-tests* is false, deftest is ignored."
  [name & body]
  (when *load-tests*
    `(do
       (def ~name (fn [] (clojure.test/test-var (var ~name))))
       (alter-meta! (var ~name)
                    (fn [m#] (assoc (or m# {}) :test (fn [] ~@body))))
       (var ~name))))

(defmacro deftest-
  "Like deftest but creates a private var."
  [name & body]
  (when *load-tests*
    `(do
       (def ~name (fn [] (clojure.test/test-var (var ~name))))
       (alter-meta! (var ~name)
                    (fn [m#] (assoc (or m# {}) :test (fn [] ~@body) :private true)))
       (var ~name))))

(defmacro set-test
  "Experimental.  Sets :test metadata of the named var to a fn with the
   given body.  The var must already exist.  Does not modify the value of
   the var.

   When *load-tests* is false, set-test is ignored."
  [name & body]
  (when *load-tests*
    `(alter-meta! (var ~name)
                  (fn [m#] (assoc (or m# {}) :test (fn [] ~@body))))))


;;; DEFINING FIXTURES

(defn- add-ns-meta
  "Adds elements in coll to the current namespace metadata as the value of
   key."
  [key coll]
  (alter-meta! *ns* (fn [m] (assoc (or m {}) key coll))))

(defmulti use-fixtures
  "Wrap test runs in a fixture function to perform setup and teardown.
   Using a fixture-type of :each wraps every test individually, while
   :once wraps the whole run in a single function."
  (fn [fixture-type & _args] fixture-type))

(defmethod use-fixtures :each [_fixture-type & args]
  (add-ns-meta ::each-fixtures args))

(defmethod use-fixtures :once [_fixture-type & args]
  (add-ns-meta ::once-fixtures args))

(defn- default-fixture
  "The default, empty, fixture function.  Just calls its argument."
  [f] (f))

(defn compose-fixtures
  "Composes two fixture functions, creating a new fixture function that
   combines their behavior."
  [f1 f2]
  (fn [g] (f1 (fn [] (f2 g)))))

(defn join-fixtures
  "Composes a collection of fixtures, in order.  Always returns a valid
   fixture function, even if the collection is empty."
  [fixtures]
  (reduce compose-fixtures default-fixture fixtures))


;;; RUNNING TESTS: LOW-LEVEL FUNCTIONS

(defn ^:dynamic test-var
  "If v has a function in its :test metadata, calls that function, with
   *testing-vars* bound to (conj *testing-vars* v)."
  [v]
  (when-let [t (:test (meta v))]
    (binding [*testing-vars* (conj *testing-vars* v)]
      (clojure.test/do-report {:type :begin-test-var, :var v})
      (inc-report-counter :test)
      (try (t)
           (catch builtins.Exception e
             (clojure.test/do-report {:type :error, :message "Uncaught exception, not in assertion."
                         :expected nil, :actual e})))
      (clojure.test/do-report {:type :end-test-var, :var v}))))

(defn test-vars
  "Groups vars by their namespace and runs test-var on them with
   appropriate fixtures applied."
  [vars]
  (doseq [[ns vars] (group-by (comp :ns meta) vars)]
    (let [once-fixture-fn (join-fixtures (::once-fixtures (meta ns)))
          each-fixture-fn (join-fixtures (::each-fixtures (meta ns)))]
      (once-fixture-fn
        (fn []
          (doseq [v vars]
            (when (:test (meta v))
              (each-fixture-fn (fn [] (test-var v))))))))))

(defn test-all-vars
  "Calls test-vars on every var interned in the namespace, with fixtures."
  [ns]
  (test-vars (vals (ns-interns ns))))

(defn test-ns
  "If the namespace defines a function named test-ns-hook, calls that.
   Otherwise, calls test-all-vars on the namespace.  'ns' is a namespace
   object or a symbol.

   Internally binds *report-counters* to a ref initialized to
   *initial-report-counters*.  Returns the final, dereferenced state of
   *report-counters*."
  [ns]
  (binding [*report-counters* (ref *initial-report-counters*)]
    (let [ns-obj (the-ns ns)]
      (clojure.test/do-report {:type :begin-test-ns, :ns ns-obj})
      ;; If the namespace has a test-ns-hook function, call that:
      (if-let [v (find-var (symbol (str (ns-name ns-obj)) "test-ns-hook"))]
        ((var-get v))
        ;; Otherwise, just test every var in the namespace.
        (test-all-vars ns-obj))
      (clojure.test/do-report {:type :end-test-ns, :ns ns-obj}))
    @*report-counters*))


;;; RUNNING TESTS: HIGH-LEVEL FUNCTIONS

(defn run-tests
  "Runs all tests in the given namespaces; prints results.  Defaults to
   current namespace if none given.  Returns a map summarizing test
   results."
  ([] (run-tests *ns*))
  ([& namespaces]
   (let [summary (assoc (apply merge-with + (map test-ns namespaces))
                        :type :summary)]
     (clojure.test/do-report summary)
     summary)))

(defn run-all-tests
  "Runs all tests in all namespaces; prints results.  Optional argument
   is a regular expression; only namespaces with names matching the regex
   (with re-matches) will be tested."
  ([] (apply run-tests (all-ns)))
  ([re] (apply run-tests (filter (fn [n] (re-matches re (name (ns-name n)))) (all-ns)))))

(defn successful?
  "Returns true if the given test summary indicates all tests were
   successful, false otherwise."
  [summary]
  (and (zero? (:fail summary 0))
       (zero? (:error summary 0))))

(defn run-test-var
  "Runs the tests for a single Var, with fixtures executed around the test,
   and summary output after."
  [v]
  (binding [*report-counters* (ref *initial-report-counters*)]
    (let [ns-obj  (-> v meta :ns)
          summary (do
                    (clojure.test/do-report {:type :begin-test-ns :ns ns-obj})
                    (test-vars [v])
                    (clojure.test/do-report {:type :end-test-ns   :ns ns-obj})
                    (assoc @*report-counters* :type :summary))]
      (clojure.test/do-report summary)
      summary)))

(defmacro run-test
  "Runs a single test.  No test-ns-hook is honored."
  [test-symbol]
  (let [v (resolve test-symbol)]
    (cond
      (nil? v)
      (binding [*out* *err*]
        (println "Unable to resolve" test-symbol "to a test function."))

      (not (-> v meta :test))
      (binding [*out* *err*]
        (println test-symbol "is not a test."))

      :else
      `(run-test-var ~v))))
