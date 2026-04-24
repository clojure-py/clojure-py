"""Tests for slicing extensions (split-at/split-with/take-last/drop-last/
take-nth/nthnext/nthrest), map aggregation (merge/merge-with/zipmap),
grouping (group-by/frequencies/reductions), predicate combinators
(some-fn/every-pred), flatten + tree-seq + sequential?."""

import pytest
from hypothesis import given, strategies as st, settings
from clojure._core import eval_string, keyword


def _ev(src):
    return eval_string(src)


def _vec(src):
    return list(eval_string(f"(vec {src})"))


# --- split-at / split-with ---

def test_split_at():
    r = _ev("(split-at 2 [1 2 3 4 5])")
    # Result is [left right] — two seqs.
    assert list(r[0]) == [1, 2]
    assert list(r[1]) == [3, 4, 5]


def test_split_at_beyond_len():
    r = _ev("(split-at 10 [1 2 3])")
    assert list(r[0]) == [1, 2, 3]
    assert list(r[1]) == []


def test_split_with():
    r = _ev("(split-with pos? [1 2 -1 3])")
    assert list(r[0]) == [1, 2]
    assert list(r[1]) == [-1, 3]


# --- take-last / drop-last / take-nth ---

def test_take_last():
    assert list(_ev("(take-last 2 [1 2 3 4 5])")) == [4, 5]
    # `(take-last 0 ...)` returns nil (empty seq).
    assert _ev("(take-last 0 [1 2 3])") is None


def test_take_last_beyond_len():
    assert list(_ev("(take-last 10 [1 2 3])")) == [1, 2, 3]


def test_drop_last_default():
    assert _vec("(drop-last [1 2 3 4 5])") == [1, 2, 3, 4]


def test_drop_last_n():
    assert _vec("(drop-last 3 [1 2 3 4 5])") == [1, 2]


def test_drop_last_beyond_len():
    assert _vec("(drop-last 10 [1 2 3])") == []


def test_take_nth():
    assert _vec("(take-nth 2 (range 10))") == [0, 2, 4, 6, 8]
    assert _vec("(take-nth 3 (range 10))") == [0, 3, 6, 9]


# --- nthnext / nthrest ---

def test_nthnext():
    assert list(_ev("(nthnext [1 2 3 4 5] 2)")) == [3, 4, 5]
    assert _ev("(nthnext [1 2 3] 0)") is not None
    assert list(_ev("(nthnext [1 2 3] 0)")) == [1, 2, 3]
    assert _ev("(nthnext [1 2 3] 10)") is None


def test_nthrest():
    assert _vec("(nthrest [1 2 3 4 5] 2)") == [3, 4, 5]
    assert _vec("(nthrest [1 2 3] 0)") == [1, 2, 3]


# --- merge / merge-with / zipmap ---

def test_merge_basic():
    m = _ev("(merge {:a 1} {:b 2})")
    assert m[keyword("a")] == 1
    assert m[keyword("b")] == 2


def test_merge_overwrites_right_wins():
    m = _ev("(merge {:a 1} {:a 2})")
    assert m[keyword("a")] == 2


def test_merge_nil_maps_skipped():
    m = _ev("(merge {:a 1} nil {:b 2})")
    assert m[keyword("a")] == 1
    assert m[keyword("b")] == 2


def test_merge_all_nil_returns_nil():
    assert _ev("(merge nil nil)") is None


def test_merge_with_f_on_conflict():
    m = _ev("(merge-with + {:a 1 :b 2} {:a 10} {:b 20 :c 30})")
    assert m[keyword("a")] == 11
    assert m[keyword("b")] == 22
    assert m[keyword("c")] == 30


def test_merge_with_only_conflicts_apply_f():
    # keys only in one map shouldn't invoke f
    m = _ev("(merge-with + {:a 1} {:b 2})")
    assert m[keyword("a")] == 1
    assert m[keyword("b")] == 2


def test_zipmap():
    m = _ev("(zipmap [:a :b :c] [1 2 3])")
    assert m[keyword("a")] == 1
    assert m[keyword("b")] == 2
    assert m[keyword("c")] == 3


def test_zipmap_short_vals_truncates():
    m = _ev("(zipmap [:a :b :c] [1 2])")
    assert m[keyword("a")] == 1
    assert m[keyword("b")] == 2


def test_zipmap_short_keys_truncates():
    m = _ev("(zipmap [:a :b] [1 2 3])")
    assert m[keyword("a")] == 1
    assert m[keyword("b")] == 2


# --- group-by / frequencies / reductions ---

def test_group_by_even():
    g = _ev("(group-by even? (range 6))")
    assert list(g[True]) == [0, 2, 4]
    assert list(g[False]) == [1, 3, 5]


def test_group_by_keyfn():
    g = _ev("(group-by count [\"a\" \"bb\" \"cc\" \"ddd\"])")
    assert list(g[1]) == ["a"]
    assert sorted(list(g[2])) == ["bb", "cc"]
    assert list(g[3]) == ["ddd"]


def test_frequencies_basic():
    f = _ev("(frequencies [:a :b :a :c :a :b])")
    assert f[keyword("a")] == 3
    assert f[keyword("b")] == 2
    assert f[keyword("c")] == 1


def test_frequencies_empty():
    f = _ev("(frequencies [])")
    assert len(f) == 0


def test_reductions_no_init():
    assert _vec("(reductions + [1 2 3 4])") == [1, 3, 6, 10]


def test_reductions_with_init():
    assert _vec("(reductions + 100 [1 2 3])") == [100, 101, 103, 106]


def test_reductions_empty_with_init():
    # (reductions f init []) → (init)
    assert _vec("(reductions + 42 [])") == [42]


def test_reductions_empty_no_init():
    # (reductions f []) → ((f))  — (+) = 0
    assert _vec("(reductions + [])") == [0]


def test_reductions_laziness():
    # Taking the first 5 of (reductions + (range)) must not force the whole
    # infinite seq.
    assert _vec("(take 5 (reductions + (range)))") == [0, 1, 3, 6, 10]


# --- some-fn / every-pred ---

def test_every_pred_single():
    assert _ev("((every-pred pos?) 5)") is True
    assert _ev("((every-pred pos?) -5)") is False


def test_every_pred_composite():
    assert _ev("((every-pred pos? even?) 4)") is True
    assert _ev("((every-pred pos? even?) 3)") is False
    assert _ev("((every-pred pos? even?) -4)") is False


def test_every_pred_many_args():
    assert _ev("((every-pred pos?) 1 2 3)") is True
    assert _ev("((every-pred pos?) 1 -1 3)") is False


def test_every_pred_zero_args():
    # Vacuous case.
    assert _ev("((every-pred pos?))") is True


def test_some_fn_single():
    assert _ev("((some-fn pos?) -1)") is False
    assert _ev("((some-fn pos?) 1)") is True


def test_some_fn_composite():
    assert _ev("((some-fn pos? even?) -4)") is True
    assert _ev("((some-fn pos? even?) -3)") is False


def test_some_fn_many_args():
    assert _ev("((some-fn pos?) -1 -2 3)") is True
    assert _ev("((some-fn pos?) -1 -2 -3)") is False


# --- flatten / tree-seq / sequential? ---

def test_sequential_pred():
    assert _ev("(sequential? [1 2 3])") is True
    assert _ev("(sequential? '(1 2 3))") is True
    assert _ev("(sequential? {:a 1})") is False
    assert _ev("(sequential? #{1 2})") is False
    assert _ev('(sequential? "str")') is False


def test_flatten_basic():
    assert _vec("(flatten [[1 2] [3 [4 5]]])") == [1, 2, 3, 4, 5]


def test_flatten_deep():
    assert _vec("(flatten [1 [2 [3 [4 [5]]]]])") == [1, 2, 3, 4, 5]


def test_flatten_already_flat():
    assert _vec("(flatten [1 2 3])") == [1, 2, 3]


def test_flatten_empty():
    assert _vec("(flatten [])") == []


def test_flatten_preserves_non_sequentials():
    # Maps/sets aren't sequential — they stay as-is inside a flattened seq.
    r = _ev("(first (flatten [[{:a 1}] [:kw]]))")
    # The first element is the map, not flattened further.
    assert r[keyword("a")] == 1


def test_tree_seq_basic():
    # tree-seq of a nested vector, children = seq, branch? = sequential?
    nodes = _vec("(tree-seq sequential? seq [[1 2] [3 [4 5]]])")
    # First is the root vector, then its children (flattened DFS).
    # We check the leaves.
    leaves = [n for n in nodes if not isinstance(n, (list, tuple)) and not hasattr(n, '__iter__') or isinstance(n, int)]
    # Rough check: all leaf ints are present somewhere in DFS order.
    ints = [n for n in nodes if isinstance(n, int)]
    assert ints == [1, 2, 3, 4, 5]


# --- Hypothesis: frequencies matches Python's Counter ---

@settings(max_examples=100, deadline=None)
@given(items=st.lists(st.integers(-20, 20), min_size=0, max_size=50))
def test_frequencies_matches_counter(items):
    from collections import Counter
    src = "[" + " ".join(str(x) for x in items) + "]"
    got = _ev(f"(frequencies {src})")
    want = Counter(items)
    assert len(got) == len(want)
    for k, v in want.items():
        assert got[k] == v


@settings(max_examples=100, deadline=None)
@given(items=st.lists(st.integers(-20, 20), min_size=0, max_size=30))
def test_reductions_last_matches_reduce(items):
    if not items:
        return
    src = "[" + " ".join(str(x) for x in items) + "]"
    last = _ev(f"(last (reductions + {src}))")
    assert last == sum(items)
