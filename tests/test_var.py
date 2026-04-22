"""Var — construction, root, alter-var-root, watches, validator."""

import pytest
import sys
import types
from clojure._core import Var, IllegalStateException, IllegalArgumentException, symbol


def _ns(name: str):
    m = types.ModuleType(name)
    sys.modules[name] = m
    return m


@pytest.fixture
def v():
    m = _ns("test.var")
    yield Var(m, symbol("x"))
    del sys.modules["test.var"]


def test_unbound_deref_raises(v):
    with pytest.raises(IllegalStateException, match="unbound"):
        v.deref()
    assert v.is_bound is False


def test_bind_root_then_deref(v):
    v.bind_root(42)
    assert v.deref() == 42
    assert v.is_bound is True


def test_alter_root(v):
    v.bind_root(10)
    new = v.alter_root(lambda o, n: o + n, 5)
    assert new == 15
    assert v.deref() == 15


def test_alter_root_no_extra_args(v):
    v.bind_root(10)
    assert v.alter_root(lambda o: o * 2) == 20


def test_validator_rejects(v):
    v.bind_root(0)
    v.set_validator(lambda x: x >= 0)
    with pytest.raises(IllegalArgumentException):
        v.bind_root(-1)
    assert v.deref() == 0  # unchanged


def test_validator_allows(v):
    v.set_validator(lambda x: isinstance(x, int))
    v.bind_root(7)
    assert v.deref() == 7


def test_get_validator(v):
    check = lambda x: True
    v.set_validator(check)
    assert v.get_validator() is check
    v.set_validator(None)
    assert v.get_validator() is None


def test_watches_fire(v):
    calls = []
    v.bind_root(0)
    v.add_watch("w1", lambda k, ref, old, new: calls.append((k, old, new)))
    v.bind_root(5)
    assert calls == [("w1", 0, 5)]


def test_watches_receive_var_as_ref(v):
    seen_refs = []
    v.bind_root(1)
    v.add_watch("w", lambda k, ref, old, new: seen_refs.append(ref))
    v.bind_root(2)
    assert seen_refs == [v]


def test_remove_watch(v):
    calls = []
    v.bind_root(0)
    v.add_watch("w1", lambda k, r, o, n: calls.append("w1"))
    v.remove_watch("w1")
    v.bind_root(1)
    assert calls == []


def test_repr(v):
    v.bind_root(1)
    assert repr(v) == "#'test.var/x"


def test_dynamic_flag(v):
    assert v.is_dynamic is False
    v.set_dynamic(True)
    assert v.is_dynamic is True


def test_meta_set_get(v):
    assert v.meta is None
    v.set_meta({"doc": "a var"})
    assert v.meta == {"doc": "a var"}


def test_alter_root_watches_fire_once_per_change(v):
    v.bind_root(1)
    calls = []
    v.add_watch("w", lambda k, r, o, n: calls.append((o, n)))
    v.alter_root(lambda x, y: x + y, 10)
    v.alter_root(lambda x: x * 2)
    assert calls == [(1, 11), (11, 22)]
