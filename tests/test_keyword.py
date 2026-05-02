import gc
import pytest

from clojure.lang import Keyword, Symbol, IFn, Named, IHashEq, NOT_FOUND, ILookup


class TestConstructionAndInterning:
    def test_intern_from_name(self):
        k = Keyword.intern("foo")
        assert k.get_namespace() is None
        assert k.get_name() == "foo"

    def test_intern_from_ns_name(self):
        k = Keyword.intern("user", "foo")
        assert k.get_namespace() == "user"
        assert k.get_name() == "foo"

    def test_intern_from_symbol(self):
        s = Symbol.intern("user", "foo")
        k = Keyword.intern(s)
        assert k.get_namespace() == "user"
        assert k.get_name() == "foo"

    def test_intern_returns_canonical_instance(self):
        # Same args → same Python object.
        a = Keyword.intern("foo")
        b = Keyword.intern("foo")
        assert a is b

    def test_intern_canonical_across_arg_forms(self):
        a = Keyword.intern("user/foo")
        b = Keyword.intern("user", "foo")
        c = Keyword.intern(Symbol.intern("user", "foo"))
        assert a is b is c

    def test_direct_construction_allowed(self):
        # Keyword(sym) bypasses interning but stays equal to the interned one.
        sym = Symbol.intern("foo")
        k = Keyword(sym)
        assert k == Keyword.intern("foo")

    def test_find_returns_existing_or_none(self):
        k = Keyword.intern("findme")  # hold a reference — entries are weak
        assert Keyword.find("findme") is k
        # A name we haven't interned should not exist.
        assert Keyword.find("definitely-not-interned-9999") is None


class TestEquality:
    def test_interned_identity(self):
        assert Keyword.intern("foo") is Keyword.intern("foo")
        assert Keyword.intern("foo") == Keyword.intern("foo")

    def test_structural_equal_when_not_interned(self):
        sym = Symbol.intern("foo")
        a = Keyword(sym)
        b = Keyword(sym)
        assert a == b  # not necessarily `is` — direct construction skips interning

    def test_different_keywords_not_equal(self):
        assert Keyword.intern("a") != Keyword.intern("b")
        assert Keyword.intern("a", "x") != Keyword.intern("b", "x")

    def test_keyword_not_equal_to_symbol(self):
        assert Keyword.intern("foo") != Symbol.intern("foo")


class TestHashing:
    def test_hashable_in_set(self):
        assert {Keyword.intern("foo"), Keyword.intern("foo")} == {Keyword.intern("foo")}

    def test_hash_diff_from_symbol_hash(self):
        # Java: keyword.hashCode = sym.hashCode + 0x9e3779b9.
        s = Symbol.intern("foo")
        k = Keyword.intern(s)
        assert hash(k) != hash(s)

    def test_hasheq_diff_from_symbol_hasheq(self):
        s = Symbol.intern("foo")
        k = Keyword.intern(s)
        assert k.hasheq() != s.hasheq()


class TestStr:
    def test_no_ns(self):
        assert str(Keyword.intern("foo")) == ":foo"

    def test_with_ns(self):
        assert str(Keyword.intern("user", "foo")) == ":user/foo"


class TestComparison:
    def test_lt_via_underlying_sym(self):
        assert Keyword.intern("a") < Keyword.intern("b")

    def test_compare_to_non_keyword_raises(self):
        with pytest.raises(TypeError):
            Keyword.intern("foo") < Symbol.intern("foo")


class TestInterfaces:
    def test_isinstance_ifn(self):
        assert isinstance(Keyword.intern("foo"), IFn)

    def test_isinstance_named(self):
        assert isinstance(Keyword.intern("foo"), Named)

    def test_isinstance_ihasheq(self):
        assert isinstance(Keyword.intern("foo"), IHashEq)


class TestInvoke:
    class StubMap(ILookup):
        def __init__(self, d): self.d = d
        def val_at(self, key, not_found=NOT_FOUND):
            if key in self.d:
                return self.d[key]
            return None if not_found is NOT_FOUND else not_found

    def test_lookup_present(self):
        k = Keyword.intern("foo")
        assert k(self.StubMap({k: 42})) == 42

    def test_lookup_missing(self):
        assert Keyword.intern("nope")(self.StubMap({})) is None

    def test_lookup_with_default(self):
        assert Keyword.intern("nope")(self.StubMap({}), "x") == "x"

    def test_lookup_dict_fallback(self):
        k = Keyword.intern("foo")
        assert k({k: 99}) == 99

    def test_lookup_none_target(self):
        assert Keyword.intern("foo")(None) is None


class TestWeakInterning:
    def test_dropped_keywords_can_be_collected(self):
        # Intern a keyword with a unique name, drop the only reference, force gc,
        # and verify find() no longer returns it. This validates the weak-value
        # interning behavior matches the JVM (which uses WeakReference too).
        name = "weak-test-keyword-unique-name-xyz"
        k = Keyword.intern(name)
        assert Keyword.find(name) is k
        del k
        gc.collect()
        # After collection the WeakValueDictionary entry should be gone.
        # Note: this may be flaky if some other reference is held; the unique
        # name minimizes that risk.
        assert Keyword.find(name) is None
