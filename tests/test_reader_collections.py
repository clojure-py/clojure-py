"""Phase R3 — collection readers."""

import pytest
from clojure._core import (
    read_string, PersistentList, PersistentVector,
    PersistentHashMap, PersistentArrayMap, PersistentHashSet,
    keyword, symbol, ReaderError,
)


# --- Lists ---

def test_empty_list():
    lst = read_string("()")
    from clojure._core import EmptyList
    assert isinstance(lst, EmptyList)


def test_list_of_ints():
    lst = read_string("(1 2 3)")
    assert isinstance(lst, PersistentList)
    assert list(lst) == [1, 2, 3]


def test_nested_list():
    lst = read_string("(1 (2 3) 4)")
    assert list(lst)[0] == 1
    inner = list(lst)[1]
    assert isinstance(inner, PersistentList)
    assert list(inner) == [2, 3]
    assert list(lst)[2] == 4


# --- Vectors ---

def test_empty_vector():
    v = read_string("[]")
    assert isinstance(v, PersistentVector)
    assert len(v) == 0


def test_vector_of_ints():
    v = read_string("[1 2 3]")
    assert isinstance(v, PersistentVector)
    assert list(v) == [1, 2, 3]


def test_nested_vector():
    v = read_string("[[1 2] [3 4]]")
    assert len(v) == 2
    assert list(v.nth(0)) == [1, 2]
    assert list(v.nth(1)) == [3, 4]


def test_vector_of_mixed_types():
    v = read_string('[1 "two" :three]')
    assert v.nth(0) == 1
    assert v.nth(1) == "two"
    assert v.nth(2) == keyword("three")


# --- Maps ---

def test_empty_map():
    m = read_string("{}")
    assert isinstance(m, (PersistentHashMap, PersistentArrayMap))
    assert len(m) == 0


def test_map_with_keyword_keys():
    m = read_string("{:a 1 :b 2}")
    assert len(m) == 2
    assert m.val_at(keyword("a")) == 1
    assert m.val_at(keyword("b")) == 2


def test_nested_map():
    m = read_string("{:outer {:inner 42}}")
    inner = m.val_at(keyword("outer"))
    assert inner.val_at(keyword("inner")) == 42


def test_map_odd_forms_raises():
    with pytest.raises(ReaderError, match="even"):
        read_string("{:a 1 :b}")


# --- Sets ---

def test_empty_set():
    s = read_string("#{}")
    assert isinstance(s, PersistentHashSet)
    assert len(s) == 0


def test_set_of_values():
    s = read_string("#{1 2 3}")
    assert len(s) == 3
    assert 1 in s
    assert 2 in s
    assert 3 in s


def test_set_duplicate_raises():
    with pytest.raises(ReaderError, match="[Dd]uplicate"):
        read_string("#{1 1}")


# --- Mixed ---

def test_mixed_complex():
    v = read_string('[:a {:b [1 2]} #{:c}]')
    assert v.nth(0) == keyword("a")
    m = v.nth(1)
    assert isinstance(m, (PersistentHashMap, PersistentArrayMap))
    inner_v = m.val_at(keyword("b"))
    assert list(inner_v) == [1, 2]
    s = v.nth(2)
    assert isinstance(s, PersistentHashSet)
    assert keyword("c") in s


# --- Errors ---

def test_unmatched_open_paren():
    with pytest.raises(ReaderError, match="EOF"):
        read_string("(1 2")


def test_unmatched_open_bracket():
    with pytest.raises(ReaderError, match="EOF"):
        read_string("[1 2")


def test_unmatched_open_brace():
    with pytest.raises(ReaderError, match="EOF"):
        read_string("{1 2")


def test_delimiter_mismatch():
    with pytest.raises(ReaderError, match="[Uu]nmatched"):
        read_string("(1 2]")


# --- Large stress ---

def test_large_vector():
    src = "[" + " ".join(str(i) for i in range(500)) + "]"
    v = read_string(src)
    assert len(v) == 500
    for i in range(500):
        assert v.nth(i) == i


def test_large_map():
    pairs = " ".join(f":k{i} {i}" for i in range(200))
    m = read_string("{" + pairs + "}")
    assert len(m) == 200
    for i in range(200):
        assert m.val_at(keyword(f"k{i}")) == i
