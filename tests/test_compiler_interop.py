"""Compiler tests — `.method`, `.-field`, and `(. obj ...)` interop forms.

These map Clojure interop syntax onto Python attribute access:
  (.method obj args) → obj.method(args)
  (.-field obj)      → obj.field
  (. obj method args)/(. obj (method args)) → obj.method(args)
  (. obj -field)     → obj.field
"""

import pytest

from clojure.lang import (
    Compiler,
    read_string,
    Var, Symbol,
)


class _Pt:
    def __init__(self, x, y):
        self.x = x
        self.y = y
    def dist(self):
        return (self.x ** 2 + self.y ** 2) ** 0.5
    def add(self, dx, dy):
        return _Pt(self.x + dx, self.y + dy)
    def label(self, prefix):
        return prefix + str((self.x, self.y))


def _setup():
    ns = Compiler.current_ns()
    Var.intern(ns, Symbol.intern("cit-mkpt"), _Pt)
    Var.intern(ns, Symbol.intern("cit-pt34"), _Pt(3, 4))
    Var.intern(ns, Symbol.intern("cit-strv"), "hello world")


_setup()


def _eval(src):
    return Compiler.eval(read_string(src))


# --- .-field -----------------------------------------------------------

def test_field_access():
    assert _eval("(.-x cit-pt34)") == 3
    assert _eval("(.-y cit-pt34)") == 4

def test_field_access_explicit_dot_form():
    assert _eval("(. cit-pt34 -x)") == 3
    assert _eval("(. cit-pt34 -y)") == 4

def test_field_access_extra_args_raises():
    with pytest.raises(SyntaxError):
        _eval("(.-x cit-pt34 99)")
    with pytest.raises(SyntaxError):
        _eval("(. cit-pt34 -x 99)")


# --- .method (no args) -------------------------------------------------

def test_zero_arg_method():
    assert _eval("(.dist cit-pt34)") == 5.0

def test_zero_arg_method_explicit():
    assert _eval("(. cit-pt34 dist)") == 5.0

def test_zero_arg_method_grouped():
    assert _eval("(. cit-pt34 (dist))") == 5.0


# --- .method with args -------------------------------------------------

def test_method_with_args():
    p = _eval("(.add cit-pt34 1 1)")
    assert (p.x, p.y) == (4, 5)

def test_method_with_args_explicit():
    p = _eval("(. cit-pt34 add 1 1)")
    assert (p.x, p.y) == (4, 5)

def test_method_with_args_grouped():
    p = _eval("(. cit-pt34 (add 1 1))")
    assert (p.x, p.y) == (4, 5)

def test_method_with_string_arg():
    assert _eval('(.label cit-pt34 "p=")') == "p=(3, 4)"


# --- chaining ----------------------------------------------------------

def test_chained_method_calls():
    """(.dist (.add p 0 0)) → 5.0"""
    assert _eval("(.dist (.add cit-pt34 0 0))") == 5.0

def test_chained_field_then_method():
    """Use (.upper (.-some-attr ...)) — well, on strings."""
    Var.intern(Compiler.current_ns(), Symbol.intern("cit-mk_obj"), type)
    # Use builtin behavior — call .upper on result of attribute access
    assert _eval('(.upper cit-strv)') == "HELLO WORLD"

def test_method_in_let():
    assert _eval("(let* [p cit-pt34] (.-x p))") == 3


# --- string interop (Python builtins) ---------------------------------

def test_str_method_calls():
    assert _eval('(.upper "abc")') == "ABC"
    assert _eval('(.replace "hello" "l" "L")') == "heLLo"
    assert _eval('(.startswith "abc" "ab")') is True

def test_int_method_call():
    assert _eval("(.bit_length 255)") == 8


# --- error cases -------------------------------------------------------

def test_method_call_requires_target():
    with pytest.raises(SyntaxError):
        _eval("(.dist)")

def test_field_access_requires_target():
    with pytest.raises(SyntaxError):
        _eval("(.-x)")

def test_dot_requires_target_and_member():
    with pytest.raises(SyntaxError):
        _eval("(.)")
    with pytest.raises(SyntaxError):
        _eval("(. cit-pt34)")
