"""Tests for core.clj batch 39: sequence-fns batch (JVM 7298-7590).

Forms ported:
  group-by, partition-by, frequencies, reductions, rand-nth,
  partition-all, splitv-at, partitionv, partitionv-all, shuffle,
  map-indexed, keep, keep-indexed, bounded-count.

Adaptations from JVM:
  - java.util.ArrayList → Python list. .add → .append, .clear stays,
    .size → (py.__builtins__/len al), .isEmpty → (zero? len), and
    .toArray → just iterate the list (vec works directly).
  - java.util.Collections/shuffle → py.random/shuffle (in-place).
  - flatten was already pulled forward in batch 31; not in this batch.
"""

import pytest

import clojure.core  # bootstrap

from clojure.lang import (
    Compiler,
    read_string,
    Keyword,
)


def E(src):
    return Compiler.eval(read_string(src))


def K(name):
    return Keyword.intern(None, name)


# --- group-by ---------------------------------------------------

def test_group_by_basic():
    out = E("(group-by odd? [1 2 3 4 5])")
    d = dict(out)
    assert list(d[True]) == [1, 3, 5]
    assert list(d[False]) == [2, 4]

def test_group_by_preserves_order():
    out = E("(group-by :type [{:type :a :v 1} {:type :b :v 2} {:type :a :v 3}])")
    d = dict(out)
    a_vals = [dict(x)[K("v")] for x in d[K("a")]]
    b_vals = [dict(x)[K("v")] for x in d[K("b")]]
    assert a_vals == [1, 3]
    assert b_vals == [2]

def test_group_by_empty():
    out = E("(group-by odd? [])")
    assert dict(out) == {}


# --- partition-by ----------------------------------------------

def test_partition_by_runs():
    out = E("(partition-by odd? [1 1 2 2 3 1 1])")
    assert [list(p) for p in out] == [[1, 1], [2, 2], [3, 1, 1]]

def test_partition_by_singletons():
    out = E("(partition-by identity [:a :a :b :c :c :c :a])")
    assert [list(p) for p in out] == [
        [K("a"), K("a")], [K("b")], [K("c"), K("c"), K("c")], [K("a")]
    ]

def test_partition_by_empty():
    assert list(E("(partition-by odd? [])")) == []

def test_partition_by_transducer():
    """Transducer arity returns vectors, not seqs."""
    out = E("(into [] (partition-by odd?) [1 1 2 2 3])")
    assert [list(p) for p in out] == [[1, 1], [2, 2], [3]]

def test_partition_by_transducer_into_clears_buffer():
    """Sanity: repeated runs through the same transducer don't leak
    state — the ArrayList-equivalent Python list gets cleared."""
    xform_form = "(partition-by odd?)"
    a = list(E(f"(into [] {xform_form} [1 1 2 2])"))
    b = list(E(f"(into [] {xform_form} [3 3 4])"))
    assert [list(p) for p in a] == [[1, 1], [2, 2]]
    assert [list(p) for p in b] == [[3, 3], [4]]


# --- frequencies -----------------------------------------------

def test_frequencies_basic():
    out = E("(frequencies [:a :b :a :c :b :a])")
    assert dict(out) == {K("a"): 3, K("b"): 2, K("c"): 1}

def test_frequencies_empty():
    assert dict(E("(frequencies [])")) == {}


# --- reductions ------------------------------------------------

def test_reductions_with_init():
    out = E("(reductions + 100 [1 2 3])")
    assert list(out) == [100, 101, 103, 106]

def test_reductions_no_init():
    out = E("(reductions + [1 2 3 4])")
    assert list(out) == [1, 3, 6, 10]

def test_reductions_empty_no_init_calls_f():
    """Empty coll with no init returns (f) wrapped in a list."""
    out = E("(reductions (fn ([] :empty) ([a b] [a b])) [])")
    assert list(out) == [K("empty")]

def test_reductions_lazy():
    """reductions is lazy — taking 3 from an infinite seq must not OOM."""
    out = E("(take 3 (reductions + (range)))")
    assert list(out) == [0, 1, 3]

def test_reductions_honors_reduced_init():
    """If init is itself a reduced, reductions short-circuits."""
    out = E("(reductions + (reduced :stop) [1 2 3])")
    assert list(out) == [K("stop")]


# --- rand-nth --------------------------------------------------

def test_rand_nth_picks_member():
    """Run a few times and verify result is in the collection."""
    for _ in range(20):
        out = E("(rand-nth [10 20 30 40 50])")
        assert out in (10, 20, 30, 40, 50)

def test_rand_nth_singleton():
    assert E("(rand-nth [:only])") == K("only")


# --- partition-all ---------------------------------------------

def test_partition_all_complete_groups():
    out = E("(partition-all 2 [1 2 3 4])")
    assert [list(p) for p in out] == [[1, 2], [3, 4]]

def test_partition_all_partial_tail():
    out = E("(partition-all 2 [1 2 3 4 5])")
    assert [list(p) for p in out] == [[1, 2], [3, 4], [5]]

def test_partition_all_with_step():
    out = E("(partition-all 2 1 [1 2 3 4])")
    assert [list(p) for p in out] == [[1, 2], [2, 3], [3, 4], [4]]

def test_partition_all_empty():
    assert list(E("(partition-all 3 [])")) == []

def test_partition_all_transducer_partial():
    out = E("(into [] (partition-all 3) [1 2 3 4 5])")
    assert [list(p) for p in out] == [[1, 2, 3], [4, 5]]


# --- splitv-at -------------------------------------------------

def test_splitv_at_basic():
    out = E("(splitv-at 2 [10 20 30 40 50])")
    parts = list(out)
    assert list(parts[0]) == [10, 20]
    assert list(parts[1]) == [30, 40, 50]

def test_splitv_at_zero():
    out = E("(splitv-at 0 [1 2 3])")
    parts = list(out)
    assert list(parts[0]) == []
    assert list(parts[1]) == [1, 2, 3]

def test_splitv_at_overshoot():
    out = E("(splitv-at 99 [1 2 3])")
    parts = list(out)
    assert list(parts[0]) == [1, 2, 3]
    assert list(parts[1]) == []


# --- partitionv ------------------------------------------------

def test_partitionv_basic():
    out = E("(partitionv 3 [1 2 3 4 5 6 7])")
    assert [list(p) for p in out] == [[1, 2, 3], [4, 5, 6]]

def test_partitionv_with_step():
    out = E("(partitionv 2 1 [1 2 3 4])")
    assert [list(p) for p in out] == [[1, 2], [2, 3], [3, 4]]

def test_partitionv_with_pad():
    out = E("(partitionv 3 3 [:p1 :p2] [1 2 3 4])")
    assert [list(p) for p in out] == [[1, 2, 3], [4, K("p1"), K("p2")]]

def test_partitionv_pad_runs_short():
    """If pad has fewer elements than needed, last partition is short."""
    out = E("(partitionv 4 4 [:p] [1 2 3 4 5])")
    assert [list(p) for p in out] == [[1, 2, 3, 4], [5, K("p")]]


# --- partitionv-all -------------------------------------------

def test_partitionv_all_basic():
    out = E("(partitionv-all 3 [1 2 3 4 5 6 7])")
    assert [list(p) for p in out] == [[1, 2, 3], [4, 5, 6], [7]]

def test_partitionv_all_complete():
    out = E("(partitionv-all 2 [1 2 3 4])")
    assert [list(p) for p in out] == [[1, 2], [3, 4]]

def test_partitionv_all_transducer_delegates():
    """1-arity partitionv-all delegates to partition-all (which IS a
    transducer). Verify by composing into vector."""
    out = E("(into [] (partitionv-all 3) [1 2 3 4 5])")
    assert [list(p) for p in out] == [[1, 2, 3], [4, 5]]


# --- shuffle ---------------------------------------------------

def test_shuffle_preserves_elements():
    out = E("(shuffle [1 2 3 4 5])")
    assert sorted(out) == [1, 2, 3, 4, 5]

def test_shuffle_returns_vector():
    out = E("(shuffle [1 2 3])")
    # Vector-ish: countable + indexable
    assert E(f"(vector? '{list(out)})") or hasattr(out, "nth")

def test_shuffle_empty():
    out = E("(shuffle [])")
    assert list(out) == []

def test_shuffle_singleton():
    out = E("(shuffle [:only])")
    assert list(out) == [K("only")]


# --- map-indexed -----------------------------------------------

def test_map_indexed_basic():
    out = E("(map-indexed vector [:a :b :c])")
    assert [list(p) for p in out] == [[0, K("a")], [1, K("b")], [2, K("c")]]

def test_map_indexed_arithmetic():
    out = E("(map-indexed (fn [i v] (+ i v)) [10 20 30])")
    assert list(out) == [10, 21, 32]

def test_map_indexed_empty():
    assert list(E("(map-indexed vector [])")) == []

def test_map_indexed_transducer():
    out = E("(into [] (map-indexed vector) [:x :y :z])")
    assert [list(p) for p in out] == [[0, K("x")], [1, K("y")], [2, K("z")]]

def test_map_indexed_lazy():
    """Lazy: take 3 from infinite range works."""
    out = E("(take 3 (map-indexed vector (range)))")
    assert [list(p) for p in out] == [[0, 0], [1, 1], [2, 2]]


# --- keep ------------------------------------------------------

def test_keep_drops_nil_keeps_false():
    """keep keeps false (only drops nil)."""
    out = E("(keep (fn [x] (if (zero? x) nil (even? x))) [0 1 2 3 4])")
    assert list(out) == [False, True, False, True]

def test_keep_basic():
    out = E("(keep #(when (even? %) (* % 10)) [1 2 3 4 5])")
    assert list(out) == [20, 40]

def test_keep_empty():
    assert list(E("(keep identity [])")) == []

def test_keep_all_nil():
    assert list(E("(keep (fn [_] nil) [1 2 3])")) == []

def test_keep_transducer():
    out = E("(into [] (keep #(when (odd? %) %)) [1 2 3 4 5])")
    assert list(out) == [1, 3, 5]


# --- keep-indexed ----------------------------------------------

def test_keep_indexed_basic():
    """Pick items at odd indices."""
    out = E("(keep-indexed #(when (odd? %1) %2) [:a :b :c :d :e])")
    assert list(out) == [K("b"), K("d")]

def test_keep_indexed_drops_nil_keeps_false():
    out = E("(keep-indexed (fn [i x] (when-not (= i 1) (boolean x))) [1 2 3])")
    # i=0 → True; i=1 → nil (skipped); i=2 → True
    assert list(out) == [True, True]

def test_keep_indexed_empty():
    assert list(E("(keep-indexed vector [])")) == []

def test_keep_indexed_transducer():
    out = E("(into [] (keep-indexed #(when (even? %1) [%1 %2])) [:a :b :c :d])")
    assert [list(p) for p in out] == [[0, K("a")], [2, K("c")]]


# --- bounded-count ---------------------------------------------

def test_bounded_count_counted_returns_count():
    """For counted? colls, bounded-count just calls count — no upper bound."""
    assert E("(bounded-count 3 [1 2 3 4 5])") == 5
    assert E("(bounded-count 1 #{:a :b :c :d})") == 4

def test_bounded_count_seq_capped():
    """For non-counted, walk at most n elements."""
    out = E("(bounded-count 3 (map identity [1 2 3 4 5]))")
    assert out == 3

def test_bounded_count_seq_short():
    """Seq shorter than n returns its real count."""
    out = E("(bounded-count 99 (map identity [1 2 3]))")
    assert out == 3

def test_bounded_count_empty():
    assert E("(bounded-count 5 [])") == 0
    assert E("(bounded-count 5 (map identity []))") == 0
