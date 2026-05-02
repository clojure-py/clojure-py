"""Tests for MultiFn / hierarchy operations / with_bindings / bound_fn /
MethodImplCache."""
import threading
import pytest

from clojure.lang import (
    MultiFn, MethodImplCache, MethodImplCache_Entry,
    global_hierarchy, derive, underive, isa_pred,
    parents_of, ancestors_of, descendants_of, make_hierarchy,
    Var, Namespace, Symbol, Keyword,
    PersistentHashMap, PersistentHashSet, PersistentVector,
    with_bindings, bound_fn,
    IFn,
)


# =========================================================================
# with_bindings / bound_fn
# =========================================================================

class TestWithBindings:
    def setup_method(self):
        self.ns = Namespace.find_or_create(Symbol.intern("test.with_bindings"))

    def test_pushes_and_pops(self):
        v = Var.intern(self.ns, Symbol.intern("a"), "out").set_dynamic()
        assert v.deref() == "out"
        result = with_bindings(
            PersistentHashMap.create(v, "in"),
            lambda: v.deref())
        assert result == "in"
        assert v.deref() == "out"  # popped

    def test_pops_on_exception(self):
        v = Var.intern(self.ns, Symbol.intern("b"), "out").set_dynamic()

        def raise_inside():
            raise RuntimeError("boom")

        with pytest.raises(RuntimeError):
            with_bindings(
                PersistentHashMap.create(v, "in"),
                raise_inside)
        # Frame must be popped even on exception.
        assert v.deref() == "out"

    def test_returns_fn_result(self):
        v = Var.intern(self.ns, Symbol.intern("c"), 0).set_dynamic()
        assert with_bindings(PersistentHashMap.create(v, 99), lambda: v.deref() * 2) == 198


class TestBoundFn:
    def setup_method(self):
        self.ns = Namespace.find_or_create(Symbol.intern("test.bound_fn"))

    def test_captures_current_frame(self):
        v = Var.intern(self.ns, Symbol.intern("color"), "default").set_dynamic()
        captured = with_bindings(
            PersistentHashMap.create(v, "blue"),
            lambda: bound_fn(lambda: v.deref()))
        # After with_bindings exits, bare deref returns default.
        assert v.deref() == "default"
        # But the bound_fn-wrapped callable still sees the captured "blue".
        assert captured() == "blue"

    def test_bound_fn_runs_on_other_thread(self):
        v = Var.intern(self.ns, Symbol.intern("flag"), "main").set_dynamic()

        result_box = [None]
        def grab():
            result_box[0] = v.deref()

        bound = with_bindings(
            PersistentHashMap.create(v, "captured"),
            lambda: bound_fn(grab))

        t = threading.Thread(target=bound)
        t.start(); t.join()
        assert result_box[0] == "captured"

    def test_bound_fn_restores_caller_frame(self):
        v = Var.intern(self.ns, Symbol.intern("ctx"), "outer").set_dynamic()
        bound = bound_fn(lambda: v.deref())
        # Now push another binding around the call.
        result = with_bindings(
            PersistentHashMap.create(v, "intermediate"),
            lambda: (bound(), v.deref()))
        # bound() saw the captured frame ("outer"), then bound's wrapper
        # restored the "intermediate" caller frame.
        assert result == ("outer", "intermediate")


# =========================================================================
# Hierarchy operations
# =========================================================================

class TestHierarchy:
    def test_make_empty(self):
        h = make_hierarchy()
        assert parents_of(h, Keyword.intern("anything")) is None

    def test_isa_identity(self):
        h = make_hierarchy()
        kw = Keyword.intern("x")
        assert isa_pred(h, kw, kw)

    def test_isa_python_class(self):
        h = make_hierarchy()
        assert isa_pred(h, int, object)
        assert isa_pred(h, bool, int)
        assert not isa_pred(h, str, int)

    def test_isa_via_derived_hierarchy(self):
        h = make_hierarchy()
        h = derive(h, Keyword.intern("dog"), Keyword.intern("animal"))
        assert isa_pred(h, Keyword.intern("dog"), Keyword.intern("animal"))
        assert not isa_pred(h, Keyword.intern("cat"), Keyword.intern("animal"))

    def test_isa_transitive(self):
        h = make_hierarchy()
        h = derive(h, Keyword.intern("poodle"), Keyword.intern("dog"))
        h = derive(h, Keyword.intern("dog"), Keyword.intern("mammal"))
        assert isa_pred(h, Keyword.intern("poodle"), Keyword.intern("mammal"))

    def test_isa_vector_componentwise(self):
        h = make_hierarchy()
        h = derive(h, Keyword.intern("dog"), Keyword.intern("animal"))
        v_concrete = PersistentVector.create(Keyword.intern("dog"), int)
        v_abstract = PersistentVector.create(Keyword.intern("animal"), object)
        assert isa_pred(h, v_concrete, v_abstract)

    def test_derive_self_raises(self):
        h = make_hierarchy()
        with pytest.raises(ValueError):
            derive(h, Keyword.intern("a"), Keyword.intern("a"))

    def test_derive_idempotent(self):
        h = make_hierarchy()
        h = derive(h, Keyword.intern("a"), Keyword.intern("b"))
        # Re-derive should be a no-op.
        h2 = derive(h, Keyword.intern("a"), Keyword.intern("b"))
        assert h is h2

    def test_derive_cycle_raises(self):
        h = make_hierarchy()
        h = derive(h, Keyword.intern("a"), Keyword.intern("b"))
        with pytest.raises(ValueError):
            derive(h, Keyword.intern("b"), Keyword.intern("a"))

    def test_underive(self):
        h = make_hierarchy()
        h = derive(h, Keyword.intern("a"), Keyword.intern("b"))
        assert isa_pred(h, Keyword.intern("a"), Keyword.intern("b"))
        h = underive(h, Keyword.intern("a"), Keyword.intern("b"))
        assert not isa_pred(h, Keyword.intern("a"), Keyword.intern("b"))

    def test_descendants(self):
        h = make_hierarchy()
        h = derive(h, Keyword.intern("a"), Keyword.intern("z"))
        h = derive(h, Keyword.intern("b"), Keyword.intern("z"))
        descs = descendants_of(h, Keyword.intern("z"))
        names = {k.get_name() for k in descs}
        assert names == {"a", "b"}


# =========================================================================
# MultiFn — basic
# =========================================================================

class TestMultiFnBasics:
    def _multimethod(self, dispatch_fn, default=None):
        return MultiFn("test-mf", dispatch_fn, default_dispatch_val=default)

    def test_dispatch_by_type(self):
        mf = self._multimethod(lambda x: type(x))
        mf.add_method(int, lambda x: f"int({x})")
        mf.add_method(str, lambda x: f"str({x})")
        assert mf(5) == "int(5)"
        assert mf("hi") == "str(hi)"

    def test_default_dispatch_value(self):
        mf = self._multimethod(lambda x: type(x), default=object)
        mf.add_method(int, lambda x: "int!")
        mf.add_method(object, lambda x: "fallback")
        assert mf(5) == "int!"
        assert mf("hi") == "fallback"
        assert mf([1, 2]) == "fallback"

    def test_no_method_raises(self):
        mf = self._multimethod(lambda x: type(x))
        mf.add_method(int, lambda x: "int")
        with pytest.raises(RuntimeError):
            mf(3.14)   # no method for float

    def test_remove_method(self):
        mf = self._multimethod(lambda x: type(x), default=object)
        mf.add_method(int, lambda x: "int")
        mf.add_method(object, lambda x: "fallback")
        assert mf(5) == "int"
        mf.remove_method(int)
        assert mf(5) == "fallback"

    def test_reset(self):
        mf = self._multimethod(lambda x: type(x))
        mf.add_method(int, lambda x: "int")
        mf.reset()
        assert mf.get_method_table().count() == 0

    def test_chainable_returns_self(self):
        mf = self._multimethod(lambda x: type(x))
        assert mf.add_method(int, lambda x: 0) is mf
        assert mf.remove_method(int) is mf
        assert mf.reset() is mf

    def test_isinstance_ifn(self):
        mf = self._multimethod(lambda x: x)
        assert isinstance(mf, IFn)


# =========================================================================
# MultiFn — hierarchy-based dispatch
# =========================================================================

class TestMultiFnHierarchy:
    def setup_method(self):
        # Use a fresh local hierarchy ref, NOT the global one (test isolation).
        from clojure.lang import Atom
        self.h_atom = Atom(make_hierarchy())
        self.h_atom.swap(lambda h: derive(h, Keyword.intern("dog"),
                                          Keyword.intern("animal")))
        self.h_atom.swap(lambda h: derive(h, Keyword.intern("cat"),
                                          Keyword.intern("animal")))
        self.h_atom.swap(lambda h: derive(h, Keyword.intern("poodle"),
                                          Keyword.intern("dog")))

    def test_dispatch_via_isa(self):
        mf = MultiFn("speak", lambda x: x, hierarchy_ref=self.h_atom)
        mf.add_method(Keyword.intern("animal"), lambda x: "generic noise")
        # dog isa animal → matches the animal method.
        assert mf(Keyword.intern("dog")) == "generic noise"
        # poodle isa animal too (transitively) → still matches.
        assert mf(Keyword.intern("poodle")) == "generic noise"

    def test_more_specific_method_wins(self):
        mf = MultiFn("speak", lambda x: x, hierarchy_ref=self.h_atom)
        mf.add_method(Keyword.intern("animal"), lambda x: "generic")
        mf.add_method(Keyword.intern("dog"), lambda x: "woof")
        assert mf(Keyword.intern("dog")) == "woof"          # specific wins
        assert mf(Keyword.intern("poodle")) == "woof"       # poodle isa dog → woof
        assert mf(Keyword.intern("cat")) == "generic"        # cat isa animal but not dog

    def test_ambiguous_raises_unless_preferred(self):
        # Both :dog and :cat methods, dispatch on a vector that isa? both.
        mf = MultiFn("speak", lambda x: x, hierarchy_ref=self.h_atom)
        # Add a derivation so :pet isa :dog AND :cat (artificial multi-parent).
        self.h_atom.swap(lambda h: derive(h, Keyword.intern("pet"),
                                          Keyword.intern("dog")))
        self.h_atom.swap(lambda h: derive(h, Keyword.intern("pet"),
                                          Keyword.intern("cat")))
        mf.add_method(Keyword.intern("dog"), lambda x: "woof")
        mf.add_method(Keyword.intern("cat"), lambda x: "meow")
        with pytest.raises(RuntimeError):
            mf(Keyword.intern("pet"))

    def test_prefer_method_resolves_ambiguity(self):
        self.h_atom.swap(lambda h: derive(h, Keyword.intern("pet"),
                                          Keyword.intern("dog")))
        self.h_atom.swap(lambda h: derive(h, Keyword.intern("pet"),
                                          Keyword.intern("cat")))
        mf = MultiFn("speak", lambda x: x, hierarchy_ref=self.h_atom)
        mf.add_method(Keyword.intern("dog"), lambda x: "woof")
        mf.add_method(Keyword.intern("cat"), lambda x: "meow")
        mf.prefer_method(Keyword.intern("dog"), Keyword.intern("cat"))
        assert mf(Keyword.intern("pet")) == "woof"

    def test_get_method_returns_fn_or_none(self):
        mf = MultiFn("m", lambda x: x, hierarchy_ref=self.h_atom)
        fn = lambda x: "ok"
        mf.add_method(Keyword.intern("dog"), fn)
        assert mf.get_method(Keyword.intern("dog")) is fn
        assert mf.get_method(Keyword.intern("nope")) is None


# =========================================================================
# MultiFn — multi-arg dispatch
# =========================================================================

class TestMultiFnMultiArg:
    def test_dispatch_on_combined_args(self):
        # Common pattern: dispatch on (vector of types).
        def disp(*args):
            return PersistentVector.create(*[type(a) for a in args])

        mf = MultiFn("op", disp)
        mf.add_method(PersistentVector.create(int, int), lambda a, b: a + b)
        mf.add_method(PersistentVector.create(str, str), lambda a, b: a + " " + b)
        assert mf(2, 3) == 5
        assert mf("hi", "there") == "hi there"


# =========================================================================
# MethodImplCache
# =========================================================================

class TestMethodImplCache:
    def test_map_based_lookup(self):
        e1 = MethodImplCache_Entry(int, lambda x: f"int({x})")
        e2 = MethodImplCache_Entry(str, lambda x: f"str({x})")
        cache = MethodImplCache(
            sym=Symbol.intern("p1"),
            protocol=PersistentHashMap.create(),
            methodk=Keyword.intern("m1"),
            map={int: e1, str: e2})
        assert cache.fn_for(int)(5) == "int(5)"
        assert cache.fn_for(str)("hi") == "str(hi)"
        assert cache.fn_for(float) is None

    def test_mre_caches_last(self):
        e = MethodImplCache_Entry(int, lambda x: x)
        cache = MethodImplCache(
            sym=Symbol.intern("p"),
            protocol=PersistentHashMap.create(),
            methodk=Keyword.intern("m"),
            map={int: e})
        # First call walks; second hits the mre cache.
        cache.fn_for(int)
        cache.fn_for(int)   # mre fast path


# =========================================================================
# global_hierarchy is the default
# =========================================================================

class TestGlobalHierarchy:
    def test_default_hierarchy_is_global(self):
        # MultiFn with no hierarchy_ref uses global_hierarchy.
        mf = MultiFn("x", lambda x: x)
        assert mf.hierarchy_ref is global_hierarchy
