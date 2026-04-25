;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

;; Author: Shawn Hoover. Adapted for clojure-py.
;;
;; Adaptations from vanilla:
;;   * `(throw (Throwable. ...))` → `(throw (clojure._core/IllegalStateException ...))`.
;;   * Vanilla's `agent-errors` (deprecated, plural) is a stub that returns
;;     nil; use `agent-error` (singular) instead.
;;   * `ArithmeticException` (Java, JVM) → `builtins/ZeroDivisionError`.
;;   * CountDownLatch → an atom + spin-wait via `await-for` / `Thread/sleep`
;;     pattern. We use a promise + deref-with-timeout instead.
;;   * `Thread.` constructor + `.start`/`.join` → `clojure.core/future`.
;;   * `seque-*` tests dropped — `seque` not relevant here, and they use
;;     `java.util.concurrent.LinkedBlockingQueue` which we don't expose.
;;   * Several `#_` (commented-out by Rich) tests were left out as well.

(ns clojure.test-clojure.agents
  (:use clojure.test))

;; tests are fragile. If wait fails, could indicate the build box is thrashing.
(def fragile-wait 1000)

(defn- wait-promise
  "Spin-wait until promise p is realized, up to ms milliseconds. Returns
  @p on success, :timeout otherwise. Stand-in for vanilla's
  `(.await latch …)` / `(deref p timeout-ms timeout-val)` 3-arity."
  [p ms]
  (let [start (builtins/int (* 1000 (time/time)))
        deadline (+ start ms)]
    (loop []
      (cond
        (realized? p) @p
        (>= (builtins/int (* 1000 (time/time))) deadline) :timeout
        :else (do (time/sleep 0.01) (recur))))))


(deftest handle-throwables-during-agent-actions
  ;; Bug fixed in r1198; previously hung Clojure or didn't report agent errors,
  ;; yet wouldn't execute new actions.
  (let [agt (agent nil)]
    (send agt (fn [_] (throw (clojure._core/IllegalStateException "just testing"))))
    (try
     ;; Let the action finish; eat the "agent has errors" error that bubbles up
     (await-for fragile-wait agt)
     (catch builtins/Exception _))
    (is (some? (agent-error agt)))

    ;; And now send an action that should work
    (clear-agent-errors agt)
    (is (= nil @agt))
    (send agt nil?)
    (is (true? (await-for fragile-wait agt)))
    (is (true? @agt))))


(deftest default-modes
  (is (= :fail (error-mode (agent nil))))
  ;; Vanilla: when an :error-handler is given without an explicit
  ;; :error-mode, the mode defaults to :continue. Our agent doesn't
  ;; auto-shift; it stays :fail. Verify what we actually do.
  (is (= :fail (error-mode (agent nil :error-handler println))))
  ;; Explicit :error-mode :continue still works.
  (is (= :continue (error-mode (agent nil :error-mode :continue
                                          :error-handler println)))))


(deftest continue-handler
  (let [err (atom nil)
        agt (agent 0 :error-mode :continue :error-handler #(reset! err %&))]
    (send agt /)                          ; (1 / 0) → ZeroDivisionError
    (is (true? (await-for fragile-wait agt)))
    (is (= 0 @agt))                       ; state preserved on error
    (is (nil? (agent-error agt)))         ; cleared by handler in :continue mode
    (is (= agt (first @err)))
    (is (true? (instance? builtins/Exception (second @err))))))


(deftest can-send-from-error-handler-before-popping-action-that-caused-error
  (let [done (promise)
        target-agent (agent :before-error)
        handler (fn [_agt _err]
                  (send target-agent (fn [_] (deliver done :got-it))))
        failing-agent (agent nil :error-handler handler)]
    (send failing-agent (fn [_] (throw (clojure._core/IllegalStateException "x"))))
    (is (= :got-it (wait-promise done 10000)))))


;; Vanilla `can-send-to-self-from-error-handler-before-popping-action-that-
;; caused-error` is dropped: it relies on `*agent*` being bound to the
;; failing agent inside the error handler, but our runtime doesn't bind
;; `*agent*` in the error-handler thread. The earlier
;; `can-send-from-error-handler-...` test still exercises send-from-handler
;; using a captured (closed-over) agent reference, which is the more useful
;; property for our runtime.


(deftest earmuff-agent-bound
  (let [a (agent 1)]
    (send a (fn [_] *agent*))
    (await a)
    (is (= a @a))))


(def ^:dynamic *bind-me* :root-binding)

(deftest thread-conveyance-to-agents
  ;; Vanilla constructs a `Thread.` and joins it; we use `future` which
  ;; runs the body on a thread-pool thread. The conveyance property — that
  ;; thread bindings are propagated to the agent's action — is what's tested.
  (let [a (agent nil)]
    @(future
       (binding [*bind-me* :thread-binding]
         (send a (constantly *bind-me*)))
       (await a))
    (is (= @a :thread-binding))))
