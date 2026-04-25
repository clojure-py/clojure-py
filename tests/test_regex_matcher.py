"""Regex matcher: stateful re-find and Indexed access."""

import builtins as bi
import pytest
from clojure._core import eval_string as e


def test_re_find_advances_matcher():
    """Calling (re-find m) repeatedly walks all matches, then returns nil."""
    src = """
    (let [m (re-matcher #"\\d+" "1 22 333")]
      [(re-find m) (re-find m) (re-find m) (re-find m)])
    """
    result = e(src)
    assert result[0] == "1"
    assert result[1] == "22"
    assert result[2] == "333"
    assert result[3] is None


def test_re_find_returns_vector_for_groups():
    """With groups, re-find on a matcher returns [whole g1 g2 ...] vector."""
    src = """
    (let [m (re-matcher #"(\\d{2})/(\\d{2})/(\\d{4})" "12/02/1975")]
      (re-find m))
    """
    result = e(src)
    assert list(result) == ["12/02/1975", "12", "02", "1975"]


def test_re_find_two_arity_unchanged():
    """Existing 2-arity (re-find pattern str) still works."""
    src = '(re-find #"\\d+" "abc 42 def")'
    assert e(src) == "42"


def test_nth_after_re_find_returns_groups():
    src = """
    (let [m (re-matcher #"(\\d{2})/(\\d{2})/(\\d{4})" "12/02/1975")]
      (re-find m)
      [(nth m 0) (nth m 1) (nth m 2) (nth m 3)])
    """
    result = e(src)
    assert list(result) == ["12/02/1975", "12", "02", "1975"]


def test_nth_out_of_bounds_raises_index_error():
    src_neg = """
    (let [m (re-matcher #"(\\d{2})/(\\d{2})/(\\d{4})" "12/02/1975")]
      (re-find m)
      (nth m -1))
    """
    with pytest.raises(bi.IndexError):
        e(src_neg)
    src_high = """
    (let [m (re-matcher #"(\\d{2})/(\\d{2})/(\\d{4})" "12/02/1975")]
      (re-find m)
      (nth m 4))
    """
    with pytest.raises(bi.IndexError):
        e(src_high)


def test_nth_with_default_returns_default_on_oob():
    src = """
    (let [m (re-matcher #"(\\d{2})/(\\d{2})/(\\d{4})" "12/02/1975")]
      (re-find m)
      [(nth m -1 :foo) (nth m 4 :foo)])
    """
    result = e(src)
    assert list(result) == [e(":foo"), e(":foo")]


def test_nth_before_re_find_raises_state_error():
    """Calling nth on a fresh matcher (no re-find yet) raises."""
    src = """(let [m (re-matcher #"\\d+" "abc")] (nth m 0))"""
    with pytest.raises(Exception) as ei:
        e(src)
    assert "match" in str(ei.value).lower()


def test_re_groups_reads_last_match():
    src = """
    (let [m (re-matcher #"(\\d+)/(\\d+)" "1/2 3/4")]
      (re-find m)
      (re-groups m))
    """
    result = e(src)
    assert list(result) == ["1/2", "1", "2"]
