"""Tests for core.clj batch 37: futures + pmap (JVM 7113-7200).

Adaptations from JVM:
  - JVM uses `proxy [Future IDeref IBlockingDeref IPending] []` to
    return a Java object implementing all four interfaces. We don't
    need that — Python's concurrent.futures.Future already exposes
    .result / .done / .cancel / .cancelled. future-call returns the
    raw Future from the agent solo-executor pool.
  - `deref` is redefined here to be multi-arity, dispatching IDeref
    instances to .deref and falling through to deref-future for
    Futures. Mirrors JVM's deref redef in the same section.
  - `realized?` checks IPending (delays/promises/lazy seqs) AND raw
    concurrent.futures.Future (.done()).
  - future-cancel ignores the JVM `mayInterruptIfRunning` arg —
    Python Future.cancel takes none and only cancels not-yet-running
    submissions.
  - pmap reads (py.os/cpu_count) instead of
    Runtime.getRuntime().availableProcessors().

Sub-batch B fix shaken out: extend now accepts a class as its
"protocol" arg and registers it via .register, instead of treating
non-protocols as an error. This let us extend reify over
clojure.lang.IDeref (a Cython ABC, not a Clojure protocol).
"""

import time as _time
import concurrent.futures as _cf

import pytest

import clojure.core  # bootstrap

from clojure.lang import (
    Compiler,
    read_string,
    Keyword,
)


def E(src):
    return Compiler.eval(read_string(src))


def K(name):
    return Keyword.intern(None, name)


# --- future / future-call basics ---------------------------------

def test_future_returns_value():
    assert E("@(future 42)") == 42

def test_future_call_returns_value():
    assert E("@(future-call (fn [] (* 7 6)))") == 42

def test_future_runs_in_other_thread():
    """The body runs on a different thread than the caller."""
    out = E("""
      (let [main-thread (.. (py.threading/current_thread) -ident)]
        @(future
           (let [worker-thread (.. (py.threading/current_thread) -ident)]
             (not= main-thread worker-thread))))""")
    assert out is True

def test_future_returns_python_future_instance():
    """future-call returns the raw concurrent.futures.Future."""
    fut = E("(future 1)")
    assert isinstance(fut, _cf.Future)


# --- deref / @ ---------------------------------------------------

def test_deref_blocks_until_done():
    """deref blocks the caller until the future completes."""
    out = E("""
      (let [f (future (do (py.time/sleep 0.05) :done))]
        (deref f))""")
    assert out == K("done")

def test_deref_with_timeout_returns_timeout_val():
    """deref with timeout returns the timeout-val if not realized in time."""
    out = E("""
      (let [f (future (do (py.time/sleep 0.2) :slow))]
        (deref f 30 :timed-out))""")
    assert out == K("timed-out")

def test_deref_with_timeout_returns_value_when_done():
    """If completion is faster than the timeout, we get the value."""
    out = E("""
      (let [f (future :fast)]
        (py.time/sleep 0.05)
        (deref f 1000 :nope))""")
    assert out == K("fast")

def test_deref_atom_still_works():
    """The deref redef must still handle non-Future IDeref instances."""
    out = E("(let [a (atom 42)] (deref a))")
    assert out == 42


# --- realized? ---------------------------------------------------

def test_realized_pred_for_done_future():
    out = E("""
      (let [f (future :v)]
        (deref f)
        (realized? f))""")
    assert out is True

def test_realized_pred_for_pending_future():
    out = E("""
      (let [f (future (do (py.time/sleep 0.5) :late))]
        (realized? f))""")
    assert out is False

def test_realized_pred_for_delay():
    """realized? on a delay: false until forced, true after."""
    E("(def -tcb-d1 (delay :forced))")
    assert E("(realized? -tcb-d1)") is False
    E("@-tcb-d1")
    assert E("(realized? -tcb-d1)") is True


# --- future-done? / future-cancelled? / future? -----------------

def test_future_done_after_completion():
    out = E("""
      (let [f (future :v)]
        (deref f)
        (future-done? f))""")
    assert out is True

def test_future_done_before_completion():
    out = E("""
      (let [f (future (do (py.time/sleep 0.5) :late))]
        (future-done? f))""")
    assert out is False

def test_future_cancelled_default_false():
    out = E("""
      (let [f (future :v)]
        (deref f)
        (future-cancelled? f))""")
    assert out is False

def test_future_pred_true_for_future_call_result():
    out = E("(future? (future :v))")
    assert out is True

def test_future_pred_false_for_other():
    assert E("(future? 42)") is False
    assert E("(future? nil)") is False


# --- future-cancel ----------------------------------------------

def test_future_cancel_returns_bool():
    """future-cancel returns True/False depending on cancellation success.
    Python Future.cancel() can only cancel not-yet-running futures."""
    # Submit a long task to fill the executor, then queue another that
    # we'll try to cancel before it starts.
    out = E("""
      (let [blocker (future (py.time/sleep 0.2))
            target (future :never)]
        (future-cancel target))""")
    # Result may be True (queued, cancellable) or False (already running).
    # We just check it returned a bool.
    assert isinstance(out, bool)


# --- pmap --------------------------------------------------------

def test_pmap_single_coll():
    out = E("(pmap inc [1 2 3 4 5])")
    assert list(out) == [2, 3, 4, 5, 6]

def test_pmap_two_colls():
    out = E("(pmap + [1 2 3] [10 20 30])")
    assert list(out) == [11, 22, 33]

def test_pmap_three_colls():
    out = E("(pmap + [1 2 3] [10 20 30] [100 200 300])")
    assert list(out) == [111, 222, 333]

def test_pmap_uneven_truncates():
    out = E("(pmap + [1 2 3 4] [10 20])")
    assert list(out) == [11, 22]

def test_pmap_actually_parallel():
    """If pmap is parallel, the total wall-time should be much less
    than the sequential sum."""
    import time
    E("(def -tcb-slow-fn (fn [x] (do (py.time/sleep 0.1) (* x 2))))")
    start = time.monotonic()
    out = E("(doall (pmap -tcb-slow-fn [1 2 3 4]))")
    elapsed = time.monotonic() - start
    # 4 items × 0.1s sequential = 0.4s. Parallel should be well under.
    assert elapsed < 0.3
    assert list(out) == [2, 4, 6, 8]


# --- pcalls / pvalues -------------------------------------------

def test_pcalls():
    out = E("(pcalls (fn [] 1) (fn [] 2) (fn [] 3))")
    assert list(out) == [1, 2, 3]

def test_pcalls_empty():
    """No fns → empty seq."""
    out = E("(pcalls)")
    assert list(out) == []

def test_pvalues():
    out = E("(pvalues (* 1 1) (* 2 2) (* 3 3))")
    assert list(out) == [1, 4, 9]

def test_pvalues_lazy_evaluation():
    """pvalues should run all the exprs in parallel (eagerly via pcalls)."""
    out = E("""
      (let [counter (atom 0)
            inc-counter (fn [v] (swap! counter inc) v)]
        (doall (pvalues (inc-counter :a)
                        (inc-counter :b)
                        (inc-counter :c)))
        (deref counter))""")
    assert out == 3


# --- binding-conveyor: dynamic var propagation ------------------

def test_future_propagates_dynamic_bindings():
    """A binding established before (future ...) should be visible
    inside the future's thread."""
    E("(def ^:dynamic *tcb-test-var* :default)")
    out = E("""
      (binding [*tcb-test-var* :overridden]
        @(future *tcb-test-var*))""")
    assert out == K("overridden")


# --- reify with ABCs (covered by future-call's machinery) ------

def test_reify_with_ideref_abc():
    """reify over a Cython ABC (clojure.lang.IDeref) registers the
    class via .register and the methods via Python class attrs.
    Validates the extend fix that lets non-protocol classes be passed
    to extend / extend-type / reify."""
    E("""(def -tcb-r2
           (reify clojure.lang.IDeref
             (deref [_] :hello)))""")
    assert E("(instance? clojure.lang.IDeref -tcb-r2)") is True
    assert E("(deref -tcb-r2)") == K("hello")
    assert E("@-tcb-r2") == K("hello")
