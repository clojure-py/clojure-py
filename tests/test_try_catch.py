"""try/catch/finally special-form tests."""

import pytest
from clojure._core import eval_string, keyword, IllegalArgumentException, IllegalStateException


def _ev(src):
    return eval_string(src)


# --- Basic try ---

def test_try_no_exception_returns_body():
    assert _ev("(try 42)") == 42
    assert _ev("(try (+ 1 2 3))") == 6


def test_try_no_body_returns_nil():
    assert _ev("(try)") is None


def test_try_returns_last_expr():
    assert _ev("(try 1 2 3)") == 3


# --- Catch ---

def test_catch_matches_raised_exception():
    assert _ev(
        "(try (clojure.lang.RT/throw-iae \"boom\") "
        "  (catch clojure.lang.IllegalArgumentException e (str e)))"
    ) == "boom"


def test_catch_non_matching_propagates():
    with pytest.raises(IllegalArgumentException):
        _ev(
            "(try (clojure.lang.RT/throw-iae \"boom\") "
            "  (catch clojure.lang.IllegalStateException e :caught))"
        )


def test_catch_tries_multiple_clauses_in_order():
    # First clause mismatches → second clause matches.
    assert _ev(
        "(try (clojure.lang.RT/throw-iae \"x\") "
        "  (catch clojure.lang.IllegalStateException e :first) "
        "  (catch clojure.lang.IllegalArgumentException e :second))"
    ) == keyword("second")


def test_catch_binds_exception_to_local():
    r = _ev(
        "(try (clojure.lang.RT/throw-iae \"msg-text\") "
        "  (catch clojure.lang.IllegalArgumentException e (str e)))"
    )
    assert "msg-text" in r


def test_nested_try_uses_innermost_catch():
    r = _ev(
        "(try "
        "  (try (clojure.lang.RT/throw-iae \"inner\") "
        "    (catch clojure.lang.IllegalStateException e :wrong)) "
        "  (catch clojure.lang.IllegalArgumentException e :outer))"
    )
    assert r == keyword("outer")


# --- Finally ---

def test_finally_runs_on_normal_exit():
    r = _ev(
        "(let [log (atom [])] "
        "  (try :body (finally (swap! log conj :fin))) "
        "  (vec (deref log)))"
    )
    assert list(r) == [keyword("fin")]


def test_finally_runs_on_exception_propagation():
    r = _ev(
        "(let [log (atom [])] "
        "  (try "
        "    (try (clojure.lang.RT/throw-iae \"x\") "
        "      (finally (swap! log conj :fin))) "
        "    (catch clojure.lang.IllegalArgumentException _ :caught)) "
        "  (vec (deref log)))"
    )
    assert list(r) == [keyword("fin")]


def test_finally_runs_after_matched_catch():
    r = _ev(
        "(let [log (atom [])] "
        "  (try (clojure.lang.RT/throw-iae \"x\") "
        "    (catch clojure.lang.IllegalArgumentException e (swap! log conj :caught)) "
        "    (finally (swap! log conj :fin))) "
        "  (vec (deref log)))"
    )
    assert list(r) == [keyword("caught"), keyword("fin")]


def test_try_returns_body_even_with_finally():
    # Finally's value is discarded.
    assert _ev(
        "(try 42 (finally :ignored))"
    ) == 42


def test_try_returns_catch_value():
    assert _ev(
        "(try (clojure.lang.RT/throw-iae \"x\") "
        "  (catch clojure.lang.IllegalArgumentException e :caught) "
        "  (finally :ignored))"
    ) == keyword("caught")


# --- Throw through functions ---

def test_throw_from_called_fn_catches():
    r = _ev(
        "(let [f (fn [] (clojure.lang.RT/throw-iae \"inner\"))]"
        "  (try (f) (catch clojure.lang.IllegalArgumentException e (str e))))"
    )
    assert "inner" in r


def test_throw_in_catch_body_propagates():
    # A catch that itself throws should propagate out of the try.
    # We throw a value directly; the VM wraps non-exception values as IAE.
    with pytest.raises(IllegalStateException):
        _ev(
            "(try (clojure.lang.RT/throw-iae \"first\") "
            "  (catch clojure.lang.IllegalArgumentException _ "
            "    (throw :re-thrown)))"
        )
