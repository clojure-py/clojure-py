"""Phase E2 — fn, invocation, closures, ns resolution."""

import pytest
import sys
import types
from clojure._core import (
    eval_string, keyword, symbol, Fn, create_ns, intern, EvalError,
)


def _ev(src): return eval_string(src)


# --- Fn creation ---

def test_fn_is_Fn():
    f = _ev("(fn [x] x)")
    assert isinstance(f, Fn)


def test_fn_identity():
    assert _ev("((fn [x] x) 42)") == 42


def test_fn_two_args():
    # Requires a callable that adds. Use a simple test fn that returns the vec of its args.
    # But we don't have any callables yet — the fn body must use locals only.
    assert _ev("((fn [x y] y) 1 2)") == 2


def test_fn_nested_let():
    assert _ev("((fn [x] (let [y x] y)) 7)") == 7


def test_fn_anonymous_repr():
    f = _ev("(fn [x] x)")
    assert "Fn" in repr(f)


# --- Closures ---

def test_closure_captures_outer_let():
    """(let [x 10] (fn [] x)) — the fn sees x via captured env."""
    f = _ev("(let [x 10] (fn [] x))")
    assert f() == 10


def test_closure_nested():
    f = _ev("(let [x 1 y 2] (fn [] y))")
    assert f() == 2


def test_closure_of_fn_of_fn():
    f = _ev("(let [n 99] (fn [] ((fn [] n))))")
    assert f() == 99


def test_fn_arity_mismatch_raises():
    f = _ev("(fn [x] x)")
    with pytest.raises(EvalError, match="Wrong number of args"):
        f(1, 2)


# --- ns-resolved callables ---

def test_resolve_from_current_ns():
    """Pre-seed clojure.user with a callable Var; call it via eval."""
    ns = create_ns(symbol("clojure.user"))
    v = intern(ns, symbol("double-it"))
    v.bind_root(lambda x: x * 2)
    assert _ev("(double-it 21)") == 42


def test_resolve_qualified():
    ns = create_ns(symbol("ev.test"))
    v = intern(ns, symbol("triple"))
    v.bind_root(lambda x: x * 3)
    assert _ev("(ev.test/triple 7)") == 21


def test_invoke_plain_python_callable_via_var():
    ns = create_ns(symbol("clojure.user"))
    v = intern(ns, symbol("as-str"))
    v.bind_root(str)
    assert _ev("(as-str 42)") == "42"


def test_unresolved_still_raises():
    with pytest.raises(EvalError, match="Unable to resolve"):
        _ev("(totally-undefined-symbol 1)")


# --- Keyword-as-IFn (since Keyword implements IFn, it should be callable in eval) ---

def test_keyword_invocation():
    # (:k {:k 1}) — keyword as fn
    assert _ev("(:k {:k 42})") == 42


def test_keyword_invocation_default():
    assert _ev("(:missing {:k 1} \"default\")") == "default"


# --- Vector-as-IFn ---

def test_vector_as_fn():
    # ([a b c] 1) → b (vector indexed)
    assert _ev("([10 20 30] 1)") == 20
