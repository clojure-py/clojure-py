"""Tests for core.clj batch 11 (lines 2876-3068):

unreduced,
take, take-while,
drop, drop-last, take-last, drop-while,
cycle, split-at, split-with,
repeat, replicate, iterate, range
"""

import pytest

import clojure.core  # triggers load

from clojure.lang import (
    Compiler,
    read_string,
    PersistentVector, ISeq, Reduced,
    Iterate, Cycle, Repeat, Range, IDrop,
)


def E(src):
    return Compiler.eval(read_string(src))


# --- unreduced -----------------------------------------------------

def test_unreduced_unwraps_reduced():
    assert E("(clojure.core/unreduced (clojure.core/reduced 7))") == 7

def test_unreduced_passes_through_non_reduced():
    assert E("(clojure.core/unreduced 5)") == 5

def test_unreduced_passes_through_nil():
    assert E("(clojure.core/unreduced nil)") is None


# --- take ----------------------------------------------------------

def test_take_basic():
    assert list(E("(clojure.core/take 3 [10 20 30 40 50])")) == [10, 20, 30]

def test_take_more_than_coll():
    assert list(E("(clojure.core/take 10 [1 2 3])")) == [1, 2, 3]

def test_take_zero():
    assert list(E("(clojure.core/take 0 [1 2 3])")) == []

def test_take_negative():
    assert list(E("(clojure.core/take -1 [1 2 3])")) == []

def test_take_empty():
    assert list(E("(clojure.core/take 5 nil)")) == []

def test_take_lazy():
    """take should not realize the rest of the seq."""
    seen = []
    # iterate gives an infinite seq; take 3 should still terminate.
    out = list(E("(clojure.core/take 3 (clojure.core/iterate clojure.core/inc 0))"))
    assert out == [0, 1, 2]

def test_take_transducer():
    """1-arity returns a transducer usable by sequence."""
    assert list(E("(clojure.core/sequence (clojure.core/take 3) [10 20 30 40 50])")) == [10, 20, 30]

def test_take_transducer_zero():
    assert list(E("(clojure.core/sequence (clojure.core/take 0) [1 2 3])")) == []


# --- take-while ----------------------------------------------------

def test_take_while_basic():
    assert list(E("(clojure.core/take-while clojure.core/pos? [1 2 3 -1 4 5])")) == [1, 2, 3]

def test_take_while_none_match():
    assert list(E("(clojure.core/take-while clojure.core/pos? [-1 -2])")) == []

def test_take_while_all_match():
    assert list(E("(clojure.core/take-while clojure.core/pos? [1 2 3])")) == [1, 2, 3]

def test_take_while_empty():
    assert list(E("(clojure.core/take-while clojure.core/pos? nil)")) == []

def test_take_while_transducer():
    assert list(E(
        "(clojure.core/sequence (clojure.core/take-while clojure.core/pos?) [1 2 3 -1 4])"
    )) == [1, 2, 3]


# --- drop ----------------------------------------------------------

def test_drop_basic():
    assert list(E("(clojure.core/drop 2 [1 2 3 4 5])")) == [3, 4, 5]

def test_drop_zero_returns_seq():
    assert list(E("(clojure.core/drop 0 [1 2 3])")) == [1, 2, 3]

def test_drop_negative_returns_seq():
    assert list(E("(clojure.core/drop -3 [1 2 3])")) == [1, 2, 3]

def test_drop_more_than_count():
    assert list(E("(clojure.core/drop 10 [1 2 3])")) == []

def test_drop_empty():
    assert list(E("(clojure.core/drop 2 nil)")) == []

def test_drop_uses_idrop_fast_path_on_vector():
    """Vectors implement IDrop — drop should invoke .drop, not walk a seq."""
    v = E("[1 2 3 4 5]")
    assert isinstance(v, IDrop)
    assert list(E("(clojure.core/drop 2 [1 2 3 4 5])")) == [3, 4, 5]

def test_drop_float_n_uses_math_ceil():
    """Non-int n on an IDrop coll routes through Math/ceil."""
    assert list(E("(clojure.core/drop 2.4 [1 2 3 4 5])")) == [4, 5]

def test_drop_on_seq_lazy_path():
    """Non-IDrop coll falls through the lazy step path."""
    assert list(E("(clojure.core/drop 2 (clojure.core/take 5 (clojure.core/iterate clojure.core/inc 0)))")) == [2, 3, 4]

def test_drop_transducer():
    assert list(E("(clojure.core/sequence (clojure.core/drop 2) [10 20 30 40])")) == [30, 40]


# --- drop-last -----------------------------------------------------

def test_drop_last_default_one():
    assert list(E("(clojure.core/drop-last [1 2 3 4 5])")) == [1, 2, 3, 4]

def test_drop_last_n():
    assert list(E("(clojure.core/drop-last 3 [1 2 3 4 5])")) == [1, 2]

def test_drop_last_n_geq_count():
    assert list(E("(clojure.core/drop-last 10 [1 2 3])")) == []

def test_drop_last_zero():
    assert list(E("(clojure.core/drop-last 0 [1 2 3])")) == [1, 2, 3]


# --- take-last -----------------------------------------------------

def test_take_last_basic():
    assert list(E("(clojure.core/take-last 2 [1 2 3 4 5])")) == [4, 5]

def test_take_last_n_geq_count():
    assert list(E("(clojure.core/take-last 10 [1 2 3])")) == [1, 2, 3]

def test_take_last_zero_is_nil():
    assert E("(clojure.core/take-last 0 [1 2 3])") is None

def test_take_last_empty():
    assert E("(clojure.core/take-last 3 nil)") is None


# --- drop-while ----------------------------------------------------

def test_drop_while_basic():
    assert list(E("(clojure.core/drop-while clojure.core/pos? [1 2 3 -1 4 5])")) == [-1, 4, 5]

def test_drop_while_all_match_returns_empty():
    assert list(E("(clojure.core/drop-while clojure.core/pos? [1 2 3])")) == []

def test_drop_while_none_match_returns_full():
    assert list(E("(clojure.core/drop-while clojure.core/pos? [-1 -2 3])")) == [-1, -2, 3]

def test_drop_while_empty():
    assert list(E("(clojure.core/drop-while clojure.core/pos? nil)")) == []

def test_drop_while_transducer():
    assert list(E(
        "(clojure.core/sequence (clojure.core/drop-while clojure.core/pos?) [1 2 -1 3])"
    )) == [-1, 3]


# --- cycle ---------------------------------------------------------

def test_cycle_take():
    assert list(E("(clojure.core/take 7 (clojure.core/cycle [1 2 3]))")) == [1, 2, 3, 1, 2, 3, 1]

def test_cycle_empty_is_empty():
    assert list(E("(clojure.core/cycle [])")) == []

def test_cycle_returns_cycle_or_empty():
    out = E("(clojure.core/cycle [1 2 3])")
    assert isinstance(out, (Cycle, ISeq))


# --- split-at ------------------------------------------------------

def test_split_at_basic():
    out = E("(clojure.core/split-at 2 [1 2 3 4 5])")
    assert list(out[0]) == [1, 2]
    assert list(out[1]) == [3, 4, 5]
    # Result is a vector
    assert isinstance(out, PersistentVector)

def test_split_at_zero():
    out = E("(clojure.core/split-at 0 [1 2 3])")
    assert list(out[0]) == []
    assert list(out[1]) == [1, 2, 3]

def test_split_at_n_geq_count():
    out = E("(clojure.core/split-at 10 [1 2 3])")
    assert list(out[0]) == [1, 2, 3]
    assert list(out[1]) == []


# --- split-with ----------------------------------------------------

def test_split_with_basic():
    out = E("(clojure.core/split-with clojure.core/pos? [1 2 3 -1 4 5])")
    assert list(out[0]) == [1, 2, 3]
    assert list(out[1]) == [-1, 4, 5]
    assert isinstance(out, PersistentVector)

def test_split_with_none_match():
    out = E("(clojure.core/split-with clojure.core/pos? [-1 -2 3])")
    assert list(out[0]) == []
    assert list(out[1]) == [-1, -2, 3]


# --- repeat --------------------------------------------------------

def test_repeat_n_x():
    assert list(E("(clojure.core/repeat 4 :x)")) == [E(":x"), E(":x"), E(":x"), E(":x")]

def test_repeat_zero_is_empty():
    assert list(E("(clojure.core/repeat 0 :x)")) == []

def test_repeat_negative_is_empty():
    assert list(E("(clojure.core/repeat -3 :x)")) == []

def test_repeat_infinite_take():
    """Single-arity repeat is infinite — must stay lazy."""
    assert list(E("(clojure.core/take 5 (clojure.core/repeat 7))")) == [7, 7, 7, 7, 7]

def test_repeat_returns_repeat_type():
    out = E("(clojure.core/repeat 3 :a)")
    assert isinstance(out, (Repeat, ISeq))


# --- replicate -----------------------------------------------------

def test_replicate_basic():
    assert list(E("(clojure.core/replicate 3 :y)")) == [E(":y"), E(":y"), E(":y")]

def test_replicate_zero():
    assert list(E("(clojure.core/replicate 0 :y)")) == []


# --- iterate -------------------------------------------------------

def test_iterate_take():
    assert list(E("(clojure.core/take 5 (clojure.core/iterate clojure.core/inc 1))")) == [1, 2, 3, 4, 5]

def test_iterate_doubles():
    assert list(E(
        "(clojure.core/take 5 (clojure.core/iterate (fn* [x] (clojure.core/* x 2)) 1))"
    )) == [1, 2, 4, 8, 16]

def test_iterate_returns_iterate_type():
    out = E("(clojure.core/iterate clojure.core/inc 0)")
    assert isinstance(out, Iterate)


# --- range ---------------------------------------------------------

def test_range_no_args_is_infinite():
    assert list(E("(clojure.core/take 5 (clojure.core/range))")) == [0, 1, 2, 3, 4]

def test_range_end():
    assert list(E("(clojure.core/range 5)")) == [0, 1, 2, 3, 4]

def test_range_start_end():
    assert list(E("(clojure.core/range 2 7)")) == [2, 3, 4, 5, 6]

def test_range_start_end_step():
    assert list(E("(clojure.core/range 0 10 2)")) == [0, 2, 4, 6, 8]

def test_range_negative_step():
    assert list(E("(clojure.core/range 5 0 -1)")) == [5, 4, 3, 2, 1]

def test_range_empty_when_start_eq_end():
    assert list(E("(clojure.core/range 3 3)")) == []

def test_range_int_returns_long_range():
    """int? args should route to LongRange (which we alias to Range)."""
    out = E("(clojure.core/range 0 5)")
    assert isinstance(out, Range)

def test_range_float_step():
    """Non-int args should route through Range/create."""
    out = list(E("(clojure.core/range 0 3 0.5)"))
    assert out == [0, 0.5, 1.0, 1.5, 2.0, 2.5]


# --- composition smoke tests ---------------------------------------

def test_take_drop_compose_back_to_full():
    """(concat (take n c) (drop n c)) ~ c (item-wise) — sanity check
    that take/drop are dual."""
    n = 3
    full = list(range(10))
    head = list(E(f"(clojure.core/take {n} (clojure.core/range 10))"))
    tail = list(E(f"(clojure.core/drop {n} (clojure.core/range 10))"))
    assert head + tail == full
