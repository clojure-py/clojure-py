"""Agent error-mode auto-shift and *agent* binding inside the error handler."""

import time
import pytest

from clojure._core import eval_string


def _ev(s):
    return eval_string(s)


# ---------- :error-mode auto-shift ----------

def test_no_handler_default_is_fail():
    assert _ev("(error-mode (agent nil))") == _ev(":fail")


def test_handler_alone_defaults_to_continue():
    """Vanilla: :error-handler implies :error-mode :continue when not explicit."""
    src = """
    (let [h (fn [_a _e])]
      (error-mode (agent nil :error-handler h)))
    """
    assert _ev(src) == _ev(":continue")


def test_explicit_fail_with_handler_stays_fail():
    src = """
    (let [h (fn [_a _e])]
      (error-mode (agent nil :error-mode :fail :error-handler h)))
    """
    assert _ev(src) == _ev(":fail")


def test_explicit_continue_with_handler_stays_continue():
    src = """
    (let [h (fn [_a _e])]
      (error-mode (agent nil :error-mode :continue :error-handler h)))
    """
    assert _ev(src) == _ev(":continue")


def test_explicit_mode_no_handler():
    """Explicit :error-mode :continue without handler is honored."""
    assert _ev("(error-mode (agent nil :error-mode :continue))") == _ev(":continue")


def test_options_in_either_order_handler_first():
    """Order of options shouldn't matter for the auto-shift."""
    src = """
    (let [h (fn [_a _e])]
      (error-mode (agent nil :error-handler h :error-mode :fail)))
    """
    assert _ev(src) == _ev(":fail")


# ---------- *agent* binding inside error handler ----------

def test_agent_star_bound_inside_error_handler():
    """Vanilla: *agent* refers to the failing agent inside the handler thread.

    The handler captures *agent* into an atom; we read it on the main thread
    after `await` ensures the action and handler have run.
    """
    src = """
    (let [captured (atom nil)
          handler  (fn [_agt _err] (reset! captured *agent*))
          a        (agent nil :error-handler handler)]
      (send a (fn [_] (throw (clojure._core/IllegalStateException "boom"))))
      (await a)
      [a @captured])
    """
    pair = _ev(src)
    a, captured = pair[0], pair[1]
    assert captured is a, "*agent* in handler should be the failing agent"


def test_send_to_star_agent_from_handler():
    """Restored vanilla test: handler can `send *agent*` to the failing agent.

    The handler closes over `done`, sends a function to *agent* that delivers
    `done`. We assert the delivery completes within timeout.
    """
    src = """
    (let [done    (promise)
          handler (fn [_agt _err]
                    (send *agent* (fn [_] (deliver done :got-it))))
          a       (agent nil :error-handler handler)]
      (send a (fn [_] (throw (clojure._core/IllegalStateException "x"))))
      (await a)
      @done)
    """
    result = _ev(src)
    assert result == _ev(":got-it"), f"expected :got-it, got {result!r}"
