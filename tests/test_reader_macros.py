"""Phase R4 — reader macros."""

import pytest
from clojure._core import (
    read_string, PersistentList, PersistentVector, PersistentHashMap,
    PersistentArrayMap, keyword, symbol, ReaderError,
)


# --- Quote ---

def test_quote_simple():
    """'x → (quote x)"""
    lst = read_string("'x")
    assert isinstance(lst, PersistentList)
    items = list(lst)
    assert items[0] == symbol("quote")
    assert items[1] == symbol("x")


def test_quote_of_list():
    lst = read_string("'(1 2 3)")
    assert list(lst)[0] == symbol("quote")
    inner = list(lst)[1]
    assert list(inner) == [1, 2, 3]


def test_quote_nested():
    lst = read_string("''x")
    # ''x → (quote (quote x))
    items = list(lst)
    assert items[0] == symbol("quote")
    inner = items[1]
    assert list(inner)[0] == symbol("quote")


# --- Deref ---

def test_deref_simple():
    lst = read_string("@x")
    items = list(lst)
    assert items[0] == symbol("deref")
    assert items[1] == symbol("x")


def test_deref_expression():
    lst = read_string("@(f 1)")
    items = list(lst)
    assert items[0] == symbol("deref")
    inner = items[1]
    assert list(inner)[0] == symbol("f")


# --- Var quote ---

def test_var_quote_simple():
    lst = read_string("#'foo")
    items = list(lst)
    assert items[0] == symbol("var")
    assert items[1] == symbol("foo")


# --- Discard ---

def test_discard_simple():
    """#_ x y → y"""
    assert read_string("#_ x y") == symbol("y")


def test_discard_in_collection():
    v = read_string("[1 #_ 2 3]")
    assert list(v) == [1, 3]


def test_discard_nested():
    v = read_string("[1 #_ #_ 2 3 4]")
    # Two discards consume 2 AND 3, leaving 4.
    assert list(v) == [1, 4]


# --- Meta (^) ---

def test_meta_keyword_shorthand():
    """^:private x attaches {:private true} to x"""
    v = read_string("^:private [1 2 3]")
    m = v.meta
    assert m is not None
    assert m.val_at(keyword("private")) is True


def test_meta_map_attachment():
    v = read_string("^{:tag 'MyClass :other 42} [1 2]")
    m = v.meta
    assert m is not None
    assert m.val_at(keyword("other")) == 42


def test_meta_symbol_shorthand():
    """^Foo x attaches {:tag Foo} to x"""
    v = read_string("^Foo [1 2]")
    m = v.meta
    assert m is not None
    assert m.val_at(keyword("tag")) == symbol("Foo")


def test_meta_string_shorthand():
    v = read_string('^"MyType" [1]')
    m = v.meta
    assert m.val_at(keyword("tag")) == "MyType"


# --- Comments ---

def test_line_comment_before_form():
    assert read_string("; this is a comment\n42") == 42


def test_line_comment_inside_collection():
    v = read_string("[1 ; skip\n 2 3]")
    assert list(v) == [1, 2, 3]
