"""MapEntry — key/value pair used in map iteration."""

from clojure._core import MapEntry, map_entry


def test_construction():
    e = map_entry("k", 42)
    assert isinstance(e, MapEntry)
    assert e.key == "k"
    assert e.val == 42


def test_iter_yields_key_then_val():
    """Support destructuring: (let [[k v] entry] ...)"""
    e = map_entry("k", 42)
    lst = list(e)
    assert lst == ["k", 42]


def test_indexable_like_tuple():
    e = map_entry("k", 42)
    assert e[0] == "k"
    assert e[1] == 42


def test_len_is_2():
    e = map_entry("k", 42)
    assert len(e) == 2


def test_equal_when_same_key_val():
    assert map_entry("k", 1) == map_entry("k", 1)
    assert map_entry("k", 1) != map_entry("k", 2)
    assert map_entry("k", 1) != map_entry("j", 1)


def test_hash_stable():
    assert hash(map_entry("k", 1)) == hash(map_entry("k", 1))


def test_repr():
    assert repr(map_entry("k", 42)) == "[k 42]"


def test_getitem_out_of_bounds():
    import pytest
    e = map_entry("k", 42)
    with pytest.raises(IndexError):
        _ = e[5]
