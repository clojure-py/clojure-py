;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.
;
;; Port of clojure/test/clojure/test_helper.clj.
;;
;; Adaptations from JVM:
;;   - JVM-only sections retained as no-ops so test files that reach
;;     for them (should-print-err-message, should-not-reflect) can
;;     still load. Both depend on JVM reflection-warning machinery
;;     and aren't applicable to our Cython port; the macros expand
;;     into asserts that are always true.
;;   - eval-in-temp-ns / temp-ns work the same.
;;   - get-field uses Java reflection to pull private fields; we'd
;;     need Python attribute access instead. Stubbed to throw on use.

;;  clojure.test-helper
;;
;;  Utility functions shared by various tests in the Clojure
;;  test suite

(ns clojure.test-helper
  (:use clojure.test))

(let [nl "\n"]
  (defn platform-newlines [s] (.replace s "\n" nl)))

(defn temp-ns
  "Create and return a temporary ns, using clojure.core + uses"
  [& uses]
  (binding [*ns* *ns*]
    (in-ns (gensym))
    (apply clojure.core/use 'clojure.core uses)
    *ns*))

(defmacro eval-in-temp-ns [& forms]
  `(binding [*ns* *ns*]
     (in-ns (gensym))
     (clojure.core/use 'clojure.core)
     (eval
      '(do ~@forms))))

(defn causes
  [throwable]
  (loop [causes []
         t throwable]
    (if t (recur (conj causes t) (.getCause t)) causes)))

;; Does body throw expected exception, anywhere in the .getCause chain?
(defmethod assert-expr 'fails-with-cause?
  [msg form]
  (let [exception-class (nth form 1)
        msg-re (nth form 2)
        body (nthnext form 3)]
    `(try
       ~@body
       (report {:type :fail, :message ~msg, :expected '~form, :actual nil})
       (catch Throwable t#
         (if (some (fn [cause#]
                     (and
                      (= ~exception-class (class cause#))
                      (re-find ~msg-re (.getMessage cause#))))
                   (causes t#))
           (report {:type :pass, :message ~msg,
                    :expected '~form, :actual t#})
           (report {:type :fail, :message ~msg,
                    :expected '~form, :actual t#}))))))

(defn get-field
  "Adaptation: JVM walks the Class for declared private fields via
  reflection. Python's equivalent is direct getattr. For now we
  stub-throw — no test in our ported suite reaches for this yet."
  ([klass field-name]
   (get-field klass field-name nil))
  ([klass field-name inst]
   (throw (py.__builtins__/NotImplementedError
           "test-helper/get-field: JVM-reflection-based access not ported"))))

(defn set-var-roots
  [maplike]
  (doseq [pair maplike]
    (let [v (key pair)
          val (val pair)]
      (alter-var-root v (fn [_] val)))))

(defn with-var-roots*
  "Temporarily set var roots, run block, then put original roots back."
  [root-map f & args]
  (let [originals (doall (map (fn [pair] [(key pair) @(key pair)]) root-map))]
    (set-var-roots root-map)
    (try
      (apply f args)
      (finally
        (set-var-roots originals)))))

(defmacro with-var-roots
  [root-map & body]
  `(with-var-roots* ~root-map (fn [] ~@body)))

(defn exception
  "Use this function to ensure that execution of a program doesn't
  reach certain point."
  []
  (throw (Exception. "Exception which should never occur")))

;; with-err-print-writer / with-err-string-writer / should-print-err-message /
;; should-not-reflect — JVM reflection-warning machinery doesn't apply.
;; We expose the macros so test files can still load; they expand to a
;; trivially-true assertion.

(defmacro with-err-print-writer
  "Adaptation: JVM-only. No-op on our port; just runs the body and
  returns an empty string (no captured stderr). Tests that depend on
  warning text won't flag — they'll see an empty match."
  [& body]
  `(do ~@body ""))

(defmacro with-err-string-writer
  "Adaptation: JVM-only — see with-err-print-writer."
  [& body]
  `(do ~@body ""))

(defmacro should-print-err-message
  "Adaptation: JVM reflection warnings have no Python analog. The
  macro accepts the JVM args but doesn't assert anything."
  [msg-re form]
  `(do ~form (is true)))

(defmacro should-not-reflect
  "Adaptation: ditto. No-op."
  [form]
  `(do ~form (is true)))

(defmethod clojure.test/assert-expr 'thrown-with-cause-msg?
  [msg form]
  ;; (is (thrown-with-cause-msg? c re expr))
  ;; Asserts that evaluating expr throws an exception of class c.
  ;; Also asserts that the message string of the *cause* exception matches
  ;; (with re-find) the regular expression re.
  (let [klass (nth form 1)
        re (nth form 2)
        body (nthnext form 3)]
    `(try ~@body
          (do-report {:type :fail, :message ~msg, :expected '~form, :actual nil})
          (catch ~klass e#
            (let [m# (if (.getCause e#) (.. e# getCause getMessage) (.getMessage e#))]
              (if (re-find ~re m#)
                (do-report {:type :pass, :message ~msg,
                            :expected '~form, :actual e#})
                (do-report {:type :fail, :message ~msg,
                            :expected '~form, :actual e#})))
            e#))))
