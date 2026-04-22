"""Namespace operations: intern, refer, alias, import, ns-* helpers."""

from clojure._core import (
    create_ns, intern, refer, alias, import_cls,
    ns_map, ns_aliases, ns_refers, ns_imports, ns_meta,
    symbol, Var,
)


def test_intern_creates_var_as_attribute():
    ns = create_ns(symbol("i.a"))
    v = intern(ns, symbol("foo"))
    assert isinstance(v, Var)
    assert getattr(ns, "foo") is v


def test_intern_idempotent():
    ns = create_ns(symbol("i.b"))
    v1 = intern(ns, symbol("x"))
    v2 = intern(ns, symbol("x"))
    assert v1 is v2


def test_intern_symbol_with_punct():
    ns = create_ns(symbol("i.c"))
    v = intern(ns, symbol("foo?"))
    assert getattr(ns, "foo?") is v


def test_intern_returns_var_bound_to_ns_and_sym():
    ns = create_ns(symbol("i.d"))
    v = intern(ns, symbol("x"))
    assert v.ns is ns
    assert v.sym == symbol("x")
    assert v.is_bound is False  # freshly interned, unbound


def test_refer_installs_as_attribute_and_records():
    src = create_ns(symbol("r.src")); v = intern(src, symbol("x")); v.bind_root(42)
    tgt = create_ns(symbol("r.tgt"))
    refer(tgt, symbol("x"), v)
    assert getattr(tgt, "x") is v
    assert ns_refers(tgt)[symbol("x")] is v


def test_refer_under_different_name():
    src = create_ns(symbol("r2.src")); v = intern(src, symbol("original"))
    tgt = create_ns(symbol("r2.tgt"))
    refer(tgt, symbol("renamed"), v)
    assert getattr(tgt, "renamed") is v
    assert ns_refers(tgt)[symbol("renamed")] is v


def test_alias():
    ns = create_ns(symbol("al.a"))
    target = create_ns(symbol("al.b"))
    alias(ns, symbol("b"), target)
    assert ns_aliases(ns)[symbol("b")] is target


def test_import_cls():
    ns = create_ns(symbol("im.a"))
    import_cls(ns, symbol("DD"), dict)
    assert ns_imports(ns)[symbol("DD")] is dict


def test_ns_map_lists_all_interned_vars():
    ns = create_ns(symbol("m.a"))
    intern(ns, symbol("foo"))
    intern(ns, symbol("bar"))
    intern(ns, symbol("baz?"))
    m = ns_map(ns)
    names = {k.name for k in m.keys()}
    assert names == {"foo", "bar", "baz?"}
    for v in m.values():
        assert isinstance(v, Var)


def test_ns_map_excludes_non_var_attrs():
    ns = create_ns(symbol("m.b"))
    intern(ns, symbol("v"))
    # Set a non-Var attribute — should be excluded from ns_map.
    setattr(ns, "not_a_var", 42)
    m = ns_map(ns)
    names = {k.name for k in m.keys()}
    assert names == {"v"}


def test_ns_meta_default_none():
    ns = create_ns(symbol("meta.a"))
    assert ns_meta(ns) is None


def test_refers_dict_is_empty_initially():
    ns = create_ns(symbol("r3.empty"))
    assert ns_refers(ns) == {}


def test_aliases_dict_is_empty_initially():
    ns = create_ns(symbol("al.empty"))
    assert ns_aliases(ns) == {}


def test_imports_dict_is_empty_initially():
    ns = create_ns(symbol("im.empty"))
    assert ns_imports(ns) == {}


def test_var_deref_via_ns_attr():
    """The full path: create ns, intern var, bind root, access via attribute → deref."""
    ns = create_ns(symbol("deref.test"))
    v = intern(ns, symbol("greeting"))
    v.bind_root("hello")
    assert getattr(ns, "greeting").deref() == "hello"
