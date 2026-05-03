"""Compiler tests — set!.

set! works on dynamic Vars (must be thread-bound) and on object fields
via .-form. Locals can't be set! — Clojure forbids it (immutability),
and our compile-time check catches it."""

import pytest

from clojure.lang import (
    Compiler,
    read_string,
    Var, Symbol,
    PersistentArrayMap,
)


def _eval(src):
    return Compiler.eval(read_string(src))


def _intern(name, val, dynamic=False):
    ns = Compiler.current_ns()
    v = Var.intern(ns, Symbol.intern(name), val)
    if dynamic:
        v.set_dynamic()
    return v


# --- set! on dynamic Var ----------------------------------------------

def test_set_bang_dynamic_var_in_binding():
    v = _intern("csb-dyn", "root", dynamic=True)
    Var.push_thread_bindings(
        PersistentArrayMap.create_with_check([v, "init"]))
    try:
        assert _eval("csb-dyn") == "init"
        _eval('(set! csb-dyn "changed")')
        assert _eval("csb-dyn") == "changed"
    finally:
        Var.pop_thread_bindings()
    # Outside the binding, root is still "root"
    assert _eval("csb-dyn") == "root"

def test_set_bang_returns_new_value():
    v = _intern("csb-ret", 0, dynamic=True)
    Var.push_thread_bindings(
        PersistentArrayMap.create_with_check([v, 0]))
    try:
        assert _eval('(set! csb-ret 99)') == 99
    finally:
        Var.pop_thread_bindings()

def test_set_bang_non_dynamic_var_raises():
    _intern("csb-static", "x")
    with pytest.raises(RuntimeError):
        _eval('(set! csb-static "y")')

def test_set_bang_dynamic_var_outside_binding_raises():
    _intern("csb-dyn-unbound", "root", dynamic=True)
    with pytest.raises(RuntimeError):
        _eval('(set! csb-dyn-unbound "x")')


# --- set! on object fields --------------------------------------------

class _Box:
    def __init__(self, v):
        self.v = v


def test_set_bang_field_dotdash():
    b = _Box(0)
    _intern("csb-b", b)
    _eval("(set! (.-v csb-b) 42)")
    assert b.v == 42

def test_set_bang_field_explicit_dot():
    b = _Box(0)
    _intern("csb-b2", b)
    _eval("(set! (. csb-b2 -v) 99)")
    assert b.v == 99

def test_set_bang_field_returns_value():
    b = _Box(0)
    _intern("csb-b3", b)
    assert _eval("(set! (.-v csb-b3) 7)") == 7

def test_set_bang_field_in_let():
    """Field setattr can use a let-bound object reference."""
    b = _Box(0)
    _intern("csb-b4", b)
    _eval("(let* [x csb-b4] (set! (.-v x) 13))")
    assert b.v == 13


# --- set! on local raises ----------------------------------------------

def test_set_bang_on_local_let_raises():
    with pytest.raises(SyntaxError):
        _eval("(let* [x 1] (set! x 2))")

def test_set_bang_on_local_fn_arg_raises():
    with pytest.raises(SyntaxError):
        _eval("(fn* [x] (set! x 2))")


# --- error cases -------------------------------------------------------

def test_set_bang_requires_target_and_value():
    with pytest.raises(SyntaxError):
        _eval("(set!)")
    with pytest.raises(SyntaxError):
        _eval("(set! foo)")

def test_set_bang_extra_args_raises():
    with pytest.raises(SyntaxError):
        _eval("(set! foo 1 2)")

def test_set_bang_unresolved_target_raises():
    with pytest.raises(SyntaxError):
        _eval("(set! csb-no-such-symbol 1)")
