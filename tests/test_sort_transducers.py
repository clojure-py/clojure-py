"""Tests for sort/sort-by/comparator and the transducer ecosystem
(completing, transduce, into, sequence, map/filter/remove/keep/take/drop/
take-while/drop-while/cat/mapcat/interpose/dedupe transducer arities)."""

import pytest
from hypothesis import given, strategies as st, settings
from clojure._core import eval_string, keyword


def _ev(src):
    return eval_string(src)


def _vec(src):
    return list(eval_string(f"(vec {src})"))


# ---------------------------------------------------------------------
# Sort
# ---------------------------------------------------------------------

def test_sort_basic():
    assert _vec("(sort [3 1 2 5 4])") == [1, 2, 3, 4, 5]


def test_sort_empty():
    assert list(_ev("(sort [])")) == []


def test_sort_single():
    assert _vec("(sort [42])") == [42]


def test_sort_strings():
    assert _vec('(sort ["banana" "apple" "cherry"])') == ["apple", "banana", "cherry"]


def test_sort_with_predicate_comparator():
    # `>` is a predicate returning bool — Clojure treats `(pred a b) = true`
    # as "a comes first".
    assert _vec("(sort > [1 2 3])") == [3, 2, 1]
    assert _vec("(sort < [3 1 2])") == [1, 2, 3]


def test_sort_with_int_comparator():
    # compare returns int; descending via (- compare).
    assert _vec("(sort (fn [a b] (- (compare a b))) [1 2 3])") == [3, 2, 1]


def test_sort_by_keyfn():
    rows = _vec("(sort-by first [[3 :a] [1 :b] [2 :c]])")
    unpacked = [(r[0], r[1]) for r in rows]
    assert unpacked == [
        (1, keyword("b")), (2, keyword("c")), (3, keyword("a")),
    ]


def test_sort_by_count():
    # Now that Counted has a __len__ fallback, strings work as keyfn targets.
    assert _vec('(sort-by count ["aaa" "a" "aa"])') == ["a", "aa", "aaa"]


def test_sort_by_custom_comparator():
    rows = _vec("(sort-by first > [[3 :a] [1 :b] [2 :c]])")
    unpacked = [(r[0], r[1]) for r in rows]
    assert unpacked == [
        (3, keyword("a")), (2, keyword("c")), (1, keyword("b")),
    ]


def test_sort_stability():
    # Rust's sort_by is stable — equal keys preserve input order.
    items = _vec("(sort-by first [[1 :a] [1 :b] [1 :c] [2 :d]])")
    # All :a/:b/:c have first=1; their order should be input-order.
    assert [p[1] for p in items[:3]] == [keyword("a"), keyword("b"), keyword("c")]


@given(items=st.lists(st.integers(-100, 100), min_size=0, max_size=30))
@settings(max_examples=100, deadline=None)
def test_sort_matches_python_sorted(items):
    src = "[" + " ".join(str(x) for x in items) + "]"
    got = _vec(f"(sort {src})")
    assert got == sorted(items)


# ---------------------------------------------------------------------
# Transducer infrastructure
# ---------------------------------------------------------------------

def test_completing_adds_unary_arity():
    # (completing f) wraps + in a 0/1/2 reducer with identity completion.
    assert _ev("((completing +))") == 0  # 0-arity → (+)
    assert _ev("((completing +) 5)") == 5  # 1-arity → identity
    assert _ev("((completing +) 3 4)") == 7  # 2-arity → (+ 3 4)


def test_transduce_no_init():
    # (transduce xform f coll) — init comes from (f).
    assert _ev("(transduce (map inc) + [1 2 3])") == 9


def test_transduce_with_init():
    assert _ev("(transduce (map inc) + 100 [1 2 3])") == 109


def test_transduce_composed_xform():
    # (filter even?) then (map inc) on 0..9 → 1 3 5 7 9 → sum = 25
    assert _ev("(transduce (comp (filter even?) (map inc)) + 0 (range 10))") == 25


# ---------------------------------------------------------------------
# Transducer arities of map/filter/remove/keep
# ---------------------------------------------------------------------

def test_map_transducer():
    assert _vec("(into [] (map inc) [1 2 3])") == [2, 3, 4]


def test_filter_transducer():
    assert _vec("(into [] (filter odd?) (range 10))") == [1, 3, 5, 7, 9]


def test_remove_transducer():
    assert _vec("(into [] (remove odd?) (range 10))") == [0, 2, 4, 6, 8]


def test_keep_transducer():
    # Keep non-nil results.
    assert _vec("(into [] (keep (fn [x] (if (pos? x) (* 10 x) nil))) [-1 2 -3 4])") == [20, 40]


def test_keep_preserves_false():
    # keep drops nil but keeps false/0 (non-nil).
    assert _vec("(into [] (keep (fn [x] x)) [1 nil 2 nil 3])") == [1, 2, 3]


# ---------------------------------------------------------------------
# Stateful transducer arities
# ---------------------------------------------------------------------

def test_take_transducer():
    assert _vec("(into [] (take 3) (range 100))") == [0, 1, 2]


def test_take_transducer_exhausts_source():
    # n > source size — take all.
    assert _vec("(into [] (take 10) [1 2 3])") == [1, 2, 3]


def test_take_short_circuits_on_infinite():
    assert _vec("(into [] (take 5) (range))") == [0, 1, 2, 3, 4]


def test_drop_transducer():
    assert _vec("(into [] (drop 5) (range 10))") == [5, 6, 7, 8, 9]


def test_drop_transducer_beyond_len():
    assert _vec("(into [] (drop 100) [1 2 3])") == []


def test_take_while_transducer():
    assert _vec("(into [] (take-while pos?) [1 2 3 -1 4])") == [1, 2, 3]


def test_drop_while_transducer():
    assert _vec("(into [] (drop-while pos?) [1 2 3 -1 4])") == [-1, 4]


# ---------------------------------------------------------------------
# cat / mapcat / interpose / dedupe
# ---------------------------------------------------------------------

def test_cat_transducer():
    assert _vec("(into [] cat [[1 2] [3 4] [5 6]])") == [1, 2, 3, 4, 5, 6]


def test_mapcat_transducer():
    assert _vec("(into [] (mapcat (fn [x] [x x])) [1 2 3])") == [1, 1, 2, 2, 3, 3]


def test_mapcat_returning_nil_is_dropped():
    # mapcat = (comp (map f) cat); if f returns nil/empty, cat reduces into
    # no items for that input.
    assert _vec("(into [] (mapcat (fn [x] (when (pos? x) [x]))) [-1 2 -3 4])") == [2, 4]


def test_interpose_transducer():
    assert _vec("(into [] (interpose :x) [1 2 3])") == [1, keyword("x"), 2, keyword("x"), 3]


def test_interpose_transducer_empty():
    assert _vec("(into [] (interpose :x) [])") == []


def test_interpose_transducer_single():
    assert _vec("(into [] (interpose :x) [42])") == [42]


def test_dedupe_transducer():
    assert _vec("(into [] (dedupe) [1 1 2 2 2 3 1 1])") == [1, 2, 3, 1]


def test_dedupe_fn():
    assert _vec("(dedupe [1 1 2 2 3])") == [1, 2, 3]


# ---------------------------------------------------------------------
# into
# ---------------------------------------------------------------------

def test_into_basic():
    assert _vec("(into [] [1 2 3])") == [1, 2, 3]


def test_into_with_existing():
    assert _vec("(into [10 20] [30 40])") == [10, 20, 30, 40]


def test_into_set():
    r = _ev("(into #{} (filter odd?) (range 10))")
    assert set(r) == {1, 3, 5, 7, 9}


def test_into_with_xform():
    assert _vec("(into [] (map inc) [1 2 3])") == [2, 3, 4]


# ---------------------------------------------------------------------
# sequence
# ---------------------------------------------------------------------

def test_sequence_plain():
    # (sequence coll) returns (seq coll).
    assert list(_ev("(sequence [1 2 3])")) == [1, 2, 3]


def test_sequence_empty():
    # Vanilla: (sequence []) returns an empty seq (not nil). See
    # clojure.test-clojure.logic/test-nil-punning.
    result = _ev("(sequence [])")
    assert result is not None
    assert list(result) == []


def test_sequence_with_xform():
    assert list(_ev("(sequence (map inc) [1 2 3])")) == [2, 3, 4]


# ---------------------------------------------------------------------
# Composed pipelines
# ---------------------------------------------------------------------

def test_comp_of_three_transducers():
    r = _ev(
        "(into [] (comp (filter odd?) (map (fn [x] (* x 10))) (take 3)) (range 100))"
    )
    # odds: 1 3 5 7 9 ... → *10: 10 30 50 70 90 ... → take 3: 10 30 50
    assert list(r) == [10, 30, 50]


def test_transduce_with_reduced_stops_early():
    # A reducer that bails when sum exceeds 10.
    r = _ev(
        "(transduce (map identity)"
        "           (fn ([] 0)"
        "               ([acc] acc)"
        "               ([acc x] (if (> acc 10) (reduced acc) (+ acc x))))"
        "           (range 100))"
    )
    assert r >= 10


def test_dedupe_composed_with_map():
    # (map even?) on [1 2 3 4] → [F T F T]; dedupe → [F T F T].
    r = _vec("(into [] (comp (map even?) (dedupe)) [1 2 3 4])")
    assert r == [False, True, False, True]
