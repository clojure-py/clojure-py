import pytest

from clojure.lang import Symbol, IFn, IObj, IMeta, Named, IHashEq, NOT_FOUND, ILookup


class TestConstruction:
    def test_intern_simple(self):
        s = Symbol.intern("foo")
        assert s.ns is None
        assert s.name == "foo"

    def test_intern_with_ns_arg(self):
        s = Symbol.intern("user", "foo")
        assert s.ns == "user"
        assert s.name == "foo"

    def test_intern_splits_on_slash(self):
        s = Symbol.intern("user/foo")
        assert s.ns == "user"
        assert s.name == "foo"

    def test_intern_only_slash_is_special(self):
        # The standalone '/' symbol is the division function in Clojure — it has
        # name='/' and ns=None, NOT ns='' name=''. Symbol.intern('/') returns it.
        s = Symbol.intern("/")
        assert s.ns is None
        assert s.name == "/"

    def test_intern_double_slash_splits_at_first(self):
        s = Symbol.intern("a/b/c")
        assert s.ns == "a"
        assert s.name == "b/c"

    def test_direct_construction(self):
        s = Symbol(None, "foo")
        assert s.ns is None and s.name == "foo"

    def test_name_must_be_str(self):
        with pytest.raises(TypeError):
            Symbol(None, 42)

    def test_ns_must_be_str_or_none(self):
        with pytest.raises(TypeError):
            Symbol(42, "foo")


class TestEquality:
    def test_structural(self):
        assert Symbol.intern("foo") == Symbol.intern("foo")
        assert Symbol.intern("a", "b") == Symbol.intern("a", "b")

    def test_different_name(self):
        assert Symbol.intern("foo") != Symbol.intern("bar")

    def test_different_ns(self):
        assert Symbol.intern("a", "x") != Symbol.intern("b", "x")

    def test_ns_vs_no_ns(self):
        assert Symbol.intern("a", "x") != Symbol.intern("x")

    def test_not_equal_to_non_symbol(self):
        assert Symbol.intern("foo") != "foo"
        assert Symbol.intern("foo") != 42
        assert Symbol.intern("foo") is not None


class TestHashing:
    def test_hashable_in_set(self):
        s1 = Symbol.intern("foo")
        s2 = Symbol.intern("foo")
        assert {s1, s2} == {s1}

    def test_hash_stable_within_run(self):
        s = Symbol.intern("user", "foo")
        assert hash(s) == hash(s)

    def test_hasheq_equals_to_eq_implies_hasheq_eq(self):
        # Standard hash contract: equal objects must have equal hasheq.
        a = Symbol.intern("user", "foo")
        b = Symbol.intern("user", "foo")
        assert a == b
        assert a.hasheq() == b.hasheq()

    def test_hasheq_different_for_different_syms(self):
        a = Symbol.intern("foo")
        b = Symbol.intern("bar")
        assert a.hasheq() != b.hasheq()

    def test_hash_returns_int(self):
        s = Symbol.intern("user", "foo")
        assert isinstance(hash(s), int)
        assert isinstance(s.hasheq(), int)


class TestNamed:
    def test_get_namespace(self):
        assert Symbol.intern("user", "foo").get_namespace() == "user"
        assert Symbol.intern("foo").get_namespace() is None

    def test_get_name(self):
        assert Symbol.intern("user", "foo").get_name() == "foo"


class TestStr:
    def test_no_ns(self):
        assert str(Symbol.intern("foo")) == "foo"

    def test_with_ns(self):
        assert str(Symbol.intern("user", "foo")) == "user/foo"

    def test_repr_matches_str(self):
        s = Symbol.intern("user/foo")
        assert repr(s) == str(s)


class TestComparison:
    def test_no_ns_lt_with_ns(self):
        # Java semantics: this.ns == null && other.ns != null → -1
        assert Symbol.intern("foo") < Symbol.intern("user", "foo")

    def test_ns_compare(self):
        assert Symbol.intern("a", "x") < Symbol.intern("b", "x")
        assert Symbol.intern("b", "x") > Symbol.intern("a", "x")

    def test_name_compare_within_same_ns(self):
        assert Symbol.intern("a", "x") < Symbol.intern("a", "y")

    def test_equal_compare_zero(self):
        assert Symbol.intern("foo").compare_to(Symbol.intern("foo")) == 0

    def test_compare_to_non_symbol_raises(self):
        with pytest.raises(TypeError):
            Symbol.intern("foo") < "bar"


class TestMeta:
    def test_default_meta_is_none(self):
        assert Symbol.intern("foo").meta() is None

    def test_with_meta_returns_new_instance(self):
        s = Symbol.intern("foo")
        meta = {"line": 42}
        s2 = s.with_meta(meta)
        assert s2 is not s
        assert s2.meta() is meta
        assert s.meta() is None  # original unchanged
        assert s == s2  # equality ignores meta

    def test_with_meta_idempotent_on_same_meta(self):
        s = Symbol.intern("foo")
        meta = {"line": 42}
        s2 = s.with_meta(meta)
        assert s2.with_meta(meta) is s2


class TestInterfaces:
    def test_isinstance_ifn(self):
        assert isinstance(Symbol.intern("foo"), IFn)

    def test_isinstance_iobj(self):
        assert isinstance(Symbol.intern("foo"), IObj)

    def test_isinstance_imeta_via_iobj(self):
        # IObj extends IMeta in the ABC graph; virtual-subclass propagation
        # should make isinstance true through that chain.
        assert isinstance(Symbol.intern("foo"), IMeta)

    def test_isinstance_named(self):
        assert isinstance(Symbol.intern("foo"), Named)

    def test_isinstance_ihasheq(self):
        assert isinstance(Symbol.intern("foo"), IHashEq)


class TestInvoke:
    class StubMap(ILookup):
        """Minimal ILookup for testing Symbol/Keyword __call__."""
        def __init__(self, d): self.d = d
        def val_at(self, key, not_found=NOT_FOUND):
            if key in self.d:
                return self.d[key]
            return None if not_found is NOT_FOUND else not_found

    def test_lookup_via_ilookup_present(self):
        s = Symbol.intern("foo")
        m = self.StubMap({s: 42})
        assert s(m) == 42

    def test_lookup_via_ilookup_missing(self):
        m = self.StubMap({})
        assert Symbol.intern("foo")(m) is None

    def test_lookup_with_default(self):
        m = self.StubMap({})
        assert Symbol.intern("foo")(m, "default") == "default"

    def test_lookup_dict_fallback(self):
        s = Symbol.intern("foo")
        d = {s: 99}
        assert s(d) == 99

    def test_lookup_dict_missing_returns_none(self):
        assert Symbol.intern("foo")({}) is None

    def test_lookup_none_target(self):
        assert Symbol.intern("foo")(None) is None
        assert Symbol.intern("foo")(None, "x") == "x"
