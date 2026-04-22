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
    assert _ch(r"\a") == "a"
    assert _ch(r"\A") == "A"


def test_char_named():
    assert _ch(r"\space") == " "
    assert _ch(r"\newline") == "\n"
    assert _ch(r"\tab") == "\t"


def test_char_unicode():
    assert _ch(r"\u0041") == "A"


def test_char_invalid_named_raises():
    with pytest.raises(ReaderError):
        _ch(r"\bogusname")


def test_reader_error_is_subclass_of_illegal_argument():
    from clojure._core import IllegalArgumentException
    assert issubclass(ReaderError, IllegalArgumentException)
