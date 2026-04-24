"""Sorted collections (sorted-map / sorted-set / subseq / rsubseq).

Covers: ordered iteration, custom comparators (int-returning and
predicate-style), assoc / dissoc / conj / disj preserves sort, rseq /
subseq / rsubseq, protocol surfaces (sorted?, count, get, contains?),
plus hypothesis fuzz against a reference model.
"""

import pytest
from hypothesis import given, strategies as st, settings, HealthCheck

from clojure._core import (
    eval_string,
    PersistentTreeMap,
    PersistentTreeSet,
)


def _ev(s):
    return eval_string(s)


# --- Basics ---

def test_sorted_map_empty():
    m = _ev("(sorted-map)")
    assert isinstance(m, PersistentTreeMap)
    assert _ev("(count (sorted-map))") == 0


def test_sorted_map_ordering():
    ks = list(_ev("(keys (sorted-map 3 :c 1 :a 4 :d 2 :b))"))
    assert ks == [1, 2, 3, 4]


def test_sorted_map_values_follow_keys():
    vs = list(_ev("(vals (sorted-map 3 :c 1 :a 4 :d 2 :b))"))
    assert vs == [_ev(":a"), _ev(":b"), _ev(":c"), _ev(":d")]


def test_sorted_set_empty():
    s = _ev("(sorted-set)")
    assert isinstance(s, PersistentTreeSet)
    assert _ev("(count (sorted-set))") == 0


def test_sorted_set_ordering():
    assert list(_ev("(sorted-set 3 1 4 1 5 9 2 6)")) == [1, 2, 3, 4, 5, 6, 9]


def test_sorted_set_dedup():
    assert _ev("(count (sorted-set 1 1 1 1))") == 1


# --- Assoc/dissoc preserves sort ---

def test_assoc_preserves_sort():
    ks = list(_ev("(keys (assoc (sorted-map 2 :b 5 :e) 1 :a 3 :c 4 :d))"))
    assert ks == [1, 2, 3, 4, 5]


def test_dissoc_preserves_sort():
    ks = list(_ev("(keys (dissoc (sorted-map 1 :a 2 :b 3 :c 4 :d 5 :e) 2 4))"))
    assert ks == [1, 3, 5]


def test_conj_preserves_sort_set():
    xs = list(_ev("(conj (sorted-set 2 4 6) 5 3 1)"))
    assert xs == [1, 2, 3, 4, 5, 6]


def test_disj_preserves_sort_set():
    xs = list(_ev("(disj (sorted-set 1 2 3 4 5 6) 3 5)"))
    assert xs == [1, 2, 4, 6]


# --- Lookup ---

def test_sorted_map_get():
    assert _ev("(get (sorted-map 1 :a 2 :b) 1)") == _ev(":a")
    assert _ev("(get (sorted-map 1 :a 2 :b) 99 :missing)") == _ev(":missing")


def test_sorted_map_invoke():
    assert _ev("((sorted-map 1 :a 2 :b) 2)") == _ev(":b")


def test_sorted_set_contains():
    assert _ev("(contains? (sorted-set 1 2 3) 2)") is True
    assert _ev("(contains? (sorted-set 1 2 3) 99)") is False


# --- Custom comparators ---

def test_sorted_map_by_int_ascending():
    ks = list(_ev("(keys (sorted-map-by (fn* [a b] (- a b)) 3 :c 1 :a 2 :b))"))
    assert ks == [1, 2, 3]


def test_sorted_map_by_int_descending():
    ks = list(_ev("(keys (sorted-map-by (fn* [a b] (- b a)) 3 :c 1 :a 2 :b))"))
    assert ks == [3, 2, 1]


def test_sorted_map_by_predicate_gt():
    ks = list(_ev("(keys (sorted-map-by > 1 :a 2 :b 3 :c))"))
    assert ks == [3, 2, 1]


def test_sorted_map_by_predicate_lt():
    ks = list(_ev("(keys (sorted-map-by < 3 :c 1 :a 2 :b))"))
    assert ks == [1, 2, 3]


def test_sorted_set_by_descending():
    xs = list(_ev("(sorted-set-by > 1 2 3 4 5)"))
    assert xs == [5, 4, 3, 2, 1]


# --- sorted? ---

def test_sorted_pred():
    assert _ev("(sorted? (sorted-map))") is True
    assert _ev("(sorted? (sorted-set))") is True
    assert _ev("(sorted? {})") is False
    assert _ev("(sorted? #{})") is False
    assert _ev("(sorted? [])") is False


# --- rseq ---

def test_rseq_map():
    xs = list(_ev("(rseq (sorted-map 1 :a 2 :b 3 :c))"))
    assert xs[0][0] == 3
    assert xs[1][0] == 2
    assert xs[2][0] == 1


def test_rseq_set():
    assert list(_ev("(rseq (sorted-set 1 2 3))")) == [3, 2, 1]


# --- subseq / rsubseq ---

def test_subseq_gt():
    xs = list(_ev("(subseq (sorted-map 1 :a 2 :b 3 :c 4 :d 5 :e) > 2)"))
    ks = [e[0] for e in xs]
    assert ks == [3, 4, 5]


def test_subseq_gte():
    xs = list(_ev("(subseq (sorted-map 1 :a 2 :b 3 :c 4 :d 5 :e) >= 2)"))
    ks = [e[0] for e in xs]
    assert ks == [2, 3, 4, 5]


def test_subseq_lt():
    xs = list(_ev("(subseq (sorted-map 1 :a 2 :b 3 :c 4 :d 5 :e) < 3)"))
    ks = [e[0] for e in xs]
    assert ks == [1, 2]


def test_subseq_lte():
    xs = list(_ev("(subseq (sorted-map 1 :a 2 :b 3 :c 4 :d 5 :e) <= 3)"))
    ks = [e[0] for e in xs]
    assert ks == [1, 2, 3]


def test_subseq_range():
    xs = list(_ev("(subseq (sorted-map 1 :a 2 :b 3 :c 4 :d 5 :e) >= 2 < 5)"))
    ks = [e[0] for e in xs]
    assert ks == [2, 3, 4]


def test_rsubseq_lt():
    xs = list(_ev("(rsubseq (sorted-map 1 :a 2 :b 3 :c 4 :d 5 :e) < 4)"))
    ks = [e[0] for e in xs]
    assert ks == [3, 2, 1]


def test_rsubseq_range():
    xs = list(_ev("(rsubseq (sorted-map 1 :a 2 :b 3 :c 4 :d 5 :e) > 1 <= 4)"))
    ks = [e[0] for e in xs]
    assert ks == [4, 3, 2]


# --- Equality / hashing ---

def test_sorted_map_eq_hash_map():
    # Two maps (sorted and hash) with the same entries are equal.
    assert _ev("(= (sorted-map 1 :a 2 :b) (hash-map 1 :a 2 :b))") is True


def test_sorted_set_eq_hash_set():
    assert _ev("(= (sorted-set 1 2 3) (hash-set 1 2 3))") is True


# --- Larger-scale sanity ---

def test_sorted_map_many_inserts_then_keys():
    src = "(let* [m (apply sorted-map " \
          "        (interleave (range 100) (repeat :x)))] " \
          "  (vec (keys m)))"
    assert list(_ev(src)) == list(range(100))


def test_sorted_map_delete_preserves_invariant():
    # Insert 0..49, delete 0,2,4,...,48; should leave odds.
    src = """
    (let* [m (apply sorted-map (interleave (range 50) (range 50)))
           drops (range 0 50 2)
           out (reduce (fn* [acc k] (dissoc acc k)) m drops)]
      (vec (keys out)))
    """
    assert list(_ev(src)) == list(range(1, 50, 2))


# --- Hypothesis fuzz: compare against Python sorted dict ---

@settings(
    deadline=None,
    max_examples=40,
    suppress_health_check=[HealthCheck.function_scoped_fixture, HealthCheck.too_slow],
)
@given(
    ops=st.lists(
        st.tuples(
            st.sampled_from(["assoc", "dissoc"]),
            st.integers(-100, 100),
            st.integers(0, 100),
        ),
        min_size=1,
        max_size=60,
    ),
)
def test_sorted_map_fuzz_matches_python_sorted(ops):
    # Reference model.
    ref = {}
    # Build the ops as a Clojure-evaluable sequence.
    _ev("(def --fuzz-m (sorted-map))")
    for kind, k, v in ops:
        if kind == "assoc":
            _ev("(def --fuzz-m (assoc --fuzz-m %d %d))" % (k, v))
            ref[k] = v
        else:
            _ev("(def --fuzz-m (dissoc --fuzz-m %d))" % k)
            ref.pop(k, None)

    got_keys = list(_ev("(vec (keys --fuzz-m))"))
    expected_keys = sorted(ref.keys())
    assert got_keys == expected_keys, (got_keys, expected_keys)

    got_vals = list(_ev("(vec (vals --fuzz-m))"))
    expected_vals = [ref[k] for k in sorted(ref.keys())]
    assert got_vals == expected_vals


@settings(
    deadline=None,
    max_examples=40,
    suppress_health_check=[HealthCheck.function_scoped_fixture, HealthCheck.too_slow],
)
@given(
    ops=st.lists(
        st.tuples(
            st.sampled_from(["conj", "disj"]),
            st.integers(-100, 100),
        ),
        min_size=1,
        max_size=60,
    ),
)
def test_sorted_set_fuzz_matches_python_sorted(ops):
    ref = set()
    _ev("(def --fuzz-s (sorted-set))")
    for kind, x in ops:
        if kind == "conj":
            _ev("(def --fuzz-s (conj --fuzz-s %d))" % x)
            ref.add(x)
        else:
            _ev("(def --fuzz-s (disj --fuzz-s %d))" % x)
            ref.discard(x)
    got = list(_ev("(vec --fuzz-s)"))
    assert got == sorted(ref), (got, sorted(ref))
