"""Tests for the runtime layer: AFn, RestFn, AReference, ARef, Var, Namespace, Binding."""
import threading
import pytest

from clojure.lang import (
    AFn, RestFn, ArityException, AReference, ARef,
    Var, Binding, Namespace,
    Symbol, Keyword, PersistentHashMap, PersistentList, IteratorSeq,
    IFn, IRef, IDeref, IReference, IMeta, Settable,
    Murmur3,
)


# =========================================================================
# AFn
# =========================================================================

class TestAFn:
    def test_default_call_raises_arity(self):
        with pytest.raises(ArityException):
            AFn()()

    def test_subclass_overrides_call(self):
        class Inc(AFn):
            def __call__(self, x):
                return x + 1
        assert Inc()(5) == 6

    def test_arity_exception_carries_info(self):
        try:
            AFn()(1, 2, 3)
        except ArityException as e:
            assert e.actual == 3
            assert "AFn" in e.name

    def test_apply_to_iseq(self):
        class Sum2(AFn):
            def __call__(self, a, b):
                return a + b
        seq = PersistentList.create([3, 4])
        assert Sum2().apply_to(seq) == 7

    def test_apply_to_python_list(self):
        class Sum2(AFn):
            def __call__(self, a, b):
                return a + b
        assert Sum2().apply_to([3, 4]) == 7

    def test_apply_to_none_means_no_args(self):
        class Zero(AFn):
            def __call__(self):
                return "ok"
        assert Zero().apply_to(None) == "ok"

    def test_isinstance_ifn(self):
        assert isinstance(AFn(), IFn)


# =========================================================================
# RestFn
# =========================================================================

class TestRestFn:
    def test_required_arity_default_zero(self):
        class Echo(RestFn):
            def do_invoke(self, rest_seq):
                return rest_seq
        result = Echo()(1, 2, 3)
        # rest_seq should walk to [1, 2, 3]
        items = []
        s = result
        while s is not None:
            items.append(s.first())
            s = s.next()
        assert items == [1, 2, 3]

    def test_required_arity_enforced(self):
        class Two(RestFn):
            def required_arity(self):
                return 2
            def do_invoke(self, a, b, rest_seq):
                return (a, b, list(rest_seq) if rest_seq is not None else [])
        with pytest.raises(ArityException):
            Two()(1)
        result = Two()(10, 20)
        assert result == (10, 20, [])
        result = Two()(10, 20, 30, 40)
        assert result == (10, 20, [30, 40])

    def test_isinstance_ifn(self):
        assert isinstance(RestFn(), IFn)


# =========================================================================
# Binding
# =========================================================================

class TestBinding:
    def test_basic(self):
        b = Binding(42)
        assert b.val == 42
        assert b.rest is None

    def test_chained(self):
        a = Binding(1)
        b = Binding(2, a)
        assert b.val == 2
        assert b.rest is a

    def test_val_mutable(self):
        b = Binding(1)
        b.val = 99
        assert b.val == 99


# =========================================================================
# AReference
# =========================================================================

class TestAReference:
    def test_meta_default_none(self):
        r = AReference()
        assert r.meta() is None

    def test_meta_provided(self):
        r = AReference({"k": "v"})
        assert r.meta() == {"k": "v"}

    def test_reset_meta(self):
        r = AReference()
        r.reset_meta({"new": 1})
        assert r.meta() == {"new": 1}

    def test_alter_meta_replaces(self):
        r = AReference({"count": 0})
        # alter_fn(current_meta, increment) → updated map
        def bump(current, inc):
            return {"count": current["count"] + inc}
        r.alter_meta(bump, [5])
        assert r.meta() == {"count": 5}

    def test_isinstance_ireference(self):
        assert isinstance(AReference(), IReference)
        assert isinstance(AReference(), IMeta)


# =========================================================================
# ARef
# =========================================================================

class _Cell(ARef):
    """Minimal concrete ARef for testing — holds a value, deref returns it."""
    def __init__(self, val):
        ARef.__init__(self)
        self.val = val

    def deref(self):
        return self.val


class TestARef:
    def test_get_set_validator(self):
        c = _Cell(5)
        assert c.get_validator() is None
        c.set_validator(lambda v: v > 0)
        assert c.get_validator() is not None

    def test_validator_rejects_invalid(self):
        c = _Cell(5)
        # Validator must hold for current value first.
        c.set_validator(lambda v: v > 0)
        # Setting a validator that fails on current value raises.
        with pytest.raises(RuntimeError):
            c.set_validator(lambda v: v > 100)

    def test_watches_default_empty(self):
        c = _Cell(0)
        assert c.get_watches().count() == 0

    def test_add_watch(self):
        c = _Cell(0)
        c.add_watch("k", lambda *a: None)
        assert c.get_watches().count() == 1

    def test_remove_watch(self):
        c = _Cell(0)
        c.add_watch("k", lambda *a: None)
        c.remove_watch("k")
        assert c.get_watches().count() == 0

    def test_notify_watches_calls_callbacks(self):
        c = _Cell(0)
        seen = []
        def watcher(key, ref, old, new):
            seen.append((key, old, new))
        c.add_watch("w", watcher)
        c.notify_watches(0, 99)
        assert seen == [("w", 0, 99)]

    def test_isinstance_iref_ideref(self):
        c = _Cell(0)
        assert isinstance(c, IRef)
        assert isinstance(c, IDeref)


# =========================================================================
# Var basics
# =========================================================================

class TestVarBasics:
    def setup_method(self):
        # Use a unique namespace per test class — shared registry across tests.
        self.ns = Namespace.find_or_create(Symbol.intern("test.var.basics"))

    def test_anonymous_var(self):
        v = Var.create(42)
        assert v.deref() == 42

    def test_anonymous_var_unbound_default(self):
        v = Var.create()
        assert not v.has_root()
        with pytest.raises(RuntimeError):
            v()

    def test_intern_creates_var_in_ns(self):
        v = Var.intern(self.ns, Symbol.intern("a"), 10)
        assert v.ns is self.ns
        assert v.sym.name == "a"
        assert v.deref() == 10

    def test_intern_replaces_root_by_default(self):
        v1 = Var.intern(self.ns, Symbol.intern("b"), 1)
        v2 = Var.intern(self.ns, Symbol.intern("b"), 2)
        # Same Var instance (interned).
        assert v1 is v2
        assert v1.deref() == 2

    def test_intern_no_replace(self):
        v1 = Var.intern(self.ns, Symbol.intern("c"), 1)
        v2 = Var.intern(self.ns, Symbol.intern("c"), 99, False)
        assert v1.deref() == 1
        assert v2 is v1

    def test_var_as_function(self):
        class Doubler(AFn):
            def __call__(self, x):
                return x * 2
        v = Var.intern(self.ns, Symbol.intern("doubler"), Doubler())
        assert v(5) == 10

    def test_unbound_var_raises_when_called(self):
        v = Var.intern(self.ns, Symbol.intern("unbound"))
        v.unbind_root()
        with pytest.raises(RuntimeError):
            v()

    def test_var_str(self):
        v = Var.intern(self.ns, Symbol.intern("printer"), 0)
        assert str(v).startswith("#'")

    def test_isinstance_iref_ifn_settable(self):
        v = Var.intern(self.ns, Symbol.intern("ifaces"), 0)
        assert isinstance(v, IFn)
        assert isinstance(v, IRef)
        assert isinstance(v, Settable)


# =========================================================================
# Var dynamic / thread bindings
# =========================================================================

class TestVarDynamic:
    def setup_method(self):
        self.ns = Namespace.find_or_create(Symbol.intern("test.var.dynamic"))

    def test_set_dynamic_returns_self(self):
        v = Var.intern(self.ns, Symbol.intern("d1"), 0)
        assert v.set_dynamic() is v
        assert v.is_dynamic()

    def test_push_pop_thread_bindings(self):
        v = Var.intern(self.ns, Symbol.intern("d2"), "default").set_dynamic()
        bindings = PersistentHashMap.create(v, "override")
        Var.push_thread_bindings(bindings)
        try:
            assert v.deref() == "override"
        finally:
            Var.pop_thread_bindings()
        assert v.deref() == "default"

    def test_push_non_dynamic_raises(self):
        v = Var.intern(self.ns, Symbol.intern("d3"), 0)  # NOT dynamic
        bindings = PersistentHashMap.create(v, 99)
        with pytest.raises(RuntimeError):
            Var.push_thread_bindings(bindings)

    def test_nested_bindings(self):
        v = Var.intern(self.ns, Symbol.intern("d4"), "level0").set_dynamic()
        Var.push_thread_bindings(PersistentHashMap.create(v, "level1"))
        try:
            assert v.deref() == "level1"
            Var.push_thread_bindings(PersistentHashMap.create(v, "level2"))
            try:
                assert v.deref() == "level2"
            finally:
                Var.pop_thread_bindings()
            assert v.deref() == "level1"
        finally:
            Var.pop_thread_bindings()
        assert v.deref() == "level0"

    def test_pop_without_push_raises(self):
        with pytest.raises(RuntimeError):
            Var.pop_thread_bindings()

    def test_set_within_binding(self):
        v = Var.intern(self.ns, Symbol.intern("d5"), 0).set_dynamic()
        Var.push_thread_bindings(PersistentHashMap.create(v, 1))
        try:
            v.set(99)
            assert v.deref() == 99
        finally:
            Var.pop_thread_bindings()
        # After pop, root is unchanged.
        assert v.deref() == 0

    def test_set_outside_binding_raises(self):
        v = Var.intern(self.ns, Symbol.intern("d6"), 0).set_dynamic()
        with pytest.raises(RuntimeError):
            v.set(99)

    def test_per_thread_isolation(self):
        v = Var.intern(self.ns, Symbol.intern("d7"), "main").set_dynamic()
        Var.push_thread_bindings(PersistentHashMap.create(v, "override"))
        try:
            other_thread_saw = []

            def worker():
                other_thread_saw.append(v.deref())

            t = threading.Thread(target=worker)
            t.start()
            t.join()

            # Other thread sees root value, NOT this thread's binding.
            assert other_thread_saw == ["main"]
            # Our thread still sees the binding.
            assert v.deref() == "override"
        finally:
            Var.pop_thread_bindings()


# =========================================================================
# Var meta
# =========================================================================

class TestVarMeta:
    def setup_method(self):
        self.ns = Namespace.find_or_create(Symbol.intern("test.var.meta"))

    def test_var_meta_seeded_with_name_and_ns(self):
        v = Var.intern(self.ns, Symbol.intern("m1"), 0)
        m = v.meta()
        assert m is not None
        assert m.val_at(Keyword.intern("name")).name == "m1"
        assert m.val_at(Keyword.intern("ns")) is self.ns

    def test_set_macro_marks_meta(self):
        v = Var.intern(self.ns, Symbol.intern("m2"), 0)
        v.set_macro()
        assert v.is_macro()

    def test_default_is_public(self):
        v = Var.intern(self.ns, Symbol.intern("m3"), 0)
        assert v.is_public()


# =========================================================================
# Namespace
# =========================================================================

class TestNamespace:
    def test_find_or_create(self):
        ns_name = Symbol.intern("test.ns.foo")
        ns1 = Namespace.find_or_create(ns_name)
        ns2 = Namespace.find_or_create(ns_name)
        assert ns1 is ns2

    def test_find_returns_none_when_absent(self):
        assert Namespace.find(Symbol.intern("does.not.exist.ever")) is None

    def test_intern_var_in_ns(self):
        ns = Namespace.find_or_create(Symbol.intern("test.ns.intern"))
        v = ns.intern(Symbol.intern("x"))
        assert v.ns is ns
        assert v.sym.name == "x"
        assert isinstance(v, Var)

    def test_intern_returns_existing(self):
        ns = Namespace.find_or_create(Symbol.intern("test.ns.intern2"))
        v1 = ns.intern(Symbol.intern("x"))
        v2 = ns.intern(Symbol.intern("x"))
        assert v1 is v2

    def test_intern_qualified_sym_raises(self):
        ns = Namespace.find_or_create(Symbol.intern("test.ns.q"))
        with pytest.raises(ValueError):
            ns.intern(Symbol.intern("other", "x"))

    def test_unmap(self):
        ns = Namespace.find_or_create(Symbol.intern("test.ns.unmap"))
        ns.intern(Symbol.intern("x"))
        ns.unmap(Symbol.intern("x"))
        assert ns.find_interned_var(Symbol.intern("x")) is None

    def test_find_interned_var(self):
        ns = Namespace.find_or_create(Symbol.intern("test.ns.find"))
        v = ns.intern(Symbol.intern("x"))
        assert ns.find_interned_var(Symbol.intern("x")) is v
        assert ns.find_interned_var(Symbol.intern("nonexistent")) is None

    def test_aliases(self):
        ns = Namespace.find_or_create(Symbol.intern("test.ns.alias.from"))
        target = Namespace.find_or_create(Symbol.intern("test.ns.alias.target"))
        alias_sym = Symbol.intern("t")
        ns.add_alias(alias_sym, target)
        assert ns.lookup_alias(alias_sym) is target

    def test_alias_conflict_raises(self):
        ns = Namespace.find_or_create(Symbol.intern("test.ns.alias.conflict"))
        a = Namespace.find_or_create(Symbol.intern("aliased.ns.a"))
        b = Namespace.find_or_create(Symbol.intern("aliased.ns.b"))
        ns.add_alias(Symbol.intern("x"), a)
        # Same alias, same target → no-op.
        ns.add_alias(Symbol.intern("x"), a)
        # Same alias, different target → error.
        with pytest.raises(RuntimeError):
            ns.add_alias(Symbol.intern("x"), b)

    def test_remove_alias(self):
        ns = Namespace.find_or_create(Symbol.intern("test.ns.alias.remove"))
        target = Namespace.find_or_create(Symbol.intern("aliased.target"))
        ns.add_alias(Symbol.intern("x"), target)
        ns.remove_alias(Symbol.intern("x"))
        assert ns.lookup_alias(Symbol.intern("x")) is None

    def test_remove(self):
        ns_name = Symbol.intern("test.ns.removable")
        Namespace.find_or_create(ns_name)
        Namespace.remove(ns_name)
        assert Namespace.find(ns_name) is None

    def test_all_returns_list(self):
        Namespace.find_or_create(Symbol.intern("test.ns.allcheck"))
        names = [n.name for n in Namespace.all()]
        assert Symbol.intern("test.ns.allcheck") in names

    def test_str(self):
        ns = Namespace.find_or_create(Symbol.intern("printable.ns"))
        assert str(ns) == "printable.ns"


# =========================================================================
# Var.find for namespace-qualified symbols
# =========================================================================

class TestVarFind:
    def test_find_by_qualified_symbol(self):
        ns_name = Symbol.intern("test.var.find")
        ns = Namespace.find_or_create(ns_name)
        v = Var.intern(ns, Symbol.intern("x"), 42)
        # Find via qualified sym
        found = Var.find(Symbol.intern("test.var.find", "x"))
        assert found is v

    def test_find_unqualified_raises(self):
        with pytest.raises(ValueError):
            Var.find(Symbol.intern("just-a-name"))
