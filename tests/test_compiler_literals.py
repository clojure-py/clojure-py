"""Compiler tests — literals, quote, and the eval entry point.

This is the first compiler slice: every form here boils down to a single
LOAD_CONST, but it also exercises the FnContext / bytecode-emit /
FunctionType plumbing that all later slices build on."""

import types
import pytest

from clojure.lang import (
    Compiler,
    read_string,
    Symbol, Keyword,
    BigInt, Ratio,
    PersistentList,
)


def _eval(src):
    return Compiler.eval(read_string(src))


# --- self-evaluating atoms ---------------------------------------------

def test_eval_int():
    assert _eval("42") == 42

def test_eval_negative_int():
    assert _eval("-7") == -7

def test_eval_float():
    assert _eval("3.14") == 3.14

def test_eval_string():
    assert _eval('"hello"') == "hello"

def test_eval_string_with_escapes():
    assert _eval(r'"a\nb"') == "a\nb"

def test_eval_nil():
    assert _eval("nil") is None

def test_eval_true():
    assert _eval("true") is True

def test_eval_false():
    assert _eval("false") is False

def test_eval_keyword():
    assert _eval(":foo") == Keyword.intern(None, "foo")

def test_eval_namespaced_keyword():
    assert _eval(":a/b") == Keyword.intern("a", "b")

def test_eval_bigint():
    v = _eval("9999999999999999999999N")
    assert isinstance(v, BigInt)
    assert v == BigInt.from_long(9999999999999999999999)

def test_eval_ratio():
    v = _eval("3/4")
    assert isinstance(v, Ratio)
    assert v == Ratio(3, 4)


# --- quote -------------------------------------------------------------

def test_eval_quote_symbol():
    assert _eval("(quote foo)") == Symbol.intern("foo")

def test_eval_quote_reader_shorthand():
    assert _eval("'foo") == Symbol.intern("foo")

def test_eval_quote_nested_list():
    result = _eval("'(1 2 3)")
    assert result == read_string("(1 2 3)")

def test_eval_quote_keeps_symbols_unresolved():
    result = _eval("'(do x y)")
    assert result.first() == Symbol.intern("do")

def test_eval_quote_empty_list():
    result = _eval("'()")
    assert result.seq() is None

def test_eval_quote_requires_arg():
    with pytest.raises(SyntaxError):
        Compiler.eval(read_string("(quote)"))


# --- empty-list literal evaluates to itself ----------------------------

def test_eval_empty_list():
    result = _eval("()")
    assert result.seq() is None
    assert isinstance(result, PersistentList) or result is read_string("()")


# --- the result is a real Python function ------------------------------

def test_eval_produces_real_python_function():
    """The compiled thunk that eval runs is a genuine types.FunctionType
    with a __code__ — not a synthetic interpreter callable."""
    # Reach into the compiler's machinery just for this introspection
    # check; normal users would never call _compile_to_thunk directly.
    from clojure.lang import _compile_to_thunk
    fn = _compile_to_thunk(read_string("42"))
    assert isinstance(fn, types.FunctionType)
    assert fn.__code__.co_name == "__clj_eval__"
    assert fn() == 42
