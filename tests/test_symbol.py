from clojure._core import Symbol, symbol

def test_symbol_no_ns():
    s = symbol("foo")
    assert s.name == "foo"
    assert s.ns is None

def test_symbol_with_ns():
    s = symbol("my.ns", "foo")
    assert s.ns == "my.ns"
    assert s.name == "foo"

def test_symbol_equality_by_value():
    assert symbol("foo") == symbol("foo")
    assert symbol("a", "b") == symbol("a", "b")
    assert symbol("foo") != symbol("bar")
    assert symbol("a", "b") != symbol("b")

def test_symbol_identity_not_interned():
    assert symbol("foo") is not symbol("foo")

def test_symbol_hash_value_based():
    assert hash(symbol("foo")) == hash(symbol("foo"))
    assert hash(symbol("a", "b")) == hash(symbol("a", "b"))

def test_symbol_repr():
    assert repr(symbol("foo")) == "foo"
    assert repr(symbol("my.ns", "foo")) == "my.ns/foo"

def test_symbol_isinstance():
    assert isinstance(symbol("foo"), Symbol)

def test_with_meta_preserves_value_equality():
    s1 = symbol("foo")
    s2 = s1.with_meta({"a": 1})
    assert s1 == s2
    assert hash(s1) == hash(s2)

def test_with_meta_independent_instances():
    s1 = symbol("foo")
    s2 = s1.with_meta({"a": 1})
    assert s1.meta is None
    assert s2.meta == {"a": 1}
