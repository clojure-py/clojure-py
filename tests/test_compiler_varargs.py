"""Compiler tests — varargs (`& rest`) in fn*.

The rest binding is a Clojure seq (or nil if no extras), not a Python
tuple — the prologue runs RT.seq on the raw *rest tuple before the body
sees it."""

import pytest

from clojure.lang import (
    Compiler,
    read_string,
    Var, Symbol, ISeq,
)


def _eval(src):
    return Compiler.eval(read_string(src))


def _intern_fn(name, fn):
    Var.intern(Compiler.current_ns(), Symbol.intern(name), fn)


_intern_fn("cva-add", lambda a, b: a + b)
_intern_fn("cva-first", lambda s: s.first() if s is not None else None)
_intern_fn("cva-next", lambda s: s.next() if s is not None else None)


# --- only-rest ---------------------------------------------------------

def test_only_rest_with_args():
    f = _eval("(fn* [& xs] xs)")
    result = f(1, 2, 3)
    assert isinstance(result, ISeq)
    assert list(result) == [1, 2, 3]

def test_only_rest_no_args_is_nil():
    f = _eval("(fn* [& xs] xs)")
    assert f() is None


# --- required + rest ---------------------------------------------------

def test_required_plus_rest():
    f = _eval("(fn* [a b & rest] rest)")
    assert f(1, 2) is None
    result = f(1, 2, 3, 4, 5)
    assert list(result) == [3, 4, 5]

def test_required_args_visible():
    f = _eval("(fn* [a b & rest] (cva-add a b))")
    assert f(10, 32) == 42
    assert f(10, 32, 99, 100) == 42


# --- rest is a seq -----------------------------------------------------

def test_rest_is_a_clojure_seq_not_a_python_tuple():
    f = _eval("(fn* [& xs] xs)")
    assert isinstance(f(1, 2, 3), ISeq)

def test_rest_supports_first_and_next():
    f = _eval("(fn* [& xs] (cva-first xs))")
    assert f(99, 100) == 99
    f = _eval("(fn* [& xs] (cva-first (cva-next xs)))")
    assert f(99, 100) == 100


# --- closure interaction -----------------------------------------------

def test_vararg_fn_captures_outer():
    f = _eval("(let* [base 10] (fn* [& xs] (cva-add base (cva-first xs))))")
    assert f(5) == 15
    assert f(7, 99) == 17


# --- code object shape -------------------------------------------------

def test_vararg_code_has_correct_argcount_and_flags():
    f = _eval("(fn* [a b & rest] rest)")
    assert f.__code__.co_argcount == 2
    # CO_VARARGS = 0x4; should be set
    assert f.__code__.co_flags & 0x4
    assert "rest" in f.__code__.co_varnames


# --- error cases -------------------------------------------------------

def test_amp_without_rest_arg_raises():
    with pytest.raises(SyntaxError):
        _eval("(fn* [a &] a)")

def test_extra_arg_after_rest_raises():
    with pytest.raises(SyntaxError):
        _eval("(fn* [a & b c] a)")
