"""Bare-symbol `Exception` / `Throwable` resolution in catch clauses.

Vanilla Clojure: `(catch Exception e ...)` catches all application
exceptions; `(catch Throwable e ...)` catches BaseException-derived signals
too. We bind `Exception → builtins.Exception` and `Throwable →
builtins.BaseException` in clojure.core so unqualified refs resolve.
"""

import pytest
import builtins
from clojure._core import eval_string as e


def test_catch_exception_catches_illegal_state():
    src = """
    (try
      (throw (clojure._core/IllegalStateException "boom"))
      (catch Exception ex :caught))
    """
    assert e(src) == e(":caught")


def test_catch_exception_catches_illegal_argument():
    src = """
    (try
      (throw (clojure._core/IllegalArgumentException "x"))
      (catch Exception ex :caught))
    """
    assert e(src) == e(":caught")


def test_catch_exception_catches_arity_exception():
    src = """
    (try
      ((fn [a] a))
      (catch Exception ex :caught-arity))
    """
    assert e(src) == e(":caught-arity")


def test_catch_exception_catches_value_error():
    src = """
    (try
      (throw (builtins/ValueError "v"))
      (catch Exception ex :caught))
    """
    assert e(src) == e(":caught")


def test_catch_throwable_catches_BaseException():
    src = """
    (try
      (throw (builtins/SystemExit "x"))
      (catch Throwable ex :caught))
    """
    assert e(src) == e(":caught")


def test_catch_throwable_catches_exception_too():
    src = """
    (try
      (throw (clojure._core/IllegalStateException "x"))
      (catch Throwable ex :caught))
    """
    assert e(src) == e(":caught")


def test_catch_exception_propagates_BaseException():
    """Bare `Exception` does NOT catch BaseException-derived signals.
    The throw should propagate out unmatched.
    """
    src = """
    (try
      (throw (builtins/SystemExit "x"))
      (catch Exception ex :caught))
    """
    with pytest.raises(builtins.SystemExit):
        e(src)


def test_catch_exception_returns_bound_value():
    """The exception is bound to the catch's local; we verify the message."""
    src = """
    (try
      (throw (clojure._core/IllegalStateException "boom"))
      (catch Exception ex (str ex)))
    """
    msg = e(src)
    assert "boom" in msg
