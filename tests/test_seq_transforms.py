"""Tests for the sequence-transform chunk: closures (comp/juxt/partial),
predicate combinators (every?/some/...), map/filter/remove/keep, slicing
(take/drop), generators (range/iterate/repeat/cycle/repeatedly), and
concat helpers (mapcat/interleave/interpose)."""

import functools
import pytest
from hypothesis import given, strategies as st, settings
from clojure._core import eval_string, keyword


def _ev(src):
    return eval_string(src)


def _vec(src):
    """Force lazy seq into a Python list for assertion."""
    return list(eval_string(f"(vec {src})"))


# --- Closure utilities ---

def test_comp_identity():
    assert _ev("((comp) 42)") == 42


def test_comp_single():
    assert _ev("((comp inc) 5)") == 6


def test_comp_two():
    assert _ev("((comp inc inc) 5)") == 7


def test_comp_three():
    assert _ev("((comp inc inc inc) 0)") == 3


def test_comp_n():
    # Composition right-to-left: ((comp f g h) x) == (f (g (h x)))
    assert _ev("((comp inc inc inc inc inc) 0)") == 5


def test_juxt_single():
    r = _ev("((juxt inc) 5)")
    assert list(r) == [6]


def test_juxt_two():
    assert list(_ev("((juxt first last) [1 2 3])")) == [1, 3]


def test_juxt_three():
    assert list(_ev("((juxt first second last) [1 2 3 4 5])")) == [1, 2, 5]


def test_partial_zero_extra():
    # (partial f) returns f
    assert _ev("((partial inc) 5)") == 6


def test_partial_one():
    assert _ev("((partial + 10) 5)") == 15


def test_partial_many():
    assert _ev("((partial + 1 2 3) 10 20)") == 36


# --- Predicate combinators ---

def test_every_pred():
    assert _ev("(every? pos? [1 2 3])") is True
    assert _ev("(every? pos? [1 -1 3])") is False
    assert _ev("(every? pos? [])") is True  # vacuous


def test_some_pred():
    assert _ev("(some pos? [-1 -2 3])") is True
    assert _ev("(some pos? [-1 -2 -3])") is None
    assert _ev("(some pos? [])") is None


def test_not_every():
    assert _ev("(not-every? pos? [1 -1 3])") is True
    assert _ev("(not-every? pos? [1 2 3])") is False


def test_not_any():
    assert _ev("(not-any? neg? [1 2 3])") is True
    assert _ev("(not-any? neg? [1 -1 3])") is False


# --- map / filter / remove / keep ---

def test_map_1coll():
    assert _vec("(map inc [1 2 3])") == [2, 3, 4]


def test_map_2coll():
    assert _vec("(map + [1 2 3] [10 20 30])") == [11, 22, 33]


def test_map_3coll():
    assert _vec("(map + [1 2 3] [10 20 30] [100 200 300])") == [111, 222, 333]


def test_map_stops_at_shortest():
    assert _vec("(map + [1 2 3 4] [10 20])") == [11, 22]


def test_filter():
    assert _vec("(filter even? (range 10))") == [0, 2, 4, 6, 8]


def test_remove():
    assert _vec("(remove even? (range 6))") == [1, 3, 5]


def test_keep_filters_nil():
    # Only positive x yields a non-nil result.
    r = _vec("(keep (fn [x] (if (pos? x) (* x 10) nil)) [-1 2 -3 4])")
    assert r == [20, 40]


# --- Slicing ---

def test_take_finite():
    assert _vec("(take 3 [1 2 3 4 5])") == [1, 2, 3]


def test_take_of_infinite():
    assert _vec("(take 5 (iterate inc 0))") == [0, 1, 2, 3, 4]


def test_take_beyond_len():
    assert _vec("(take 10 [1 2 3])") == [1, 2, 3]


def test_drop_finite():
    assert _vec("(drop 3 (range 10))") == [3, 4, 5, 6, 7, 8, 9]


def test_drop_beyond_len():
    assert _vec("(drop 10 [1 2 3])") == []


def test_take_while():
    assert _vec("(take-while pos? [1 2 3 -1 4])") == [1, 2, 3]


def test_drop_while():
    assert _vec("(drop-while pos? [1 2 3 -1 4])") == [-1, 4]


# --- Generators ---

def test_range_zero_arity_is_infinite():
    # Can't force the whole thing; just take some prefix.
    assert _vec("(take 5 (range))") == [0, 1, 2, 3, 4]


def test_range_one_arity():
    assert _vec("(range 5)") == [0, 1, 2, 3, 4]
    assert _vec("(range 0)") == []
    assert _vec("(range -3)") == []


def test_range_two_arity():
    assert _vec("(range 2 8)") == [2, 3, 4, 5, 6, 7]


def test_range_three_arity_positive_step():
    assert _vec("(range 0 10 2)") == [0, 2, 4, 6, 8]


def test_range_three_arity_negative_step():
    assert _vec("(range 10 2 -2)") == [10, 8, 6, 4]


def test_iterate_prefix():
    assert _vec("(take 6 (iterate (partial * 2) 1))") == [1, 2, 4, 8, 16, 32]


def test_repeat_n():
    assert _vec("(repeat 4 :x)") == [keyword("x")] * 4


def test_repeat_infinite_take():
    assert _vec("(take 3 (repeat :y))") == [keyword("y")] * 3


def test_cycle():
    assert _vec("(take 7 (cycle [:a :b :c]))") == [
        keyword("a"), keyword("b"), keyword("c"),
        keyword("a"), keyword("b"), keyword("c"),
        keyword("a"),
    ]


def test_repeatedly_bounded():
    # repeatedly with a stateful Python counter.
    import clojure._core as c
    counter = [0]
    def tick():
        counter[0] += 1
        return counter[0]
    rt = c.find_ns(c.symbol("clojure.lang.RT"))
    repeatedly_var = None
    core_ns = c.find_ns(c.symbol("clojure.core"))
    repeatedly_fn = getattr(core_ns, "repeatedly")
    vec_fn = getattr(core_ns, "vec")
    take_fn = getattr(core_ns, "take")
    result = vec_fn(take_fn(5, repeatedly_fn(tick)))
    assert list(result) == [1, 2, 3, 4, 5]


# --- Concat helpers ---

def test_mapcat_1coll():
    assert _vec("(mapcat (fn [x] [x x]) [1 2 3])") == [1, 1, 2, 2, 3, 3]


def test_mapcat_n_coll():
    # mapcat with 2 colls should interleave-and-concat.
    assert _vec("(mapcat (fn [a b] [a b]) [1 2 3] [:a :b :c])") == [
        1, keyword("a"), 2, keyword("b"), 3, keyword("c"),
    ]


def test_interleave_two():
    assert _vec("(interleave [1 2 3] [:a :b :c])") == [
        1, keyword("a"), 2, keyword("b"), 3, keyword("c"),
    ]


def test_interleave_stops_at_shortest():
    assert _vec("(interleave [1 2 3] [:a :b])") == [1, keyword("a"), 2, keyword("b")]


def test_interleave_empty():
    assert _vec("(interleave)") == []


def test_interpose():
    assert _vec("(interpose :x [1 2 3])") == [1, keyword("x"), 2, keyword("x"), 3]


def test_interpose_empty():
    assert _vec("(interpose :x [])") == []


def test_interpose_single():
    assert _vec("(interpose :x [42])") == [42]


# --- Laziness verification ---

def test_filter_stays_lazy_on_infinite_source():
    # Would loop forever if filter eagerly consumed (range).
    assert _ev("(first (filter even? (range)))") == 0


def test_map_stays_lazy_on_infinite_source():
    assert _ev("(first (map inc (range)))") == 1


# --- Hypothesis: map matches Python map across random int lists ---

small_ints = st.integers(-100, 100)


@settings(max_examples=100, deadline=None)
@given(items=st.lists(small_ints, min_size=0, max_size=30))
def test_map_inc_matches_python(items):
    src = "[" + " ".join(str(x) for x in items) + "]"
    got = list(eval_string(f"(vec (map inc {src}))"))
    assert got == [x + 1 for x in items]


@settings(max_examples=100, deadline=None)
@given(items=st.lists(small_ints, min_size=0, max_size=30))
def test_filter_even_matches_python(items):
    src = "[" + " ".join(str(x) for x in items) + "]"
    got = list(eval_string(f"(vec (filter even? {src}))"))
    assert got == [x for x in items if x % 2 == 0]


@settings(max_examples=100, deadline=None)
@given(n=st.integers(0, 20),
       items=st.lists(small_ints, min_size=0, max_size=30))
def test_take_matches_slice(n, items):
    src = "[" + " ".join(str(x) for x in items) + "]"
    got = list(eval_string(f"(vec (take {n} {src}))"))
    assert got == items[:n]
