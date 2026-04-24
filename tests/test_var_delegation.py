"""Var delegation dunders — arith, containers, bool, getattr reach-through."""

import pytest
import sys
import types
from clojure._core import Var, symbol


def _v(ns: str, root):
    m = types.ModuleType(ns)
    sys.modules[ns] = m
    v = Var(m, symbol("x"))
    v.bind_root(root)
    return v


def test_add():
    v = _v("delegation.1", 10)
    assert v + 3 == 13
    assert 3 + v == 13


def test_sub():
    v = _v("delegation.2", 10)
    assert v - 2 == 8
    assert 100 - v == 90


def test_mul_div():
    v = _v("delegation.3", 6)
    assert v * 7 == 42
    assert v / 3 == 2.0
    assert v // 4 == 1
    assert v % 4 == 2


def test_neg():
    v = _v("delegation.4", 10)
    assert -v == -10


def test_cmp():
    v = _v("delegation.5", 5)
    assert v < 10
    assert v <= 5
    assert v > 0
    assert v >= 5


def test_eq_is_identity():
    # Vanilla JVM behavior: Var equality is object identity, not root-value
    # equality. Using root-value equality would break Vars as `hash-map` keys
    # (and `with-local-vars` on unbound vars, which can't deref).
    v = _v("delegation.6", 42)
    assert v == v
    assert not (v == 42)
    v2 = _v("delegation.6b", 42)
    assert not (v == v2)


def test_hash_is_identity():
    # Identity-based (matches JVM Var.hashCode). Not the root's hash.
    v = _v("delegation.7", "hello")
    assert hash(v) == hash(v)  # stable
    v2 = _v("delegation.7b", "hello")
    assert hash(v) != hash(v2)  # distinct Vars → distinct hashes (almost always)


def test_str_delegates():
    v = _v("delegation.8", "howdy")
    assert str(v) == "howdy"


def test_repr_is_var_form():
    v = _v("delegation.9", 1)
    assert repr(v) == "#'delegation.9/x"


def test_bool_truthy():
    v = _v("delegation.10a", 1)
    assert bool(v) is True
    v2 = _v("delegation.10b", 0)
    assert bool(v2) is False


def test_container():
    v = _v("delegation.11", {"a": 1, "b": 2})
    assert "a" in v
    assert v["a"] == 1


def test_len():
    v = _v("delegation.12", [1, 2, 3, 4])
    assert len(v) == 4


def test_iter():
    v = _v("delegation.13", [10, 20, 30])
    assert list(v) == [10, 20, 30]


def test_getattr_reach_through():
    v = _v("delegation.14", "hello")
    assert v.upper() == "HELLO"


def test_isinstance_false_documented():
    """ns.N is a Var even when the root is an int. This is the documented edge."""
    v = _v("delegation.15", 1)
    assert not isinstance(v, int)


def test_arith_after_alter_root():
    v = _v("delegation.16", 10)
    v.alter_root(lambda x: x + 5)
    assert v + 100 == 115
