"""Char value type — tests for the dedicated Char pyclass.

Char wraps a single Unicode codepoint and is the type produced by reader char
literals (`\\a`, `\\space`, `\\u0041`, ...). It is distinct from `str` so that
JVM-equivalent type predicates work: `(string? \\a)` is false, `(char? "a")`
is false, `(= \\a "a")` is false.
"""

import pytest

from clojure._core import Char, eval_string as _ev, read_string


# ---------------------------------------------------------------------------
# Construction
# ---------------------------------------------------------------------------

def test_construct_from_str():
    assert Char("a").value == "a"


def test_construct_from_int():
    assert Char(97).value == "a"
    assert Char(0x41).value == "A"


def test_construct_from_char():
    assert Char(Char("a")).value == "a"


def test_construct_rejects_multichar_str():
    with pytest.raises((ValueError, TypeError)):
        Char("ab")


def test_construct_rejects_empty_str():
    with pytest.raises((ValueError, TypeError)):
        Char("")


def test_construct_rejects_negative_int():
    with pytest.raises((ValueError, OverflowError)):
        Char(-1)


def test_construct_rejects_out_of_range_int():
    with pytest.raises((ValueError, OverflowError)):
        Char(0x110000)  # past max codepoint


# ---------------------------------------------------------------------------
# Identity / type
# ---------------------------------------------------------------------------

def test_char_is_not_str():
    c = Char("a")
    assert not isinstance(c, str)


def test_str_is_not_char():
    assert not isinstance("a", Char)


# ---------------------------------------------------------------------------
# Equality
# ---------------------------------------------------------------------------

def test_eq_same_char():
    assert Char("a") == Char("a")


def test_eq_different_chars():
    assert Char("a") != Char("b")


def test_eq_str_is_false():
    assert Char("a") != "a"
    assert "a" != Char("a")


def test_eq_int_is_false():
    assert Char("a") != 97
    assert 97 != Char("a")


def test_eq_none():
    assert Char("a") != None


# ---------------------------------------------------------------------------
# Hash
# ---------------------------------------------------------------------------

def test_hash_codepoint():
    # Vanilla Character.hashCode() == int value.
    assert hash(Char("a")) == 97
    assert hash(Char("A")) == 65
    assert hash(Char("é")) == 0xe9


def test_hash_same_chars_match():
    assert hash(Char("x")) == hash(Char("x"))


def test_hashable_in_set():
    s = {Char("a"), Char("b"), Char("a")}
    assert len(s) == 2


# ---------------------------------------------------------------------------
# Comparison
# ---------------------------------------------------------------------------

def test_lt_by_codepoint():
    assert Char("a") < Char("b")
    assert not (Char("b") < Char("a"))


def test_gt_by_codepoint():
    assert Char("z") > Char("a")


def test_le_ge():
    assert Char("a") <= Char("a")
    assert Char("a") >= Char("a")
    assert Char("a") <= Char("b")


def test_compare_with_str_raises():
    with pytest.raises(TypeError):
        Char("a") < "b"


# ---------------------------------------------------------------------------
# Conversions
# ---------------------------------------------------------------------------

def test_str_returns_raw_char():
    assert str(Char("a")) == "a"
    assert str(Char(" ")) == " "


def test_int_returns_codepoint():
    assert int(Char("a")) == 97
    assert int(Char("A")) == 65


def test_bool_always_true():
    assert bool(Char("a"))
    assert bool(Char("\0"))  # even null is truthy


# ---------------------------------------------------------------------------
# repr — reader-form
# ---------------------------------------------------------------------------

def test_repr_simple():
    assert repr(Char("a")) == "\\a"


def test_repr_named_chars():
    assert repr(Char(" ")) == "\\space"
    assert repr(Char("\n")) == "\\newline"
    assert repr(Char("\t")) == "\\tab"
    assert repr(Char("\r")) == "\\return"
    assert repr(Char("\x08")) == "\\backspace"
    assert repr(Char("\x0c")) == "\\formfeed"
    assert repr(Char("\0")) == "\\null"


# ---------------------------------------------------------------------------
# Reader integration
# ---------------------------------------------------------------------------

def test_reader_returns_char():
    v = read_string("\\a")
    assert isinstance(v, Char)
    assert v == Char("a")


def test_reader_named():
    assert read_string("\\space") == Char(" ")
    assert read_string("\\newline") == Char("\n")


def test_reader_unicode():
    assert read_string("\\u0041") == Char("A")


# ---------------------------------------------------------------------------
# Clojure-side integration
# ---------------------------------------------------------------------------

def test_clj_char_predicate():
    assert _ev(r'(char? \a)') is True
    assert _ev(r'(char? "a")') is False
    assert _ev(r'(char? 97)') is False


def test_clj_string_predicate_rejects_char():
    assert _ev(r'(string? \a)') is False
    assert _ev(r'(string? "a")') is True


def test_clj_equality():
    assert _ev(r'(= \a \a)') is True
    assert _ev(r'(= \a \b)') is False
    assert _ev(r'(= \a "a")') is False
    assert _ev(r'(= "a" \a)') is False
    assert _ev(r'(= \a 97)') is False


def test_clj_str_concat():
    assert _ev(r'(str \h \i)') == "hi"
    assert _ev(r'(str "h" \i)') == "hi"


def test_clj_int_of_char():
    assert _ev(r'(int \a)') == 97


def test_clj_compare():
    assert _ev(r'(compare \a \b)') < 0
    assert _ev(r'(compare \b \a)') > 0
    assert _ev(r'(compare \a \a)') == 0


def test_clj_pr_str_char():
    assert _ev(r'(pr-str \a)') == "\\a"
    assert _ev(r'(pr-str \space)') == "\\space"


def test_clj_print_str_char():
    # print-str (non-readable form) outputs the raw character.
    assert _ev(r'(print-str \a)') == "a"


def test_clj_char_in_set():
    s = _ev(r'#{\a \b \c}')
    assert _ev(r'(contains? #{\a \b \c} \b)') is True
    assert _ev(r'(contains? #{\a \b \c} \z)') is False
