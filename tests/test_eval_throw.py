"""(throw expr) special form."""

import pytest
from clojure._core import eval_string, IllegalStateException


def _ev(src): return eval_string(src)


def test_throw_string_wraps_in_ise():
    # Throwing a non-exception value wraps it in IllegalStateException.
    with pytest.raises(IllegalStateException) as exc:
        _ev('(throw "boom")')
    assert "boom" in str(exc.value)


def test_throw_int_wraps_in_ise():
    with pytest.raises(IllegalStateException) as exc:
        _ev('(throw 42)')
    assert "42" in str(exc.value)


def test_throw_in_false_branch_of_if():
    assert _ev('(if true :ok (throw "not-reached"))') is not None


def test_throw_in_do_stops_evaluation():
    with pytest.raises(IllegalStateException):
        _ev('(do 1 (throw "fail") 3)')
