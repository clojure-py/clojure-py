"""Tests for the Ref reference type — pre-STM slice.

At this stage only the fast path works: construct a Ref, deref it, inspect
history/metadata. The transaction primitives (`ref-set`, `alter`, `commute`,
`ensure`, `sync`/`dosync`, `io!`) land in later commits.
"""

import pytest
from clojure._core import eval_string, Ref, IllegalStateException


def _ev(src):
    return eval_string(src)


def test_ref_ctor_python():
    from clojure._core import ref
    r = ref(42)
    assert isinstance(r, Ref)


def test_ref_ctor_clojure():
    r = _ev("(ref 42)")
    assert isinstance(r, Ref)


def test_ref_deref_fast_path():
    assert _ev("(deref (ref 42))") == 42


def test_ref_reader_deref():
    assert _ev("(let* [r (ref :x)] @r)") == _ev(":k") or _ev("(let* [r (ref :x)] @r)") == _ev(":x")


def test_ref_repr():
    r = _ev("(ref 42)")
    assert repr(r) == "#<Ref 42>"


def test_ref_history_count_starts_at_one():
    r = _ev("(ref 42)")
    assert r.history_count() == 1


def test_ref_default_history_bounds():
    r = _ev("(ref 42)")
    assert r.min_history == 0
    assert r.max_history == 10


def test_ref_meta_defaults_to_none():
    r = _ev("(ref 42)")
    assert r.meta is None
