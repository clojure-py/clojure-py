"""PersistentList + EmptyList — cons-cell list with singleton empty."""

import pytest
from clojure._core import (
    PersistentList,
    EmptyList,
    list_,
    equiv,
    hash_eq,
    count,
    first,
    rest,
    seq,
    empty,
    conj,
    IllegalStateException,
)
from clojure._core import next as next_seq  # avoid shadowing the Python builtin


def test_empty_constructor_returns_singleton():
    e1 = list_()
    e2 = list_()
    assert isinstance(e1, EmptyList)
    assert e1 is e2


def test_list_of_one():
    lst = list_(1)
    assert isinstance(lst, PersistentList)
    assert lst.first == 1
    assert isinstance(lst.rest, EmptyList)


def test_list_of_three():
    lst = list_(1, 2, 3)
    assert lst.first == 1
    assert lst.rest.first == 2
    assert lst.rest.rest.first == 3
    assert isinstance(lst.rest.rest.rest, EmptyList)


def test_dunder_len():
    assert len(list_()) == 0
    assert len(list_(1)) == 1
    assert len(list_(1, 2, 3)) == 3


def test_dunder_iter():
    assert list(list_()) == []
    assert list(list_(1, 2, 3)) == [1, 2, 3]


def test_dunder_bool():
    assert not bool(list_())
    assert bool(list_(1))


def test_dunder_eq_same_type():
    assert list_() == list_()
    assert list_(1, 2) == list_(1, 2)
    assert list_(1, 2) != list_(1, 2, 3)
    assert list_(1, 2) != list_(2, 1)


def test_dunder_hash_stable():
    assert hash(list_(1, 2)) == hash(list_(1, 2))
    # Empty list hashes via `Murmur3.mixCollHash(1, 0)` — matches vanilla
    # `Util.hasheq` for an empty IPersistentList, NOT the JVM `hashCode()`
    # which is 1.
    assert hash(list_()) == hash(list_())
    # And it agrees with empty vector / empty seq (all use Murmur3.hashOrdered).
    from clojure._core import vector
    assert hash(list_()) == hash(vector())


def test_repr():
    assert repr(list_()) == "()"
    assert repr(list_(1, 2, 3)) == "(1 2 3)"


def test_rt_count():
    assert count(list_()) == 0
    assert count(list_(1, 2, 3)) == 3


def test_rt_first_next_rest():
    lst = list_(1, 2, 3)
    assert first(lst) == 1
    assert first(next_seq(lst)) == 2
    assert rest(lst).first == 2
    # next on singleton returns nil
    assert next_seq(list_(1)) is None
    # rest on singleton returns EmptyList
    assert isinstance(rest(list_(1)), EmptyList)


def test_rt_seq():
    # (seq non-empty) returns self (or equivalent ISeq)
    lst = list_(1, 2)
    s = seq(lst)
    assert s is not None
    assert first(s) == 1
    # (seq empty) returns nil
    assert seq(list_()) is None


def test_rt_empty():
    # (empty coll) returns an empty collection of the same type
    e1 = empty(list_(1, 2, 3))
    assert isinstance(e1, EmptyList)
    e2 = empty(list_())
    assert isinstance(e2, EmptyList)


def test_rt_equiv():
    assert equiv(list_(1, 2), list_(1, 2)) is True
    assert equiv(list_(1, 2), list_(1, 2, 3)) is False
    assert equiv(list_(), list_()) is True


def test_rt_hash_eq():
    assert hash_eq(list_(1, 2)) == hash_eq(list_(1, 2))


def test_conj_onto_empty():
    c = conj(list_(), 1)
    assert c.first == 1
    assert isinstance(c.rest, EmptyList)


def test_conj_prepends():
    c = conj(list_(2, 3), 1)
    assert list(c) == [1, 2, 3]


def test_pop_empty_raises():
    # EmptyList.pop via IPersistentStack raises IllegalStateException.
    from clojure._core import pop
    with pytest.raises(IllegalStateException):
        pop(list_())


def test_pop_non_empty():
    from clojure._core import pop
    p = pop(list_(1, 2, 3))
    assert list(p) == [2, 3]


def test_peek():
    from clojure._core import peek
    assert peek(list_(1, 2, 3)) == 1
    assert peek(list_()) is None


def test_meta_default_none():
    lst = list_(1, 2)
    assert lst.meta is None


def test_with_meta_preserves_values():
    from clojure._core import with_meta
    lst = list_(1, 2)
    lst2 = with_meta(lst, {"a": 1})
    assert lst2.meta == {"a": 1}
    assert list(lst2) == [1, 2]
    assert lst.meta is None  # original unchanged
