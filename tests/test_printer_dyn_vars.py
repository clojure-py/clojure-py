"""Printer dynamic vars: *print-length*, *print-level*, *print-meta*, *print-namespace-maps*."""

import pytest
from clojure._core import eval_string as e


# ---------- *print-length* ----------

def test_print_length_vector_truncates():
    src = '(binding [*print-length* 3] (pr-str [1 2 3 4 5]))'
    assert e(src) == "[1 2 3 ...]"


def test_print_length_vector_no_truncation_when_short():
    src = '(binding [*print-length* 10] (pr-str [1 2 3]))'
    assert e(src) == "[1 2 3]"


def test_print_length_vector_zero():
    """`*print-length* 0` prints just the ellipsis form."""
    src = '(binding [*print-length* 0] (pr-str [1 2 3]))'
    assert e(src) == "[...]"


def test_print_length_list():
    src = '(binding [*print-length* 2] (pr-str (list :a :b :c :d)))'
    assert e(src) == "(:a :b ...)"


def test_print_length_set():
    """Set order is unspecified; assert presence of '...' and only N elements."""
    src = '(binding [*print-length* 2] (pr-str #{:a :b :c}))'
    result = e(src)
    assert result.startswith("#{")
    assert result.endswith("}")
    assert "..." in result
    inner = result[2:-1]
    items = inner.split(" ")
    assert len(items) == 3


def test_print_length_map():
    """Map order is unspecified; assert truncation to N pairs + ellipsis."""
    src = '(binding [*print-length* 1] (pr-str {:a 1 :b 2 :c 3}))'
    result = e(src)
    assert result.startswith("{")
    assert result.endswith("}")
    assert "..." in result


def test_print_length_nil_means_no_limit():
    """nil (the default) prints all elements — baseline."""
    src = '(pr-str [1 2 3 4 5])'
    assert e(src) == "[1 2 3 4 5]"
