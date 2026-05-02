"""Tests for PersistentHashMap and TransientHashMap (HAMT)."""
import pytest

from clojure.lang import (
    PersistentHashMap, PERSISTENT_HASH_MAP_EMPTY, TransientHashMap,
    PersistentArrayMap,
    MapEntry, Keyword, Symbol,
    Reduced,
    IPersistentMap, Associative, ILookup, IPersistentCollection,
    Counted, IFn, IHashEq, IMeta, IObj,
    IKVReduce, IEditableCollection,
    ITransientMap, ITransientAssociative, ITransientCollection,
    Murmur3,
)


# =========================================================================
# Construction
# =========================================================================

class TestConstruction:
    def test_create_kv_pairs(self):
        m = PersistentHashMap.create("a", 1, "b", 2)
        assert m.count() == 2
        assert m.val_at("a") == 1

    def test_create_from_dict(self):
        m = PersistentHashMap.create({"a": 1, "b": 2})
        assert m.count() == 2

    def test_create_no_args(self):
        m = PersistentHashMap.create()
        assert m.count() == 0

    def test_create_odd_args_raises(self):
        with pytest.raises(ValueError):
            PersistentHashMap.create("a", 1, "b")

    def test_create_with_check_rejects_duplicates(self):
        with pytest.raises(ValueError):
            PersistentHashMap.create_with_check("a", 1, "a", 2)

    def test_from_iterable_of_pairs(self):
        m = PersistentHashMap.from_iterable([("a", 1), ("b", 2)])
        assert m.count() == 2
        assert m.val_at("a") == 1

    def test_from_iterable_of_map_entries(self):
        m = PersistentHashMap.from_iterable([MapEntry("a", 1), MapEntry("b", 2)])
        assert m.count() == 2


# =========================================================================
# Basic operations — small map (single BIN)
# =========================================================================

class TestBasics:
    def test_count(self):
        m = PersistentHashMap.create("a", 1, "b", 2, "c", 3)
        assert m.count() == 3
        assert len(m) == 3

    def test_val_at_present(self):
        m = PersistentHashMap.create("a", 1)
        assert m.val_at("a") == 1

    def test_val_at_missing_returns_none(self):
        assert PersistentHashMap.create("a", 1).val_at("z") is None

    def test_val_at_with_default(self):
        assert PersistentHashMap.create("a", 1).val_at("z", "X") == "X"

    def test_contains_key(self):
        m = PersistentHashMap.create("a", 1)
        assert m.contains_key("a")
        assert not m.contains_key("z")

    def test_entry_at(self):
        e = PersistentHashMap.create("a", 1).entry_at("a")
        assert isinstance(e, MapEntry)
        assert e.key() == "a" and e.val() == 1


class TestAssoc:
    def test_assoc_new(self):
        m = PersistentHashMap.create("a", 1)
        m2 = m.assoc("b", 2)
        assert m2.count() == 2
        assert m2.val_at("b") == 2
        assert m.val_at("b") is None  # original unchanged

    def test_assoc_replace(self):
        m = PersistentHashMap.create("a", 1)
        assert m.assoc("a", 99).val_at("a") == 99
        assert m.assoc("a", 99).count() == 1

    def test_assoc_same_value_returns_self(self):
        m = PersistentHashMap.create("a", 1)
        assert m.assoc("a", 1) is m

    def test_assoc_ex_new_succeeds(self):
        assert PersistentHashMap.create().assoc_ex("a", 1).val_at("a") == 1

    def test_assoc_ex_existing_raises(self):
        m = PersistentHashMap.create("a", 1)
        with pytest.raises(ValueError):
            m.assoc_ex("a", 99)


class TestWithout:
    def test_without_present(self):
        m = PersistentHashMap.create("a", 1, "b", 2)
        m2 = m.without("a")
        assert m2.count() == 1
        assert not m2.contains_key("a")

    def test_without_missing_returns_self(self):
        m = PersistentHashMap.create("a", 1)
        assert m.without("z") is m


# =========================================================================
# Null key handling
# =========================================================================

class TestNullKey:
    def test_assoc_null_key(self):
        m = PERSISTENT_HASH_MAP_EMPTY.assoc(None, "NIL")
        assert m.val_at(None) == "NIL"
        assert m.contains_key(None)
        assert m.count() == 1

    def test_assoc_null_key_replaces(self):
        m = PERSISTENT_HASH_MAP_EMPTY.assoc(None, "X").assoc(None, "Y")
        assert m.val_at(None) == "Y"
        assert m.count() == 1

    def test_without_null_key(self):
        m = PERSISTENT_HASH_MAP_EMPTY.assoc(None, "X").assoc("a", 1)
        m2 = m.without(None)
        assert not m2.contains_key(None)
        assert m2.count() == 1

    def test_seq_yields_null_entry_first(self):
        m = PERSISTENT_HASH_MAP_EMPTY.assoc(None, "X").assoc("a", 1)
        first_entry = m.seq().first()
        assert first_entry.key() is None
        assert first_entry.val() == "X"


# =========================================================================
# Larger map — exercise BIN → ArrayNode promotion
# =========================================================================

class TestLarge:
    def test_thousand_keys(self):
        m = PERSISTENT_HASH_MAP_EMPTY
        for i in range(1000):
            m = m.assoc(i, i * 10)
        assert m.count() == 1000
        for i in range(0, 1000, 47):
            assert m.val_at(i) == i * 10

    def test_remove_through_trie(self):
        m = PERSISTENT_HASH_MAP_EMPTY
        for i in range(100):
            m = m.assoc(i, i)
        for i in range(0, 100, 2):
            m = m.without(i)
        assert m.count() == 50
        for i in range(100):
            if i % 2 == 1:
                assert m.val_at(i) == i
            else:
                assert not m.contains_key(i)

    def test_string_keys(self):
        m = PERSISTENT_HASH_MAP_EMPTY
        keys = [f"k{i}" for i in range(200)]
        for k in keys:
            m = m.assoc(k, k.upper())
        assert m.count() == 200
        for k in keys:
            assert m.val_at(k) == k.upper()

    def test_keyword_keys(self):
        m = PERSISTENT_HASH_MAP_EMPTY
        for i in range(100):
            m = m.assoc(Keyword.intern(f"k{i}"), i)
        assert m.val_at(Keyword.intern("k50")) == 50

    def test_persistence_survives_mutation(self):
        m1 = PERSISTENT_HASH_MAP_EMPTY
        for i in range(50):
            m1 = m1.assoc(i, i)
        m2 = m1
        for i in range(50, 100):
            m2 = m2.assoc(i, i)
        assert m1.count() == 50  # m1 unchanged
        assert m2.count() == 100


# =========================================================================
# Hash collisions — same hash, different keys
# =========================================================================

class _SameHash:
    """Keys whose hasheq always returns 0 — forces HashCollisionNode usage."""
    def __init__(self, name):
        self.name = name

    def hasheq(self):
        return 0

    def __hash__(self):
        return 0

    def __eq__(self, other):
        return isinstance(other, _SameHash) and self.name == other.name

    def __ne__(self, other):
        return not self.__eq__(other)


class TestHashCollisions:
    def test_collisions_stored_correctly(self):
        from clojure.lang import IHashEq
        IHashEq.register(_SameHash)
        a = _SameHash("a")
        b = _SameHash("b")
        c = _SameHash("c")
        m = PERSISTENT_HASH_MAP_EMPTY.assoc(a, 1).assoc(b, 2).assoc(c, 3)
        assert m.count() == 3
        assert m.val_at(a) == 1
        assert m.val_at(b) == 2
        assert m.val_at(c) == 3

    def test_collisions_can_be_removed(self):
        from clojure.lang import IHashEq
        IHashEq.register(_SameHash)
        a = _SameHash("a")
        b = _SameHash("b")
        m = PERSISTENT_HASH_MAP_EMPTY.assoc(a, 1).assoc(b, 2)
        m2 = m.without(a)
        assert m2.count() == 1
        assert m2.val_at(b) == 2
        assert not m2.contains_key(a)


# =========================================================================
# Equality / hash
# =========================================================================

class TestEquality:
    def test_equal_to_other_phm(self):
        a = PersistentHashMap.create("a", 1, "b", 2)
        b = PersistentHashMap.create("b", 2, "a", 1)
        assert a == b

    def test_equal_to_pam(self):
        # PHM == PAM with same entries.
        phm = PersistentHashMap.create("a", 1, "b", 2)
        pam = PersistentArrayMap.create("a", 1, "b", 2)
        assert phm == pam

    def test_equal_to_dict(self):
        m = PersistentHashMap.create("a", 1, "b", 2)
        assert m == {"a": 1, "b": 2}

    def test_different_size_not_equal(self):
        assert PersistentHashMap.create("a", 1) != PersistentHashMap.create("a", 1, "b", 2)


class TestHash:
    def test_hashable(self):
        a = PersistentHashMap.create("a", 1, "b", 2)
        b = PersistentHashMap.create("b", 2, "a", 1)
        assert hash(a) == hash(b)  # entry-order-independent

    def test_hasheq_unordered(self):
        m = PersistentHashMap.create("a", 1, "b", 2)
        # Build expected by walking entries
        entries = list(m)
        assert m.hasheq() == Murmur3.hash_unordered(entries)


# =========================================================================
# cons
# =========================================================================

class TestCons:
    def test_cons_map_entry(self):
        m = PersistentHashMap.create("a", 1).cons(MapEntry("b", 2))
        assert m.count() == 2
        assert m.val_at("b") == 2

    def test_cons_pair_tuple(self):
        m = PersistentHashMap.create().cons(("a", 1))
        assert m.val_at("a") == 1

    def test_cons_seq_of_pairs(self):
        m = PersistentHashMap.create().cons([("a", 1), ("b", 2), ("c", 3)])
        assert m.count() == 3


# =========================================================================
# Iteration / Python protocols
# =========================================================================

class TestIteration:
    def test_iter_yields_map_entries(self):
        m = PersistentHashMap.create("a", 1, "b", 2)
        entries = list(m)
        assert all(isinstance(e, MapEntry) for e in entries)
        kv_set = {(e.key(), e.val()) for e in entries}
        assert kv_set == {("a", 1), ("b", 2)}

    def test_keys(self):
        m = PersistentHashMap.create("a", 1, "b", 2)
        assert set(m.keys()) == {"a", "b"}

    def test_values(self):
        m = PersistentHashMap.create("a", 1, "b", 2)
        assert set(m.values()) == {1, 2}

    def test_in_operator_checks_key(self):
        m = PersistentHashMap.create("a", 1)
        assert "a" in m
        assert "z" not in m

    def test_getitem_raises_for_missing(self):
        m = PersistentHashMap.create("a", 1)
        assert m["a"] == 1
        with pytest.raises(KeyError):
            m["z"]

    def test_callable(self):
        m = PersistentHashMap.create("a", 1)
        assert m("a") == 1
        assert m("z", "default") == "default"


# =========================================================================
# kv_reduce
# =========================================================================

class TestKVReduce:
    def test_kv_reduce_sum_values(self):
        m = PERSISTENT_HASH_MAP_EMPTY
        for i in range(10):
            m = m.assoc(i, i)
        result = m.kv_reduce(lambda acc, k, v: acc + v, 0)
        assert result == sum(range(10))

    def test_kv_reduce_short_circuit(self):
        m = PERSISTENT_HASH_MAP_EMPTY.assoc("a", 1).assoc("b", 2)

        def f(acc, k, v):
            if v == 1:
                return Reduced("STOP")
            return acc + v
        # The order is unordered, but at some point we'll see v=1.
        result = m.kv_reduce(f, 0)
        # Either short-circuited at "a" or after seeing "b" first then hitting "a".
        assert result == "STOP" or result == 2 or result == "STOP"

    def test_kv_reduce_with_null_key(self):
        m = PERSISTENT_HASH_MAP_EMPTY.assoc(None, "NIL").assoc("a", 1)
        keys_seen = []
        m.kv_reduce(lambda acc, k, v: keys_seen.append(k) or acc, None)
        assert None in keys_seen


# =========================================================================
# Empty / meta
# =========================================================================

class TestEmpty:
    def test_empty_no_meta_returns_singleton(self):
        m = PersistentHashMap.create("a", 1)
        assert m.empty() is PERSISTENT_HASH_MAP_EMPTY

    def test_empty_propagates_meta(self):
        m = PersistentHashMap.create("a", 1).with_meta({"k": "v"})
        e = m.empty()
        assert e.meta() == {"k": "v"}


class TestMeta:
    def test_default_meta_none(self):
        assert PersistentHashMap.create("a", 1).meta() is None

    def test_with_meta(self):
        m = PersistentHashMap.create("a", 1)
        m2 = m.with_meta({"line": 5})
        assert m2.meta() == {"line": 5}
        assert m == m2  # meta-independent equality


# =========================================================================
# ABC registration
# =========================================================================

class TestInterfaces:
    def test_isinstance_ipersistentmap(self):
        m = PersistentHashMap.create("a", 1)
        assert isinstance(m, IPersistentMap)
        assert isinstance(m, Associative)
        assert isinstance(m, ILookup)
        assert isinstance(m, Counted)
        assert isinstance(m, IFn)
        assert isinstance(m, IKVReduce)
        assert isinstance(m, IEditableCollection)


# =========================================================================
# TransientHashMap
# =========================================================================

class TestTransient:
    def test_basic(self):
        t = PERSISTENT_HASH_MAP_EMPTY.as_transient()
        for i in range(100):
            t.assoc(f"k{i}", i)
        result = t.persistent()
        assert result.count() == 100
        assert result.val_at("k50") == 50

    def test_assoc_replace(self):
        t = PersistentHashMap.create("a", 1).as_transient()
        t.assoc("a", 99)
        assert t.persistent().val_at("a") == 99

    def test_without(self):
        t = PERSISTENT_HASH_MAP_EMPTY
        for i in range(10):
            t = t.assoc(i, i)
        t = t.as_transient()
        for i in range(0, 10, 2):
            t.without(i)
        result = t.persistent()
        assert result.count() == 5

    def test_null_key(self):
        t = PERSISTENT_HASH_MAP_EMPTY.as_transient()
        t.assoc(None, "NIL")
        assert t.val_at(None) == "NIL"
        assert t.contains_key(None)

    def test_use_after_persistent_raises(self):
        t = PERSISTENT_HASH_MAP_EMPTY.as_transient()
        t.assoc("a", 1)
        t.persistent()
        with pytest.raises(RuntimeError):
            t.assoc("b", 2)

    def test_callable(self):
        t = PersistentHashMap.create("a", 1).as_transient()
        assert t("a") == 1

    def test_isinstance_transient(self):
        t = PERSISTENT_HASH_MAP_EMPTY.as_transient()
        assert isinstance(t, ITransientMap)
        assert isinstance(t, ITransientAssociative)
        assert isinstance(t, ITransientCollection)


# =========================================================================
# PersistentArrayMap → PersistentHashMap spillover
# =========================================================================

class TestSpillover:
    def test_pam_assoc_past_threshold_returns_phm(self):
        m = PersistentArrayMap.create()
        for i in range(10):
            m = m.assoc(i, i)
        # Past the 16-array-slot (8-pair) threshold the implementation switches
        # to PersistentHashMap.
        assert isinstance(m, PersistentHashMap)
        assert m.count() == 10
        assert m.val_at(5) == 5

    def test_pam_below_threshold_stays_pam(self):
        m = PersistentArrayMap.create()
        for i in range(5):
            m = m.assoc(i, i)
        assert isinstance(m, PersistentArrayMap)

    def test_pam_assoc_ex_past_threshold(self):
        m = PersistentArrayMap.create()
        for i in range(10):
            m = m.assoc_ex(i, i)
        assert isinstance(m, PersistentHashMap)
        assert m.count() == 10

    def test_transient_pam_spillover(self):
        t = PersistentArrayMap.create().as_transient()
        for i in range(20):
            t = t.assoc(i, i)
        assert isinstance(t, TransientHashMap)
        result = t.persistent()
        assert result.count() == 20
