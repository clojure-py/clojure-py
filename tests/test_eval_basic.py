"""Phase E1 - basic evaluator: atoms + quote + if + do + let."""

import pytest
from clojure._core import (
    eval_string, keyword, symbol, EvalError,
)


def _ev(src): return eval_string(src)


# --- Self-evaluating atoms ---

def test_nil(): assert _ev("nil") is None
def test_true_false(): assert _ev("true") is True and _ev("false") is False
def test_int(): assert _ev("42") == 42
def test_float(): assert _ev("3.14") == 3.14
def test_string(): assert _ev('"hello"') == "hello"
def test_keyword(): assert _ev(":foo") == keyword("foo")

def test_vector_self_evals():
    v = _ev("[1 2 3]")
    assert list(v) == [1, 2, 3]

def test_map_self_evals():
    m = _ev("{:a 1}")
    assert m.val_at(keyword("a")) == 1

def test_set_self_evals():
    s = _ev("#{1 2 3}")
    assert 1 in s and 2 in s and 3 in s


# --- quote ---

def test_quote_atom():
    assert _ev("(quote 42)") == 42

def test_quote_symbol():
    assert _ev("(quote foo)") == symbol("foo")

def test_quote_list():
    l = _ev("(quote (1 2 3))")
    assert list(l) == [1, 2, 3]


# --- if ---

def test_if_true_branch():
    assert _ev("(if true 1 2)") == 1

def test_if_false_branch():
    assert _ev("(if false 1 2)") == 2

def test_if_nil_is_false():
    assert _ev("(if nil 1 2)") == 2

def test_if_zero_is_truthy():
    # Clojure: only nil and false are falsy; 0 is truthy.
    assert _ev("(if 0 :yes :no)") == keyword("yes")

def test_if_no_else_nil():
    assert _ev("(if false 1)") is None


# --- do ---

def test_do_empty():
    assert _ev("(do)") is None

def test_do_single():
    assert _ev("(do 42)") == 42

def test_do_sequence():
    assert _ev("(do 1 2 3)") == 3


# --- let ---

def test_let_empty_body():
    # No body -> nil.
    # Actually (let [a 1]) with no body evaluates the bindings and returns nil.
    assert _ev("(let [a 1])") is None

def test_let_single_binding():
    assert _ev("(let [a 42] a)") == 42

def test_let_multiple_bindings():
    assert _ev("(let [a 1 b 2] b)") == 2

def test_let_sequential():
    """later bindings can see earlier ones."""
    assert _ev("(let [a 1 b a] b)") == 1

def test_let_shadows_outer():
    assert _ev("(let [a 1] (let [a 2] a))") == 2

def test_let_body_is_do():
    assert _ev("(let [a 1] a a 99)") == 99

def test_let_odd_bindings_raises():
    with pytest.raises(EvalError, match="even"):
        _ev("(let [a] a)")


# --- Unresolved symbol ---

def test_unresolved_symbol_raises():
    with pytest.raises(EvalError, match="Unable to resolve"):
        _ev("xyz")
