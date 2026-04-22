"""Var implements IFn — calls through the IFn protocol dispatch."""

import pytest
import sys
import types
from clojure._core import Var, symbol, invoke1, invoke2, invoke_variadic, IllegalStateException


def _mkvar(ns_name: str, sym_name: str, root=None):
    m = types.ModuleType(ns_name)
    sys.modules[ns_name] = m
    v = Var(m, symbol(sym_name))
    if root is not None:
        v.bind_root(root)
    return v


def test_var_direct_call_lambda_root():
    v = _mkvar("var.ifn.1", "f", lambda x, y: x * y)
    assert v(3, 4) == 12


def test_var_called_via_invoke1():
    v = _mkvar("var.ifn.2", "inc", lambda x: x + 1)
    assert invoke1(v, 41) == 42


def test_var_called_via_invoke2():
    v = _mkvar("var.ifn.3", "add", lambda a, b: a + b)
    assert invoke2(v, 10, 32) == 42


def test_var_called_variadic():
    v = _mkvar("var.ifn.4", "sum", lambda *a: sum(a))
    assert invoke_variadic(v, 1, 2, 3, 4) == 10


def test_unbound_var_call_raises():
    v = _mkvar("var.ifn.5", "x")
    with pytest.raises(IllegalStateException, match="unbound"):
        v()


def test_var_with_ifn_root_dispatches_through_ifn():
    """A Var holding a Keyword (which implements IFn) should work transitively."""
    from clojure._core import keyword
    v = _mkvar("var.ifn.6", "k")
    v.bind_root(keyword("a"))
    assert v({keyword("a"): 99}) == 99


def test_var_invoke_after_alter_root():
    v = _mkvar("var.ifn.7", "op", lambda x: x)
    v.alter_root(lambda _old: lambda x: x * 2)
    assert invoke1(v, 21) == 42
