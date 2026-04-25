"""Phase R2 — read_string on atoms."""

import pytest
from clojure._core import read_string, Symbol, Keyword, symbol, keyword, ReaderError


def test_read_nil():
    assert read_string("nil") is None


def test_read_true_false():
    assert read_string("true") is True
    assert read_string("false") is False


def test_read_int():
    assert read_string("42") == 42
    assert read_string("-17") == -17


def test_read_float():
    assert read_string("3.14") == 3.14


def test_read_string_literal():
    assert read_string('"hello"') == "hello"


def test_read_char_literal():
    from clojure._core import Char
    assert read_string(r"\A") == Char("A")
    assert read_string(r"\space") == Char(" ")


def test_read_symbol_simple():
    s = read_string("foo")
    assert isinstance(s, Symbol)
    assert s.name == "foo"
    assert s.ns is None


def test_read_symbol_namespaced():
    s = read_string("my.ns/foo")
    assert isinstance(s, Symbol)
    assert s.ns == "my.ns"
    assert s.name == "foo"


def test_read_symbol_slash_alone():
    s = read_string("/")
    assert isinstance(s, Symbol)
    assert s.name == "/"


def test_read_keyword_simple():
    k = read_string(":foo")
    assert isinstance(k, Keyword)
    assert k.name == "foo"
    assert k.ns is None


def test_read_keyword_namespaced():
    k = read_string(":ns/name")
    assert isinstance(k, Keyword)
    assert k.ns == "ns"
    assert k.name == "name"


def test_read_keyword_identity():
    assert read_string(":foo") is read_string(":foo")  # interned


def test_whitespace_ignored():
    assert read_string("  42  ") == 42


def test_comment_ignored():
    assert read_string("; comment here\nfoo").name == "foo"


def test_eof_raises():
    with pytest.raises(ReaderError, match="EOF"):
        read_string("")


def test_trailing_content_raises():
    with pytest.raises(ReaderError, match="trailing"):
        read_string("1 2")


def test_unmatched_delimiter_raises():
    with pytest.raises(ReaderError, match="[Uu]nmatched"):
        read_string(")")


def test_commas_as_whitespace():
    assert read_string(",,, 42 ,,,") == 42
