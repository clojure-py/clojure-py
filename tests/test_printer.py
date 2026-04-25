"""Phase R5 — printer."""

import pytest
from clojure._core import (
    pr_str, read_string, keyword, symbol,
    vector, hash_map, hash_set, list_, cons,
)


# --- Atoms ---

def test_pr_nil():
    assert pr_str(None) == "nil"


def test_pr_bool():
    assert pr_str(True) == "true"
    assert pr_str(False) == "false"


def test_pr_int():
    assert pr_str(42) == "42"
    assert pr_str(-17) == "-17"
    assert pr_str(0) == "0"


def test_pr_float():
    assert pr_str(3.14) == "3.14"


def test_pr_string_simple():
    assert pr_str("hello") == '"hello"'


def test_pr_string_with_escapes():
    assert pr_str("a\nb") == '"a\\nb"'
    assert pr_str('a"b') == '"a\\"b"'
    assert pr_str("a\\b") == '"a\\\\b"'


def test_pr_keyword():
    assert pr_str(keyword("foo")) == ":foo"
    assert pr_str(keyword("ns", "foo")) == ":ns/foo"


def test_pr_symbol():
    assert pr_str(symbol("foo")) == "foo"
    assert pr_str(symbol("my.ns", "foo")) == "my.ns/foo"


# --- Collections ---

def test_pr_empty_list():
    assert pr_str(list_()) == "()"


def test_pr_list_of_ints():
    assert pr_str(list_(1, 2, 3)) == "(1 2 3)"


def test_pr_empty_vector():
    assert pr_str(vector()) == "[]"


def test_pr_vector_of_ints():
    assert pr_str(vector(1, 2, 3)) == "[1 2 3]"


def test_pr_nested():
    assert pr_str(vector(1, vector(2, 3), 4)) == "[1 [2 3] 4]"


def test_pr_empty_map():
    assert pr_str(hash_map()) == "{}"


def test_pr_map_with_keys():
    m = hash_map().assoc(keyword("a"), 1)
    assert pr_str(m) == "{:a 1}"


def test_pr_empty_set():
    assert pr_str(hash_set()) == "#{}"


def test_pr_set():
    s = hash_set(keyword("a"))
    assert pr_str(s) == "#{:a}"


# --- Round-trip via read_string (sanity) ---

def test_roundtrip_int():
    assert read_string(pr_str(42)) == 42


def test_roundtrip_vector():
    v = vector(1, 2, 3)
    assert list(read_string(pr_str(v))) == [1, 2, 3]


def test_roundtrip_nested():
    original_source = '[:a [1 2] {:k :v}]'
    v = read_string(original_source)
    printed = pr_str(v)
    reparsed = read_string(printed)
    # Same structure.
    assert pr_str(reparsed) == printed


# --- Fractions / Ratios ---

from fractions import Fraction
from clojure._core import eval as _eval


def test_pr_str_fraction_uses_slash_form():
    assert pr_str(Fraction(1, 2)) == "1/2"
    assert pr_str(Fraction(3, 4)) == "3/4"
    assert pr_str(Fraction(-1, 2)) == "-1/2"


def test_pr_str_fraction_round_trip():
    src = "1/2"
    val = _eval(read_string(src))  # Fraction(1, 2)
    assert pr_str(val) == src
    assert _eval(read_string(pr_str(val))) == val


def test_pr_str_division_result():
    half = _eval(read_string("(/ 1 2)"))
    assert pr_str(half) == "1/2"
    two = _eval(read_string("(/ 4 2)"))
    assert pr_str(two) == "2"
