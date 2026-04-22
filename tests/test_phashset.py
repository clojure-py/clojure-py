"""PersistentHashSet — thin wrapper over PersistentHashMap."""

import pytest
import threading
from clojure._core import (
    PersistentHashSet, TransientHashSet, hash_set, keyword,
    transient, persistent_bang, conj_bang, disj_bang,
    IllegalStateException,
)


def test_empty():
    s = hash_set()
    assert isinstance(s, PersistentHashSet)
    assert len(s) == 0


def test_conj():
    s = hash_set().conj("a").conj("b")
    assert len(s) == 2
    assert s.contains("a") is True
    assert s.contains("b") is True


def test_conj_duplicate_noop():
    s = hash_set().conj("a").conj("a")
    assert len(s) == 1


def test_disj():
    s = hash_set().conj("a").conj("b").disjoin("a")
    assert len(s) == 1
    assert s.contains("a") is False
    assert s.contains("b") is True


def test_disj_missing_noop():
    s = hash_set().conj("a").disjoin("nonexistent")
    assert len(s) == 1


def test_get_returns_value_or_nil():
    """Set's .get returns the stored value (== key) if present, else nil."""
    s = hash_set().conj("a")
    assert s.get("a") == "a"
    assert s.get("missing") is None


def test_callable_as_ifn():
    """(s k) returns k-if-present-else-nil."""
    s = hash_set().conj("a").conj("b")
    assert s("a") == "a"
    assert s("missing") is None


def test_stress_insertion():
    s = hash_set()
    for i in range(500):
        s = s.conj(i)
    assert len(s) == 500
    for i in range(500):
        assert s.contains(i)


def test_hash_set_constructor():
    s = hash_set("a", "b", "c")
    assert len(s) == 3
    assert all(s.contains(x) for x in ["a", "b", "c"])


def test_iteration_yields_values():
    s = hash_set("a", "b", "c")
    assert set(iter(s)) == {"a", "b", "c"}


def test_contains_via_in():
    s = hash_set("a", "b")
    assert "a" in s
    assert "x" not in s


def test_repr_contains_elements():
    s = hash_set("a")
    r = repr(s)
    assert r.startswith("#{") and r.endswith("}")
    assert "a" in r


def test_nil_element():
    s = hash_set().conj(None)
    assert s.contains(None) is True
    assert None in s


# --- Transient ---

def test_transient_round_trip():
    s = hash_set("a", "b")
    t = transient(s)
    assert isinstance(t, TransientHashSet)
    s2 = persistent_bang(t)
    assert isinstance(s2, PersistentHashSet)
    assert s2.contains("a") and s2.contains("b")


def test_transient_conj_bang():
    t = transient(hash_set())
    for i in range(100):
        conj_bang(t, i)
    s = persistent_bang(t)
    assert len(s) == 100


def test_transient_disj_bang():
    t = transient(hash_set("a", "b", "c"))
    disj_bang(t, "b")
    s = persistent_bang(t)
    assert len(s) == 2
    assert s.contains("b") is False


def test_use_after_persistent_raises():
    t = transient(hash_set())
    persistent_bang(t)
    with pytest.raises(IllegalStateException):
        conj_bang(t, 1)


def test_wrong_thread_raises():
    t = transient(hash_set())
    err_box = []
    def worker():
        try:
            conj_bang(t, 1)
        except Exception as e:
            err_box.append(type(e).__name__)
    th = threading.Thread(target=worker); th.start(); th.join()
    assert err_box and "IllegalStateException" in err_box[0]
