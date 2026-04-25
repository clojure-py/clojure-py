"""Agent tests — basic dispatch, await, error handling, binding conveyance,
and in-transaction send deferral.

Most tests use `(def --name (agent ...))` to create a named var that the
main thread and any worker threads can reference by name.
"""

import time
import threading
import pytest

from clojure._core import (
    eval_string,
    Agent,
    IllegalStateException,
    IllegalArgumentException,
)


def _ev(s):
    return eval_string(s)


# --- Basics ---


def test_agent_deref():
    a = _ev("(agent 42)")
    assert isinstance(a, Agent)
    assert _ev("@(agent 42)") == 42


def test_agent_repr():
    a = _ev("(agent 99)")
    assert repr(a) == "#<Agent 99>"


def test_send_returns_agent():
    _ev("(def --t1-ag (agent 0))")
    result = _ev("(send --t1-ag inc)")
    assert isinstance(result, Agent)
    _ev("(await --t1-ag)")
    assert _ev("@--t1-ag") == 1


def test_send_via_sync_executor():
    # A "synchronous" executor runs the action on the caller's thread
    # immediately. After send-via returns, the action should already have
    # applied.
    import sys
    calls = []
    def sync_exec(thunk):
        calls.append("before")
        thunk()
        calls.append("after")
    _ev("(def --sv-sync-exec nil)")
    sys.modules["clojure.user"].__dict__["--sv-sync-exec"].bind_root(sync_exec)
    _ev("(def --sv-sync-ag (agent 0))")
    _ev("(send-via --sv-sync-exec --sv-sync-ag inc)")
    assert _ev("@--sv-sync-ag") == 1
    assert calls == ["before", "after"]


def test_send_via_threaded_executor():
    # An async executor runs the thunk on a fresh thread. Requires an await.
    import sys, threading
    def thread_exec(thunk):
        threading.Thread(target=thunk).start()
    _ev("(def --sv-thr-exec nil)")
    sys.modules["clojure.user"].__dict__["--sv-thr-exec"].bind_root(thread_exec)
    _ev("(def --sv-thr-ag (agent 0))")
    _ev("(send-via --sv-thr-exec --sv-thr-ag inc)")
    _ev("(await --sv-thr-ag)")
    assert _ev("@--sv-thr-ag") == 1


def test_send_off_returns_agent():
    _ev("(def --t1b-ag (agent 0))")
    result = _ev("(send-off --t1b-ag inc)")
    assert isinstance(result, Agent)
    _ev("(await --t1b-ag)")
    assert _ev("@--t1b-ag") == 1


def test_action_ordering_single_sender():
    _ev("(def --ord (agent []))")
    for i in range(10):
        _ev("(send --ord conj %d)" % i)
    _ev("(await --ord)")
    result = list(_ev("@--ord"))
    assert result == list(range(10))


def test_send_with_args():
    _ev("(def --t2-ag (agent 100))")
    _ev("(send --t2-ag + 5 6 7)")
    _ev("(await --t2-ag)")
    assert _ev("@--t2-ag") == 118


def test_multiple_sends_compose():
    _ev("(def --t3-ag (agent 0))")
    for _ in range(50):
        _ev("(send --t3-ag inc)")
    _ev("(await --t3-ag)")
    assert _ev("@--t3-ag") == 50


# --- await / await-for ---


def test_await_blocks_until_drained():
    _ev("(def --aw-ag (agent 0))")
    for _ in range(10):
        _ev("(send --aw-ag inc)")
    _ev("(await --aw-ag)")
    # await returned synchronously only after all pending actions completed.
    assert _ev("@--aw-ag") == 10


def test_await_for_returns_true_on_completion():
    _ev("(def --aw2-ag (agent 0))")
    for _ in range(5):
        _ev("(send --aw2-ag inc)")
    ok = _ev("(await-for 5000 --aw2-ag)")
    assert ok is True
    assert _ev("@--aw2-ag") == 5


def test_await_multiple_agents():
    _ev("(def --aw3a (agent 0))")
    _ev("(def --aw3b (agent 0))")
    for _ in range(5):
        _ev("(send --aw3a inc)")
        _ev("(send --aw3b inc)")
    _ev("(await --aw3a --aw3b)")
    assert _ev("@--aw3a") == 5
    assert _ev("@--aw3b") == 5


# --- Error modes ---


def test_fail_mode_parks_error():
    _ev("(def --err1 (agent 0))")
    # Force an error inside the action: divide by zero.
    _ev("(send --err1 (fn* [_] (/ 1 0)))")
    _ev("(await-for 2000 --err1)")  # may return False if the agent failed
    err = _ev("(agent-error --err1)")
    assert err is not None
    # Subsequent send raises because agent is failed.
    with pytest.raises(IllegalStateException):
        _ev("(send --err1 inc)")


def test_restart_agent_clears_error():
    _ev("(def --err2 (agent 0))")
    _ev("(send --err2 (fn* [_] (/ 1 0)))")
    _ev("(await-for 2000 --err2)")
    assert _ev("(agent-error --err2)") is not None
    _ev("(restart-agent --err2 99)")
    assert _ev("(agent-error --err2)") is None
    assert _ev("@--err2") == 99


def test_restart_with_clear_actions():
    _ev("(def --err3 (agent 0))")
    # Trigger failure then queue more actions while failed.
    _ev("(send --err3 (fn* [_] (/ 1 0)))")
    _ev("(await-for 2000 --err3)")
    # restart-agent with :clear-actions discards any queued held actions.
    _ev("(restart-agent --err3 0 :clear-actions true)")
    _ev("(send --err3 inc)")
    _ev("(await --err3)")
    assert _ev("@--err3") == 1


def test_continue_mode_with_handler_keeps_running():
    _ev("(def --err4 (agent 0))")
    _ev("(set-error-mode! --err4 :continue)")
    # Handler just records that it was called.
    _ev("(def --err4-handler-calls (atom 0))")
    _ev("(set-error-handler! --err4 (fn* [a e] (swap! --err4-handler-calls inc)))")
    _ev("(send --err4 (fn* [_] (/ 1 0)))")
    _ev("(send --err4 inc)")  # This should still run in :continue mode.
    _ev("(await-for 2000 --err4)")
    assert _ev("(agent-error --err4)") is None  # no parked error in :continue
    assert _ev("@--err4-handler-calls") >= 1
    # The inc should have applied to the initial 0.
    assert _ev("@--err4") == 1


def test_error_mode_default_is_fail():
    _ev("(def --em1 (agent 0))")
    assert _ev("(error-mode --em1)") == _ev(":fail")


def test_error_mode_default_continue_when_handler_given_at_ctor():
    # Vanilla rule: if :error-handler is supplied but :error-mode is not,
    # default mode is :continue.
    _ev("(def --em2 (agent 0 :error-handler (fn* [a e] nil)))")
    assert _ev("(error-mode --em2)") == _ev(":continue")


# --- Validator ---


def test_agent_validator_rejects():
    _ev("(def --v1 (agent 0 :validator (fn* [x] (>= x 0))))")
    _ev("(send --v1 (fn* [_] -1))")
    _ev("(await-for 2000 --v1)")
    # In :fail mode, validator failure parks an error.
    assert _ev("(agent-error --v1)") is not None


def test_agent_validator_accepts():
    _ev("(def --v3 (agent 1 :validator (fn* [x] (> x 0))))")
    _ev("(send --v3 inc)")
    _ev("(await --v3)")
    assert _ev("@--v3") == 2


# --- Watches ---


def test_agent_watch_fires():
    _ev("(def --w1 (agent 0))")
    _ev("(def --w1-calls (atom []))")
    _ev("(add-watch --w1 :k (fn* [k a o n] (swap! --w1-calls conj [o n])))")
    _ev("(send --w1 inc)")
    _ev("(await --w1)")
    calls = list(_ev("@--w1-calls"))
    # one (old=0, new=1) pair recorded
    assert len(calls) == 1
    pair = list(calls[0])
    assert pair == [0, 1]


# --- *agent* binding ---


def test_agent_star_bound_in_action():
    # Inside an action, *agent* should be bound to the current agent.
    _ev("(def --star-probe (agent nil))")
    _ev("(def --star-captured (atom nil))")
    _ev("(send --star-probe (fn* [_] (reset! --star-captured *agent*) :done))")
    _ev("(await --star-probe)")
    captured = _ev("@--star-captured")
    assert isinstance(captured, Agent)
    assert captured is _ev("--star-probe")


# --- Binding conveyance ---


def test_binding_conveyance():
    # Dynamic binding on calling thread must be visible inside the action.
    _ev("(def ^:dynamic --conv-var :outer)")
    _ev("(def --conv-ag (agent nil))")
    _ev("(def --conv-captured (atom nil))")
    _ev(
        "(binding [--conv-var :inner] "
        "  (send --conv-ag (fn* [_] (reset! --conv-captured --conv-var))) "
        "  (await --conv-ag))"
    )
    assert _ev("@--conv-captured") == _ev(":inner")


# --- In-transaction send deferral ---


def test_send_in_txn_deferred_until_commit():
    _ev("(def --tx-ag (agent 0))")
    _ev("(dosync (send --tx-ag inc))")
    _ev("(await --tx-ag)")
    assert _ev("@--tx-ag") == 1


def test_send_in_failed_txn_not_dispatched():
    _ev("(def --tx2-ag (agent 0))")
    with pytest.raises(Exception):
        _ev("(dosync (send --tx2-ag inc) (/ 1 0))")
    # Give any (erroneously) dispatched action time to run.
    time.sleep(0.05)
    # Should not have incremented.
    assert _ev("@--tx2-ag") == 0


def test_send_in_txn_only_on_commit():
    # Multiple sends in a txn that commits — all dispatch, exactly once each.
    _ev("(def --tx3-ag (agent 0))")
    _ev("(dosync (send --tx3-ag inc) (send --tx3-ag inc) (send --tx3-ag inc))")
    _ev("(await --tx3-ag)")
    assert _ev("@--tx3-ag") == 3
