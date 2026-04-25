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
