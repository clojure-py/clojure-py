"""Iterator exhaustion — every collection's `iter()` must raise StopIteration
once drained, never silently yield None / wrap around / segfault.

Vanilla Clojure asserts the JVM equivalent (`NoSuchElementException` from
`.iterator()`); the Python contract is StopIteration. This file pairs simple
sized cases (empty, length-1, length-N, including PVector chunk-boundary
sizes 32 / 33 / 1024) with hypothesis-driven fuzz across random shapes.
"""

import pytest
from hypothesis import given, settings, strategies as st

from clojure._core import (
    Cons,
    EmptyList,
    LazySeq,
    PersistentArrayMap,
    PersistentHashMap,
    PersistentHashSet,
    PersistentList,
    PersistentTreeMap,
    PersistentTreeSet,
    PersistentVector,
    VectorSeq,
    eval_string as _ev,
    list_,
    seq,
    vector,
)


# ---------------------------------------------------------------------------
# Helper: drain then assert StopIteration on next()
# ---------------------------------------------------------------------------

def assert_exhausts(coll):
    """Iterate fully, then confirm `next` past the end raises StopIteration."""
    it = iter(coll)
    drained = []
    while True:
        try:
            drained.append(next(it))
        except StopIteration:
            break
    # A second next() must also raise (not return None, not loop).
    with pytest.raises(StopIteration):
        next(it)
    return drained


# ---------------------------------------------------------------------------
# PersistentVector — fixed sizes including chunk boundaries (32 / 33 / 1024)
# ---------------------------------------------------------------------------

@pytest.mark.parametrize("n", [0, 1, 7, 31, 32, 33, 100, 1024, 2000])
def test_pvector_iter_exhausts(n):
    v = vector(*range(n))
    assert isinstance(v, PersistentVector)
    drained = assert_exhausts(v)
    assert drained == list(range(n))


# ---------------------------------------------------------------------------
# PersistentList / EmptyList
# ---------------------------------------------------------------------------

def test_empty_list_iter_exhausts():
    assert isinstance(list_(), EmptyList)
    drained = assert_exhausts(list_())
    assert drained == []


@pytest.mark.parametrize("n", [1, 5, 50, 500])
def test_persistent_list_iter_exhausts(n):
    lst = list_(*range(n))
    assert isinstance(lst, PersistentList)
    drained = assert_exhausts(lst)
    assert drained == list(range(n))


# ---------------------------------------------------------------------------
# PersistentHashMap / PersistentArrayMap / PersistentTreeMap
# ---------------------------------------------------------------------------

def _build_clj_map(form):
    return _ev(form)


@pytest.mark.parametrize("n", [0, 1, 4, 7, 8, 9, 16, 64, 200])
def test_hash_map_iter_exhausts(n):
    """Maps iterate as key+entry. Just confirm StopIteration past end."""
    # Build via `into` so we can exceed the ~127-arg fn-call cap.
    m = _ev(f"(into {{}} (map (fn [i] [(keyword (str \"k\" i)) i]) (range {n})))")
    assert isinstance(m, (PersistentHashMap, PersistentArrayMap))
    drained = assert_exhausts(m)
    assert len(drained) == n


def test_array_map_small_iter_exhausts():
    m = _ev("(array-map :a 1 :b 2 :c 3)")
    assert isinstance(m, PersistentArrayMap)
    drained = assert_exhausts(m)
    assert len(drained) == 3


@pytest.mark.parametrize("n", [0, 1, 5, 20, 100])
def test_sorted_map_iter_exhausts(n):
    m = _ev(f"(into (sorted-map) (map (fn [i] [i (str \"v\" i)]) (range {n})))")
    assert isinstance(m, PersistentTreeMap)
    drained = assert_exhausts(m)
    assert len(drained) == n


# ---------------------------------------------------------------------------
# PersistentHashSet / PersistentTreeSet
# ---------------------------------------------------------------------------

@pytest.mark.parametrize("n", [0, 1, 7, 32, 33, 200])
def test_hash_set_iter_exhausts(n):
    s = _ev(f"(into #{{}} (range {n}))")
    assert isinstance(s, PersistentHashSet)
    drained = assert_exhausts(s)
    assert len(drained) == n


@pytest.mark.parametrize("n", [0, 1, 7, 50])
def test_sorted_set_iter_exhausts(n):
    items = " ".join(str(i) for i in range(n))
    s = _ev(f"(sorted-set {items})")
    assert isinstance(s, PersistentTreeSet)
    drained = assert_exhausts(s)
    assert len(drained) == n


# ---------------------------------------------------------------------------
# Seq adapters — Cons, LazySeq, VectorSeq
# ---------------------------------------------------------------------------

def test_vector_seq_iter_exhausts():
    s = seq(vector(*range(40)))
    assert isinstance(s, VectorSeq)
    drained = assert_exhausts(s)
    assert drained == list(range(40))


def test_cons_iter_exhausts():
    c = _ev("(cons 0 (cons 1 (cons 2 nil)))")
    assert isinstance(c, Cons)
    drained = assert_exhausts(c)
    assert drained == [0, 1, 2]


def test_lazy_seq_iter_exhausts():
    s = _ev("(lazy-seq (cons 1 (lazy-seq (cons 2 (lazy-seq nil)))))")
    assert isinstance(s, LazySeq)
    drained = assert_exhausts(s)
    assert drained == [1, 2]


def test_lazy_seq_empty_iter_exhausts():
    s = _ev("(lazy-seq nil)")
    assert isinstance(s, LazySeq)
    drained = assert_exhausts(s)
    assert drained == []


def test_range_seq_iter_exhausts():
    s = _ev("(range 5)")
    drained = assert_exhausts(s)
    assert drained == [0, 1, 2, 3, 4]


# ---------------------------------------------------------------------------
# Subvec is a separate code path on the JVM; on us it's just PVector — but
# vanilla flags it as a distinct iterator source. Cover anyway.
# ---------------------------------------------------------------------------

@pytest.mark.parametrize("start,end", [(0, 0), (0, 5), (10, 32), (5, 100)])
def test_subvec_iter_exhausts(start, end):
    v = _ev(f"(subvec (vec (range 200)) {start} {end})")
    drained = assert_exhausts(v)
    assert drained == list(range(start, end))


# ---------------------------------------------------------------------------
# Hypothesis fuzz — random sizes / random contents across major shapes
# ---------------------------------------------------------------------------

@given(st.lists(st.integers(min_value=-1000, max_value=1000), max_size=200))
@settings(max_examples=50, deadline=None)
def test_pvector_random_iter_exhausts(items):
    v = vector(*items)
    drained = assert_exhausts(v)
    assert drained == items


@given(st.lists(st.integers(min_value=-1000, max_value=1000), max_size=100))
@settings(max_examples=50, deadline=None)
def test_plist_random_iter_exhausts(items):
    lst = list_(*items)
    drained = assert_exhausts(lst)
    assert drained == items


@given(st.sets(st.integers(min_value=-1000, max_value=1000), max_size=100))
@settings(max_examples=50, deadline=None)
def test_phashset_random_iter_exhausts(items):
    items_str = " ".join(str(i) for i in items)
    s = _ev(f"(hash-set {items_str})")
    drained = assert_exhausts(s)
    assert set(drained) == items


@given(st.dictionaries(
    st.integers(min_value=-1000, max_value=1000),
    st.integers(min_value=-1000, max_value=1000),
    max_size=100,
))
@settings(max_examples=50, deadline=None)
def test_phashmap_random_iter_exhausts(items):
    pairs = " ".join(f"{k} {v}" for k, v in items.items())
    m = _ev(f"(hash-map {pairs})")
    drained = assert_exhausts(m)
    assert len(drained) == len(items)


# ---------------------------------------------------------------------------
# Re-iterability: each iter() must produce its own independent iterator,
# i.e. exhausting one must not leave the underlying collection in a bad state.
# ---------------------------------------------------------------------------

def test_pvector_re_iterates():
    v = vector(*range(5))
    list(iter(v))  # drain once
    assert list(iter(v)) == [0, 1, 2, 3, 4]  # second pass independent


def test_phashset_re_iterates():
    s = _ev("(hash-set 1 2 3)")
    list(iter(s))
    assert sorted(list(iter(s))) == [1, 2, 3]


def test_lazy_seq_re_iterates():
    s = _ev("(lazy-seq (cons 1 (lazy-seq (cons 2 nil))))")
    list(iter(s))
    assert list(iter(s)) == [1, 2]
