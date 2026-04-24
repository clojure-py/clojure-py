"""Tests for public `reduce`, `reduce-kv`, `reduced` family, and Reduced
short-circuit across every CollReduce impl."""

import functools
import pytest
from hypothesis import given, strategies as st, settings
from clojure._core import eval_string, find_ns, symbol, Reduced


def _ev(src):
    return eval_string(src)


# --- reduced / reduced? / unreduced / ensure-reduced ---

def test_reduced_round_trip():
    r = _ev("(reduced 42)")
    assert isinstance(r, Reduced)
    assert r.val == 42


def test_reduced_pred():
    assert _ev("(reduced? (reduced 5))") is True
    assert _ev("(reduced? 5)") is False
    assert _ev("(reduced? nil)") is False


def test_unreduced():
    assert _ev("(unreduced (reduced 42))") == 42
    assert _ev("(unreduced 7)") == 7


def test_ensure_reduced():
    a = _ev("(ensure-reduced 5)")
    assert isinstance(a, Reduced)
    assert a.val == 5
    # Already-reduced should be returned as-is.
    b = _ev("(let* [r (reduced 9)] (ensure-reduced r))")
    assert isinstance(b, Reduced)
    assert b.val == 9


# --- public reduce ---

def test_reduce_basic():
    assert _ev("(reduce + [1 2 3 4 5])") == 15
    assert _ev("(reduce + 0 [1 2 3])") == 6


def test_reduce_single_element_no_init():
    assert _ev("(reduce + [42])") == 42


def test_reduce_empty_no_init_calls_f_zero_args():
    assert _ev("(reduce + [])") == 0


def test_reduce_empty_with_init():
    assert _ev("(reduce + 100 [])") == 100


def test_reduce_list():
    assert _ev("(reduce + 0 (list 10 20 30))") == 60


def test_reduce_set():
    # Order-agnostic; sum is deterministic.
    assert _ev("(reduce + 0 #{1 2 3 4})") == 10


def test_reduce_map_yields_entries():
    pairs = _ev("(reduce (fn [a e] (conj a [(key e) (val e)])) [] {:a 1 :b 2})")
    out = {p[0].name: p[1] for p in pairs}
    assert out == {"a": 1, "b": 2}


def test_reduce_short_circuits_at_first_neg():
    # Bail before 4 is added.
    assert _ev(
        "(reduce (fn [a x] (if (neg? x) (reduced a) (+ a x))) 0 [1 2 -1 4 5])"
    ) == 3


def test_reduce_short_circuits_on_chunked_cons():
    r = _ev(
        "(let* [b (chunk-buffer 4)]"
        "  (chunk-append b 1)"
        "  (chunk-append b 2)"
        "  (chunk-append b -1)"
        "  (chunk-append b 4)"
        "  (reduce (fn [a x] (if (neg? x) (reduced a) (+ a x)))"
        "          0"
        "          (chunk-cons (chunk b) nil)))"
    )
    assert r == 3


# --- reduce-kv ---

def test_reduce_kv_hash_map():
    out = _ev("(reduce-kv (fn [acc k v] (conj acc [k v])) [] {:a 1 :b 2})")
    d = {e[0].name: e[1] for e in out}
    assert d == {"a": 1, "b": 2}


def test_reduce_kv_nil_returns_init():
    assert _ev("(reduce-kv (fn [a k v] a) :seed nil)") == _ev(":seed")


def test_reduce_kv_short_circuits():
    r = _ev(
        "(reduce-kv (fn [acc k v]"
        "             (if (= k :stop) (reduced acc) (conj acc v)))"
        "           []"
        "           {:a 1 :stop :x :c 3})"
    )
    # Stops at :stop; result contains only values collected before.
    assert len(list(r)) < 3


# --- Hypothesis: public reduce matches functools.reduce ---

small_ints = st.integers(-100, 100)


@settings(max_examples=100, deadline=None)
@given(items=st.lists(small_ints, min_size=0, max_size=30))
def test_reduce_matches_functools_on_vector(items):
    src = "[" + " ".join(str(x) for x in items) + "]"
    got = _ev(f"(reduce + 0 {src})")
    assert got == sum(items)
