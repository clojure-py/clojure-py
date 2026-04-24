"""Tests for the chunked-seq infrastructure: ArrayChunk / ChunkBuffer /
ChunkedCons, plus the IChunkedSeq-aware `concat` path."""

import pytest
from hypothesis import given, strategies as st, settings
from clojure._core import (
    eval_string,
    ArrayChunk,
    ChunkBuffer,
    ChunkedCons,
    IllegalStateException,
)


def _ev(src):
    return eval_string(src)


# --- Basic machinery ---

def test_chunk_buffer_seal_empty():
    cb = ChunkBuffer(4)
    assert len(cb) == 0
    ac = cb.chunk()
    assert isinstance(ac, ArrayChunk)
    assert len(ac) == 0


def test_chunk_buffer_add_and_seal():
    cb = ChunkBuffer(4)
    cb.add(10)
    cb.add(20)
    cb.add(30)
    ac = cb.chunk()
    assert len(ac) == 3


def test_chunk_buffer_overflow_raises():
    cb = ChunkBuffer(2)
    cb.add(1)
    cb.add(2)
    with pytest.raises(IllegalStateException):
        cb.add(3)


def test_chunked_cons_via_rt():
    cc = _ev(
        "(let* [b (chunk-buffer 4)]"
        "  (chunk-append b 1)"
        "  (chunk-append b 2)"
        "  (chunk-append b 3)"
        "  (chunk-cons (chunk b) nil))"
    )
    assert isinstance(cc, ChunkedCons)
    assert list(cc) == [1, 2, 3]


def test_chunked_seq_pred():
    assert _ev(
        "(chunked-seq?"
        "  (let* [b (chunk-buffer 2)]"
        "    (chunk-append b :a)"
        "    (chunk-append b :b)"
        "    (chunk-cons (chunk b) nil)))"
    ) is True
    assert _ev("(chunked-seq? (seq [1 2 3]))") is False


def test_chunk_cons_with_empty_chunk_returns_rest():
    r = _ev(
        "(let* [b (chunk-buffer 4)]"
        "  (chunk-cons (chunk b) (cons 99 nil)))"
    )
    assert list(r) == [99]


# --- Property-based: chunked walks match the flat source ---

small_ints = st.integers(-100, 100)


def _build_chunked_via_eval(items, cap):
    """Partition `items` into size-`cap` chunks and chain them via
    nested chunk-cons expressions. One big `eval_string` call; no Python
    recursion at runtime."""
    if not items:
        return None
    parts = [items[i:i + cap] for i in range(0, len(items), cap)]
    expr = "nil"
    for part in reversed(parts):
        chunk_expr = f"(let* [b (chunk-buffer {cap})] " + \
            " ".join(f"(chunk-append b {x})" for x in part) + \
            " (chunk b))"
        expr = f"(chunk-cons {chunk_expr} {expr})"
    return _ev(expr)


@settings(max_examples=50, deadline=None)
@given(items=st.lists(small_ints, min_size=0, max_size=40),
       cap=st.integers(min_value=1, max_value=8))
def test_chunked_walk_matches_items(items, cap):
    cc = _build_chunked_via_eval(items, cap)
    if not items:
        assert cc is None
        return
    assert list(cc) == items


# --- concat exercises the chunked-seq path on IChunkedSeq inputs ---

def test_concat_with_chunked_head_walks_correctly():
    chunked = _ev(
        "(let* [b (chunk-buffer 4)]"
        "  (chunk-append b 1)"
        "  (chunk-append b 2)"
        "  (chunk-append b 3)"
        "  (chunk-cons (chunk b) nil))"
    )
    # Assign it back via a fresh let to concat with a vector.
    _ev("(def ^:private _tmp-chunked nil)")  # no-op just to warm
    # Easier: build a single concat expression that starts with chunked.
    r = _ev(
        "(concat (let* [b (chunk-buffer 4)]"
        "          (chunk-append b 1)"
        "          (chunk-append b 2)"
        "          (chunk-append b 3)"
        "          (chunk-cons (chunk b) nil))"
        "        [4 5])"
    )
    assert list(r) == [1, 2, 3, 4, 5]
