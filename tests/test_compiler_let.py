"""Compiler tests — let* and lexical-local resolution.

Slice 4 only handles the `let*` shape (no inner closures yet); every
binding becomes a plain Python FAST local. The lexical scope is tracked
at compile time so names can be shadowed and unshadowed; the underlying
Python locals stay in the frame but become inaccessible from Clojure
code outside their scope."""

import pytest

from clojure.lang import (
    Compiler,
    read_string,
    Var, Symbol,
)


def _eval(src):
    return Compiler.eval(read_string(src))


def _intern_fn(name, fn):
    Var.intern(Compiler.current_ns(), Symbol.intern(name), fn)


_intern_fn("clt-add", lambda a, b: a + b)
_intern_fn("clt-mul", lambda a, b: a * b)
_intern_fn("clt-id", lambda x: x)


# --- basic -------------------------------------------------------------

def test_let_single_binding():
    assert _eval("(let* [x 1] x)") == 1

def test_let_two_bindings():
    assert _eval("(let* [x 10 y 32] (clt-add x y))") == 42

def test_let_sequential_dependency():
    assert _eval("(let* [x 1 y (clt-add x 2)] (clt-add x y))") == 4

def test_let_three_bindings_chained():
    assert _eval(
        "(let* [a 2 b (clt-mul a 3) c (clt-add a b)] (clt-mul c 10))"
    ) == 80

def test_let_empty_bindings():
    assert _eval("(let* [] 42)") == 42

def test_let_empty_body_returns_nil():
    assert _eval("(let* [x 5])") is None

def test_let_body_is_implicit_do():
    assert _eval("(let* [x 5] x x x)") == 5
    assert _eval("(let* [x 5] (clt-add x 1) (clt-add x 2))") == 7


# --- shadowing ---------------------------------------------------------

def test_let_shadow_inner():
    assert _eval("(let* [x 1] (let* [x 2] x))") == 2

def test_let_shadow_unshadow_outer_visible_again():
    """After the inner let returns, the outer x is back in scope."""
    assert _eval("(let* [x 1] (let* [x 99] x) x)") == 1

def test_let_shadow_with_value_using_outer():
    assert _eval("(let* [x 1] (let* [x (clt-add x 10)] x))") == 11

def test_let_same_binding_name_repeated_uses_latest():
    """Re-binding within one let* — later wins, intermediate `x` is the
    earlier value during its own RHS."""
    assert _eval("(let* [x 1 x (clt-add x 5) x (clt-add x 100)] x)") == 106


# --- locals shadow vars ------------------------------------------------

def test_local_shadows_var():
    Var.intern(Compiler.current_ns(), Symbol.intern("clt-shadowed"), 999)
    assert _eval("(let* [clt-shadowed 7] clt-shadowed)") == 7
    # Var is unshadowed after let returns
    assert _eval("clt-shadowed") == 999


# --- nested with other forms -------------------------------------------

def test_let_inside_if_branch():
    assert _eval("(if true (let* [x 7] x) 0)") == 7

def test_if_inside_let_body():
    assert _eval("(let* [x 5] (if true x 0))") == 5

def test_let_inside_do():
    assert _eval("(do (let* [x 1] x) (let* [y 2] y))") == 2

def test_let_returning_call():
    assert _eval("(let* [x 6 y 7] (clt-mul x y))") == 42


# --- error cases -------------------------------------------------------

def test_let_requires_vector_bindings():
    with pytest.raises(SyntaxError):
        _eval("(let* (x 1) x)")

def test_let_requires_even_bindings():
    with pytest.raises(SyntaxError):
        _eval("(let* [x] x)")

def test_let_binding_name_must_be_symbol():
    with pytest.raises(SyntaxError):
        _eval("(let* [1 1] 1)")

def test_let_binding_name_must_be_unqualified():
    with pytest.raises(SyntaxError):
        _eval("(let* [foo/bar 1] 1)")

def test_let_requires_bindings():
    with pytest.raises(SyntaxError):
        _eval("(let*)")
