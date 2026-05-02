"""Compiler tests — symbol resolution, the (var ...) special form, and
the #' reader macro that desugars to it."""

import pytest

from clojure.lang import (
    Compiler,
    read_string,
    Var, Symbol, Namespace, Keyword,
    PersistentArrayMap,
)


def _eval(src):
    return Compiler.eval(read_string(src))


def _intern(name, value, dynamic=False):
    """Intern a Var of the given name in the current namespace."""
    ns = Compiler.current_ns()
    v = Var.intern(ns, Symbol.intern(name), value)
    if dynamic:
        v.set_dynamic()
    return v


# --- bare symbol → Var deref -------------------------------------------

def test_bare_symbol_derefs_var():
    _intern("c-x", 42)
    assert _eval("c-x") == 42

def test_bare_symbol_dynamic_var():
    v = _intern("c-d", "root", dynamic=True)
    assert _eval("c-d") == "root"
    Var.push_thread_bindings(PersistentArrayMap.create_with_check([v, "bound"]))
    try:
        assert _eval("c-d") == "bound"
    finally:
        Var.pop_thread_bindings()
    assert _eval("c-d") == "root"

def test_bare_symbol_sees_redef():
    """Compiled code reads the Var's *current* root, not a snapshot."""
    v = _intern("c-r", 1)
    assert _eval("c-r") == 1
    v.bind_root(2)
    assert _eval("c-r") == 2

def test_bare_symbol_keyword_value():
    _intern("c-k", Keyword.intern(None, "kw"))
    assert _eval("c-k") == Keyword.intern(None, "kw")

def test_bare_symbol_unresolved_raises():
    with pytest.raises(NameError):
        _eval("c-no-such-symbol-here")


# --- (var x) returns the Var itself ------------------------------------

def test_var_special_form():
    v = _intern("c-vv", 7)
    assert _eval("(var c-vv)") is v

def test_var_reader_macro_shorthand():
    """`#'foo` is just `(var foo)`."""
    v = _intern("c-rm", 0)
    assert _eval("#'c-rm") is v

def test_var_special_form_unresolved_raises():
    with pytest.raises(NameError):
        _eval("(var c-not-a-var)")

def test_var_requires_symbol():
    with pytest.raises(SyntaxError):
        _eval("(var)")
    with pytest.raises(SyntaxError):
        _eval("(var 42)")


# --- qualified symbols resolve through the current ns ------------------

def test_qualified_symbol_resolves():
    ns = Compiler.current_ns()
    Var.intern(ns, Symbol.intern("c-q"), 123)
    qualified = ns.name.name + "/c-q"
    assert _eval(qualified) == 123
