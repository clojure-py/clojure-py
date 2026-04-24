"""Tests for CollReduce / IKVReduce / IChunk::chunk_reduce across all
collection types. Uses hypothesis to fuzz against functools.reduce."""

import functools
import pytest
from hypothesis import given, strategies as st, settings
from clojure._core import eval_string, find_ns, symbol


def _ev(src):
    return eval_string(src)


_rt_ns = find_ns(symbol("clojure.lang.RT"))
coll_reduce = getattr(_rt_ns, "coll-reduce")
kv_reduce = getattr(_rt_ns, "kv-reduce")


def _add(a, b):
    return a + b


def _conj_pair(acc, me):
    # For map reductions, items are MapEntry instances — pull .key/.val.
    return acc + [(me.key, me.val)]


# --- Basic CollReduce smoke on each coll type ---

def test_reduce_vector_with_init():
    v = _ev("[1 2 3 4 5]")
    assert coll_reduce(v, _add, 0) == 15


def test_reduce_vector_no_init():
    v = _ev("[1 2 3 4]")
    assert coll_reduce(v, _add) == 10


def test_reduce_list_with_init():
    l = _ev("'(10 20 30)")
    assert coll_reduce(l, _add, 0) == 60


def test_reduce_empty_list_calls_f_no_args():
    e = _ev("'()")
    # (reduce f ()) → (f)
    assert coll_reduce(e, lambda: 42) == 42


def test_reduce_set():
    s = _ev("#{1 2 3}")
    # Set iteration order is unspecified; sum is deterministic.
    assert coll_reduce(s, _add, 0) == 6


def test_reduce_hash_map_yields_map_entries():
    m = _ev("(hash-map :a 1 :b 2)")
    pairs = coll_reduce(m, _conj_pair, [])
    # Order unspecified — convert to set for stable assertion.
    assert sorted(pairs, key=lambda kv: str(kv[0])) == sorted(
        [(eval_string(":a"), 1), (eval_string(":b"), 2)],
        key=lambda kv: str(kv[0]),
    )


def test_reduce_array_map():
    # hash-map with <= 8 entries uses PersistentArrayMap (usually).
    m = _ev("{:a 1 :b 2 :c 3}")
    pairs = coll_reduce(m, _conj_pair, [])
    assert sorted(p[0].name for p in pairs) == ["a", "b", "c"]
    assert sorted(p[1] for p in pairs) == [1, 2, 3]


def test_reduce_cons_seq():
    s = _ev("(cons 1 (cons 2 (cons 3 nil)))")
    assert coll_reduce(s, _add, 0) == 6


def test_reduce_lazy_seq():
    ls = _ev("(lazy-seq (cons 7 (cons 8 (cons 9 nil))))")
    assert coll_reduce(ls, _add, 0) == 24


def test_reduce_chunked_cons():
    cc = _ev(
        "(let* [b (chunk-buffer 4)]"
        "  (chunk-append b 1) (chunk-append b 2) (chunk-append b 3)"
        "  (chunk-cons (chunk b) nil))"
    )
    assert coll_reduce(cc, _add, 0) == 6


def test_reduce_empty_vector_no_init_calls_f():
    v = _ev("[]")
    assert coll_reduce(v, lambda: 99) == 99


# --- IKVReduce on maps ---

def test_kv_reduce_hash_map():
    m = _ev("(hash-map :a 1 :b 2 :c 3)")
    def fold(acc, k, v):
        acc[k.name] = v
        return acc
    out = kv_reduce(m, fold, {})
    assert out == {"a": 1, "b": 2, "c": 3}


def test_kv_reduce_array_map():
    m = _ev("{:a 1 :b 2}")
    def fold(acc, k, v):
        return acc + [(k.name, v)]
    out = kv_reduce(m, fold, [])
    assert sorted(out) == [("a", 1), ("b", 2)]


# --- Hypothesis: CollReduce matches functools.reduce for common coll types ---

small_ints = st.integers(-1000, 1000)


@settings(max_examples=100, deadline=None)
@given(items=st.lists(small_ints, min_size=0, max_size=50))
def test_vector_reduce_matches_functools(items):
    src = "[" + " ".join(str(x) for x in items) + "]"
    v = _ev(src)
    got = coll_reduce(v, _add, 0)
    want = functools.reduce(_add, items, 0)
    assert got == want


@settings(max_examples=100, deadline=None)
@given(items=st.lists(small_ints, min_size=0, max_size=50))
def test_list_reduce_matches_functools(items):
    # Build via (list 1 2 3 ...) to get a PersistentList.
    src = "(list " + " ".join(str(x) for x in items) + ")"
    l = _ev(src)
    got = coll_reduce(l, _add, 0)
    want = functools.reduce(_add, items, 0)
    assert got == want


@settings(max_examples=50, deadline=None)
@given(items=st.lists(small_ints, min_size=0, max_size=30),
       cap=st.integers(min_value=1, max_value=8))
def test_chunked_cons_reduce_matches_functools(items, cap):
    if not items:
        return
    parts = [items[i:i+cap] for i in range(0, len(items), cap)]
    expr = "nil"
    for part in reversed(parts):
        chunk_expr = f"(let* [b (chunk-buffer {cap})] " + \
            " ".join(f"(chunk-append b {x})" for x in part) + " (chunk b))"
        expr = f"(chunk-cons {chunk_expr} {expr})"
    cc = _ev(expr)
    got = coll_reduce(cc, _add, 0)
    want = sum(items)
    assert got == want


# --- reduce1 exercised via variadic arithmetic ---

def test_variadic_sum_uses_reduce1():
    # With 4 args the variadic branch kicks in. Result must be correct.
    assert _ev("(+ 1 2 3 4 5 6 7 8 9 10)") == 55
    assert _ev("(* 2 3 4 5)") == 120
