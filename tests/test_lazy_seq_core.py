"""Tests for the Clojure-level `lazy-seq` macro + lazy `concat`."""

from clojure._core import eval_string, LazySeq


def _ev(src):
    return eval_string(src)


def test_lazy_seq_of_nil_is_lazyseq():
    assert isinstance(_ev("(lazy-seq nil)"), LazySeq)


def test_lazy_seq_seq_is_nil_for_empty_body():
    assert _ev("(seq (lazy-seq nil))") is None


def test_lazy_seq_first_realizes_head():
    assert _ev("(first (lazy-seq (cons 42 nil)))") == 42


def test_concat_empty_is_lazy():
    r = _ev("(concat)")
    assert isinstance(r, LazySeq)


def test_concat_two_colls():
    assert list(_ev("(concat [1 2] [3 4])")) == [1, 2, 3, 4]


def test_concat_three_colls():
    assert list(_ev("(concat [1] [2] [3 4])")) == [1, 2, 3, 4]


def test_concat_with_nil_tail():
    assert list(_ev("(concat nil [1] nil [2])")) == [1, 2]


def test_concat_preserves_chunk_boundary():
    # First coll of 1 item, second of 2 — realizing first of the result
    # should not over-realize into the second coll.
    r = _ev("(concat [1] [2 3])")
    # Iterating gives us the full sequence in order.
    assert list(r) == [1, 2, 3]
