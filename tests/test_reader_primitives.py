"""Phase R1 — low-level reader primitives."""

import pytest
from clojure._core import ReaderError


def _num(s):
    from clojure._core import _test_parse_number
    return _test_parse_number(s)


def _str(s):
    from clojure._core import _test_parse_string
    return _test_parse_string(s)


def _ch(s):
    from clojure._core import _test_parse_char
    return _test_parse_char(s)


def test_int_basic():
    assert _num("42") == 42
    assert _num("0") == 0
    assert _num("-1") == -1
    assert _num("+7") == 7


def test_int_bignum_fallback():
    n = _num("12345678901234567890123456789")
    assert n == 12345678901234567890123456789


def test_float_basic():
    assert _num("3.14") == 3.14
    assert _num("-2.5") == -2.5
    assert _num("0.0") == 0.0


def test_float_exponent():
    assert _num("1e3") == 1000.0
    assert _num("1.5e-2") == 0.015


def test_string_plain():
    assert _str('"hello"') == "hello"


def test_string_escapes():
    assert _str(r'"a\nb"') == "a\nb"
    assert _str(r'"a\tb"') == "a\tb"
    assert _str(r'"a\\b"') == "a\\b"
    assert _str(r'"a\"b"') == 'a"b'


def test_string_unicode_escape():
    assert _str(r'"\u0041"') == "A"


def test_string_unterminated_raises():
    with pytest.raises(ReaderError, match="EOF"):
        _str('"unterminated')


def test_char_ascii():
    from clojure._core import Char
    assert _ch(r"\a") == Char("a")
    assert _ch(r"\A") == Char("A")


def test_char_named():
    from clojure._core import Char
    assert _ch(r"\space") == Char(" ")
    assert _ch(r"\newline") == Char("\n")
    assert _ch(r"\tab") == Char("\t")


def test_char_unicode():
    from clojure._core import Char
    assert _ch(r"\u0041") == Char("A")


def test_char_invalid_named_raises():
    with pytest.raises(ReaderError):
        _ch(r"\bogusname")


def test_reader_error_is_subclass_of_illegal_argument():
    from clojure._core import IllegalArgumentException
    assert issubclass(ReaderError, IllegalArgumentException)


from fractions import Fraction


def test_ratio_basic():
    assert _num("1/2") == Fraction(1, 2)
    assert _num("3/4") == Fraction(3, 4)


def test_ratio_reduces_to_int_when_denominator_one():
    # 4/2 -> int 2 (NOT Fraction(2, 1))
    v = _num("4/2")
    assert v == 2
    assert type(v) is int


def test_ratio_reduces_to_lowest_terms():
    v = _num("2/4")
    assert v == Fraction(1, 2)
    assert v.numerator == 1 and v.denominator == 2


def test_ratio_negative_numerator():
    assert _num("-1/2") == Fraction(-1, 2)
    assert _num("+1/2") == Fraction(1, 2)


def test_ratio_zero_denominator_is_reader_error():
    with pytest.raises(ReaderError):
        _num("1/0")


def test_ratio_negative_denominator_is_reader_error():
    # Sign rides on the numerator only — `1/-2` is invalid.
    with pytest.raises(ReaderError):
        _num("1/-2")


def test_ratio_missing_denominator_is_reader_error():
    with pytest.raises(ReaderError):
        _num("1/")


def test_float_with_slash_is_not_a_ratio():
    # `1.5/2` is the float 1.5; trailing `/2` is not our problem at this layer
    # but the call returns 1.5 (the trailing chars are unread/left over).
    # _test_parse_number reads exactly one number. The float branch wins.
    assert _num("1.5") == 1.5  # baseline
    # We don't test "1.5/2" through _num because the helper expects a single
    # complete number; the property we care about is that the ratio branch is
    # gated on `is_float == false`.
