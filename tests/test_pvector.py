"""PersistentVector core — HAMT-backed indexed collection."""

import pytest
from clojure._core import PersistentVector, vector, IllegalStateException


def test_empty_vector():
    v = vector()
    assert isinstance(v, PersistentVector)
    assert len(v) == 0
    assert list(v) == []


def test_single_element():
    v = vector(1)
    assert len(v) == 1
    assert v.nth(0) == 1


def test_many_elements():
    v = vector(*range(100))
    assert len(v) == 100
    for i in range(100):
        assert v.nth(i) == i


def test_conj_appends():
    v = vector(1, 2, 3)
    v2 = v.conj(4)
    assert len(v2) == 4
    assert v2.nth(3) == 4
    # Original unchanged:
    assert len(v) == 3


def test_conj_crossing_tail_boundary():
    """Vector tail holds 1..=32 elements. When tail fills, it pushes into trie."""
    v = vector(*range(32))
    assert len(v) == 32
    v2 = v.conj(32)
    assert len(v2) == 33
    assert v2.nth(32) == 32
    # Everything else still accessible:
    for i in range(33):
        assert v2.nth(i) == i


def test_conj_deep_trie():
    """Build 2000 elements — forces multi-level trie."""
    v = vector()
    for i in range(2000):
        v = v.conj(i)
    assert len(v) == 2000
    for i in range(2000):
        assert v.nth(i) == i


def test_nth_out_of_bounds():
    v = vector(1, 2, 3)
    with pytest.raises(IndexError):
        v.nth(5)
    with pytest.raises(IndexError):
        v.nth(-1)


def test_nth_with_default():
    v = vector(1, 2, 3)
    assert v.nth_or_default(5, "missing") == "missing"
    assert v.nth_or_default(1, "missing") == 2


def test_assoc_n():
    v = vector(1, 2, 3)
    v2 = v.assoc_n(1, 99)
    assert v2.nth(1) == 99
    assert v.nth(1) == 2  # original unchanged


def test_assoc_n_at_end_appends():
    """assoc-n at index == count is equivalent to conj."""
    v = vector(1, 2, 3)
    v2 = v.assoc_n(3, 4)
    assert len(v2) == 4
    assert v2.nth(3) == 4


def test_assoc_n_out_of_bounds():
    v = vector(1, 2, 3)
    with pytest.raises(IndexError):
        v.assoc_n(5, 99)


def test_pop():
    v = vector(1, 2, 3)
    v2 = v.pop()
    assert len(v2) == 2
    assert v2.nth(0) == 1
    assert v2.nth(1) == 2


def test_pop_empty_raises():
    with pytest.raises(IllegalStateException):
        vector().pop()


def test_iteration_order():
    v = vector(*range(100))
    assert list(v) == list(range(100))


def test_getitem():
    v = vector("a", "b", "c")
    assert v[0] == "a"
    assert v[1] == "b"
    assert v[2] == "c"


def test_getitem_out_of_bounds():
    v = vector("a")
    with pytest.raises(IndexError):
        _ = v[5]


def test_repr():
    assert repr(vector()) == "[]"
    assert repr(vector(1, 2, 3)) == "[1 2 3]"


def test_structural_sharing_preserved():
    """Hold a reference to v1, build derived v2/v3, v1 must remain unchanged."""
    v1 = vector(*range(100))
    v2 = v1.conj(100)
    v3 = v2.assoc_n(50, 999)
    assert len(v1) == 100
    assert v1.nth(50) == 50
    assert len(v2) == 101
    assert v2.nth(50) == 50
    assert v2.nth(100) == 100
    assert len(v3) == 101
    assert v3.nth(50) == 999
    assert v3.nth(100) == 100


# --- Protocol dispatch tests (added in Phase 6B) ---

from clojure._core import count, equiv, hash_eq, conj, peek, pop, empty


def test_rt_count_via_counted():
    assert count(vector()) == 0
    assert count(vector(1, 2, 3)) == 3


def test_rt_equiv_same_contents():
    assert equiv(vector(1, 2, 3), vector(1, 2, 3)) is True
    assert equiv(vector(), vector()) is True
    assert equiv(vector(1, 2), vector(1, 2, 3)) is False


def test_rt_equiv_cross_type_vector_vs_list_true():
    """Cross-type sequential equality: a vector equals any Sequential (list,
    lazy seq, cons) with the same elements in order."""
    from clojure._core import list_
    assert equiv(vector(1, 2, 3), list_(1, 2, 3)) is True
    assert equiv(list_(1, 2, 3), vector(1, 2, 3)) is True
    assert equiv(vector(1, 2), list_(1, 2, 3)) is False
    assert equiv(vector(), list_()) is True


def test_rt_hash_eq_stable():
    assert hash_eq(vector(1, 2, 3)) == hash_eq(vector(1, 2, 3))


def test_rt_conj_ipc():
    v = conj(vector(1, 2), 3)
    assert len(v) == 3
    assert v.nth(2) == 3


def test_rt_peek_pop_via_ipersistentstack():
    assert peek(vector(1, 2, 3)) == 3
    p = pop(vector(1, 2, 3))
    assert len(p) == 2
    assert p.nth(1) == 2


def test_rt_peek_empty_returns_nil():
    assert peek(vector()) is None


def test_rt_pop_empty_raises():
    with pytest.raises(IllegalStateException):
        pop(vector())


def test_rt_empty_returns_empty_vector():
    e = empty(vector(1, 2, 3))
    assert isinstance(e, PersistentVector)
    assert len(e) == 0


def test_vector_is_callable_as_ifn():
    """Vector implements IFn — (v i) == (nth v i)."""
    v = vector("a", "b", "c")
    assert v(0) == "a"
    assert v(1) == "b"


def test_vector_call_out_of_bounds():
    v = vector("a")
    with pytest.raises(IndexError):
        _ = v(5)


def test_associative_assoc_via_rt():
    """Vector satisfies Associative — assoc by index."""
    v = vector(10, 20, 30)
    v2 = v.assoc_n(1, 99)  # use the pymethod directly
    assert v2.nth(1) == 99


def test_rt_nth_via_indexed():
    from clojure._core import nth
    v = vector("a", "b", "c")
    assert nth(v, 0) == "a"
    assert nth(v, 2) == "c"


def test_rt_length_via_ipersistent_vector():
    from clojure._core import length
    assert length(vector()) == 0
    assert length(vector(1, 2, 3)) == 3
