"""Tests for the expanded Var meta API and Namespace's Python-import helpers."""
import importlib
import math

import pytest

from clojure.lang import (
    Namespace, Var, Symbol, Keyword, PersistentHashMap,
)


# =========================================================================
# Var meta — set_meta / set_tag / get_tag / set_private
# =========================================================================

class TestSetMeta:
    def setup_method(self):
        self.ns = Namespace.find_or_create(Symbol.intern("test.var.set_meta"))
        self.v = Var.intern(self.ns, Symbol.intern("v"), 0)

    def test_set_meta_replaces(self):
        self.v.set_meta(PersistentHashMap.create(Keyword.intern("doc"), "hello"))
        m = self.v.meta()
        assert m.val_at(Keyword.intern("doc")) == "hello"

    def test_set_meta_preserves_name_and_ns(self):
        # Name/ns keys must always be present even if set_meta is called with
        # a map that lacks them.
        self.v.set_meta(PersistentHashMap.create(Keyword.intern("doc"), "x"))
        m = self.v.meta()
        assert m.val_at(Keyword.intern("name")).name == "v"
        assert m.val_at(Keyword.intern("ns")) is self.ns

    def test_set_meta_none_uses_empty(self):
        self.v.set_meta(None)
        m = self.v.meta()
        # Still has name/ns
        assert m.val_at(Keyword.intern("name")).name == "v"

    def test_set_meta_non_map_raises(self):
        with pytest.raises(TypeError):
            self.v.set_meta("not a map")


class TestTag:
    def setup_method(self):
        self.ns = Namespace.find_or_create(Symbol.intern("test.var.tag"))

    def test_get_tag_default_none(self):
        v = Var.intern(self.ns, Symbol.intern("a"), 0)
        assert v.get_tag() is None

    def test_set_tag_then_get(self):
        v = Var.intern(self.ns, Symbol.intern("b"), 0)
        v.set_tag(Symbol.intern("long"))
        assert v.get_tag().name == "long"

    def test_set_tag_accepts_any_value(self):
        v = Var.intern(self.ns, Symbol.intern("c"), 0)
        v.set_tag("string-tag")
        assert v.get_tag() == "string-tag"


class TestPrivate:
    def setup_method(self):
        self.ns = Namespace.find_or_create(Symbol.intern("test.var.private"))

    def test_default_public(self):
        v = Var.intern(self.ns, Symbol.intern("a"), 0)
        assert v.is_public()

    def test_set_private_makes_non_public(self):
        v = Var.intern(self.ns, Symbol.intern("b"), 0)
        v.set_private()
        assert not v.is_public()

    def test_set_private_false_restores(self):
        v = Var.intern(self.ns, Symbol.intern("c"), 0)
        v.set_private(True)
        v.set_private(False)
        assert v.is_public()


# =========================================================================
# Namespace.import_class — bind any Python value
# =========================================================================

class TestImportClass:
    def setup_method(self):
        self.ns = Namespace.find_or_create(Symbol.intern("test.import.class"))

    def test_import_class_with_explicit_symbol(self):
        from collections import Counter
        self.ns.import_class(Symbol.intern("MyCounter"), Counter)
        assert self.ns.get_mapping(Symbol.intern("MyCounter")) is Counter

    def test_import_class_derives_name(self):
        from collections import Counter
        self.ns.import_class(Counter)
        assert self.ns.get_mapping(Symbol.intern("Counter")) is Counter

    def test_import_class_function(self):
        # Bind a function — same path as a class.
        self.ns.import_class(Symbol.intern("sqrt-fn"), math.sqrt)
        assert self.ns.get_mapping(Symbol.intern("sqrt-fn"))(16) == 4.0

    def test_import_class_module(self):
        # Bind a module under an explicit name.
        self.ns.import_class(Symbol.intern("mathmod"), math)
        assert self.ns.get_mapping(Symbol.intern("mathmod")) is math

    def test_import_class_derives_short_name_for_dotted(self):
        # importlib.import_module returns an object whose __name__ may be
        # dotted — derive the last component.
        sub = importlib.import_module("collections.abc")
        self.ns.import_class(sub)
        # The derived name is the last component of sub.__name__.
        assert self.ns.get_mapping(Symbol.intern("_collections_abc")) is sub or \
               self.ns.get_mapping(Symbol.intern("abc")) is sub

    def test_import_class_no_name_attr_raises(self):
        class _Anon:
            pass
        instance = _Anon()
        # Plain instances usually have no __name__.
        with pytest.raises(TypeError):
            self.ns.import_class(instance)

    def test_import_class_non_symbol_first_arg_raises(self):
        with pytest.raises(TypeError):
            self.ns.import_class("not-a-symbol", math)


# =========================================================================
# Namespace.import_module — by string module name
# =========================================================================

class TestImportModule:
    def setup_method(self):
        self.ns = Namespace.find_or_create(Symbol.intern("test.import.module"))

    def test_import_module_basic(self):
        self.ns.import_module("math")
        assert self.ns.get_mapping(Symbol.intern("math")) is math

    def test_import_module_dotted_uses_last_component(self):
        self.ns.import_module("collections.abc")
        # 'abc' bound to whatever importlib returns for collections.abc.
        sub = importlib.import_module("collections.abc")
        assert self.ns.get_mapping(Symbol.intern("abc")) is sub

    def test_import_module_with_string_alias(self):
        self.ns.import_module("math", "m")
        assert self.ns.get_mapping(Symbol.intern("m")) is math

    def test_import_module_with_symbol_alias(self):
        alias = Symbol.intern("M")
        self.ns.import_module("math", alias)
        assert self.ns.get_mapping(alias) is math

    def test_import_module_invalid_alias_type_raises(self):
        with pytest.raises(TypeError):
            self.ns.import_module("math", 42)


# =========================================================================
# Namespace.import_from — `from X import a, b, (c as d)`
# =========================================================================

class TestImportFrom:
    def setup_method(self):
        self.ns = Namespace.find_or_create(Symbol.intern("test.import.from"))

    def test_import_simple_names(self):
        self.ns.import_from("math", "sqrt", "pi")
        assert self.ns.get_mapping(Symbol.intern("sqrt")) is math.sqrt
        assert self.ns.get_mapping(Symbol.intern("pi")) is math.pi

    def test_import_with_rename(self):
        self.ns.import_from("math", ("floor", "flr"))
        assert self.ns.get_mapping(Symbol.intern("flr")) is math.floor

    def test_import_mixed(self):
        self.ns.import_from("math", "ceil", ("floor", "flr"), "pi")
        assert self.ns.get_mapping(Symbol.intern("ceil")) is math.ceil
        assert self.ns.get_mapping(Symbol.intern("flr")) is math.floor
        assert self.ns.get_mapping(Symbol.intern("pi")) is math.pi

    def test_import_from_returns_self_for_chaining(self):
        result = self.ns.import_from("math", "sqrt")
        assert result is self.ns

    def test_import_unknown_attr_raises(self):
        with pytest.raises(AttributeError):
            self.ns.import_from("math", "definitely_not_a_real_attr")

    def test_import_invalid_entry_type(self):
        with pytest.raises(TypeError):
            self.ns.import_from("math", 42)
