"""Seq types — Cons, LazySeq, VectorSeq."""

import pytest
from clojure._core import (
    Cons, LazySeq, VectorSeq,
    cons, lazy_seq,
    vector, list_,
    first, next as next_seq, seq, count, rest,
)


# --- Cons ---

def test_cons_construction():
    c = cons(1, list_(2, 3))
    assert isinstance(c, Cons)
    assert first(c) == 1


def test_cons_first_and_rest():
    c = cons(1, list_(2, 3))
    assert first(c) == 1
    r = rest(c)
    assert first(r) == 2


def test_cons_onto_nil():
    c = cons(42, None)
    assert first(c) == 42
    # rest on a cons with nil more returns an empty seq (nil-ish)
    r = rest(c)
    # (rest c) returns whatever .more holds — nil in this case
    assert r is None or (hasattr(r, '__len__') and len(r) == 0)


def test_cons_count():
    c = cons(1, list_(2, 3))
    assert count(c) == 3


def test_cons_repr():
    c = cons(1, list_(2))
    # Cons prints as (1 2) like a list
    r = repr(c)
    assert r.startswith("(")


# --- LazySeq ---

def test_lazy_seq_not_realized_until_accessed():
    call_count = [0]
    def thunk():
        call_count[0] += 1
        return list_(1, 2, 3)
    ls = lazy_seq(thunk)
    assert isinstance(ls, LazySeq)
    assert call_count[0] == 0
    # Accessing first triggers realization
    assert first(ls) == 1
    assert call_count[0] == 1
    # Subsequent access doesn't re-realize
    assert first(ls) == 1
    assert call_count[0] == 1


def test_lazy_seq_rest_returns_rest():
    ls = lazy_seq(lambda: list_(10, 20, 30))
    assert first(ls) == 10
    assert first(next_seq(ls)) == 20


def test_lazy_seq_empty_realization():
    ls = lazy_seq(lambda: None)
    assert seq(ls) is None


def test_lazy_seq_chained():
    """lazy-seq can return a cons of an elem and another lazy-seq."""
    def make_range(i, n):
        if i >= n:
            return lazy_seq(lambda: None)
        return cons(i, lazy_seq(lambda: make_range(i + 1, n)))

    r = make_range(0, 5)
    # Walk via first/next
    collected = []
    cur = r
    while cur is not None and seq(cur) is not None:
        collected.append(first(cur))
        cur = next_seq(cur)
    assert collected == [0, 1, 2, 3, 4]


# --- VectorSeq: (seq vector) ---

def test_seq_empty_vector_is_nil():
    v = vector()
    assert seq(v) is None


def test_seq_non_empty_vector_returns_VectorSeq():
    v = vector(1, 2, 3)
    s = seq(v)
    assert isinstance(s, VectorSeq)


def test_seq_vector_first_and_rest():
    v = vector("a", "b", "c")
    s = seq(v)
    assert first(s) == "a"
    assert first(next_seq(s)) == "b"
    assert first(next_seq(next_seq(s))) == "c"
    # End of seq
    assert next_seq(next_seq(next_seq(s))) is None


def test_seq_vector_iteration_order():
    v = vector(*range(10))
    s = seq(v)
    collected = []
    cur = s
    while cur is not None:
        collected.append(first(cur))
        cur = next_seq(cur)
    assert collected == list(range(10))
