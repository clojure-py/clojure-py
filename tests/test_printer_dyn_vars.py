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


# ---------- *print-level* ----------

def test_print_level_zero_replaces_top_level_collection():
    """*print-level* 0: any collection at top level prints as '#'."""
    src = '(binding [*print-level* 0] (pr-str [1 2 3]))'
    assert e(src) == "#"


def test_print_level_one_truncates_inner():
    """*print-level* 1: outer collection prints; inner collections become '#'."""
    src = '(binding [*print-level* 1] (pr-str [1 [2 3]]))'
    assert e(src) == "[1 #]"


def test_print_level_two_allows_two_layers():
    src = '(binding [*print-level* 2] (pr-str [1 [2 [3 4]]]))'
    assert e(src) == "[1 [2 #]]"


def test_print_level_primitives_at_any_depth():
    """Primitives never get truncated by *print-level*."""
    src = '(binding [*print-level* 0] (pr-str 42))'
    assert e(src) == "42"
    src2 = '(binding [*print-level* 0] (pr-str :keyword))'
    assert e(src2) == ":keyword"


def test_print_level_with_print_length():
    """Combined: length + level interact correctly."""
    src = '(binding [*print-level* 1 *print-length* 2] (pr-str [1 [2 3] [4 5]]))'
    assert e(src) == "[1 # ...]"


# ---------- *print-meta* ----------

def test_print_meta_on_symbol():
    """A meta-bearing symbol prints as ^{:k v} sym when *print-meta* is true."""
    src = '''
    (binding [*print-meta* true]
      (pr-str (with-meta (quote sym) {:awesome true})))
    '''
    assert e(src) == "^{:awesome true} sym"


def test_print_meta_on_list():
    src = '''
    (binding [*print-meta* true]
      (pr-str (with-meta (list 1 2 3) {:a 1})))
    '''
    assert e(src) == "^{:a 1} (1 2 3)"


def test_print_meta_off_by_default():
    """Without *print-meta*, meta is invisible in the printed form."""
    src = '(pr-str (with-meta (quote sym) {:awesome true}))'
    assert e(src) == "sym"


def test_print_meta_no_meta_no_prefix():
    """Even with *print-meta* true, values without meta print normally."""
    src = '(binding [*print-meta* true] (pr-str (quote sym)))'
    assert e(src) == "sym"


# ---------- *print-namespace-maps* ----------

def test_print_namespace_maps_on_uniform_keys():
    """When all keys share a namespace and the var is true → #:ns{...} form."""
    src = '(binding [*print-namespace-maps* true] (pr-str {:a/x 1 :a/y 2}))'
    result = e(src)
    assert result.startswith("#:a{")
    assert result.endswith("}")
    assert ":x 1" in result
    assert ":y 2" in result


def test_print_namespace_maps_off_default():
    """Default false: standard form."""
    src = '(pr-str {:a/x 1 :a/y 2})'
    result = e(src)
    assert result.startswith("{")
    assert ":a/x 1" in result
    assert ":a/y 2" in result


def test_print_namespace_maps_mixed_namespaces():
    """If keys span multiple namespaces, fall back to standard form even when var is true."""
    src = '(binding [*print-namespace-maps* true] (pr-str {:a/x 1 :b/y 2}))'
    result = e(src)
    assert result.startswith("{")
    assert ":a/x 1" in result
    assert ":b/y 2" in result


def test_print_namespace_maps_non_keyword_keys():
    """Non-keyword keys also disqualify the short form."""
    src = '(binding [*print-namespace-maps* true] (pr-str {"a" 1 "b" 2}))'
    result = e(src)
    assert result.startswith("{")


def test_print_namespace_maps_unqualified_keys():
    """Keys without namespace also disqualify the short form."""
    src = '(binding [*print-namespace-maps* true] (pr-str {:x 1 :y 2}))'
    result = e(src)
    assert result.startswith("{")
