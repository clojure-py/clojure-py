"""Tests for MapEntry, PersistentArrayMap, TransientArrayMap."""
import pytest

from clojure.lang import (
    PersistentArrayMap, PERSISTENT_ARRAY_MAP_EMPTY, TransientArrayMap,
    MapEntry, Keyword, Symbol,
    PersistentVector,
    Reduced,
    IPersistentMap, Associative, ILookup, IPersistentCollection,
    Counted, IFn, IHashEq, IMeta, IObj, IMapEntry, Indexed, Sequential,
    IKVReduce, IDrop, IEditableCollection,
    ITransientMap, ITransientAssociative, ITransientCollection,
    Murmur3,
)


# =========================================================================
# MapEntry
# =========================================================================

class TestMapEntry:
    def test_construction(self):
        e = MapEntry("a", 1)
        assert e.key() == "a"
        assert e.val() == 1

    def test_get_key_get_value_aliases(self):
        e = MapEntry("a", 1)
        assert e.get_key() == "a"
        assert e.get_value() == 1

    def test_indexing(self):
        e = MapEntry("a", 1)
        assert e[0] == "a"
        assert e[1] == 1
        assert e[-1] == 1
        assert e[-2] == "a"

    def test_nth(self):
        e = MapEntry("a", 1)
        assert e.nth(0) == "a"
        assert e.nth(1) == 1
        assert e.nth(99, "default") == "default"
        with pytest.raises(IndexError):
            e.nth(99)

    def test_count(self):
        assert MapEntry("a", 1).count() == 2
        assert len(MapEntry("a", 1)) == 2

    def test_iteration(self):
        e = MapEntry("a", 1)
        assert list(e) == ["a", 1]

    def test_call_like_a_vector(self):
        e = MapEntry("a", 1)
        assert e(0) == "a"
        assert e(1) == 1

    def test_str(self):
        assert str(MapEntry("a", 1)) == '["a" 1]'

    def test_equality_with_other_map_entry(self):
        assert MapEntry("a", 1) == MapEntry("a", 1)
        assert MapEntry("a", 1) != MapEntry("a", 2)
        assert MapEntry("a", 1) != MapEntry("b", 1)

    def test_equality_with_list_or_tuple(self):
        # Two-element vector / tuple / list compare equal.
        assert MapEntry("a", 1) == ["a", 1]
        assert MapEntry("a", 1) == ("a", 1)
        assert MapEntry("a", 1) != ["a", 1, "extra"]

    def test_equality_with_persistent_vector(self):
        assert MapEntry("a", 1) == PersistentVector.create("a", 1)

    def test_hash_matches_apersistentvector_formula(self):
        # Java APersistentVector.hashCode for [k, v] = 31 * (31 + h(k)) + h(v)
        # with None → 0.
        e = MapEntry("x", 5)
        expected = 31 * (31 + hash("x")) + hash(5)
        # Mask to int32 like our impl does.
        m = expected & 0xFFFFFFFF
        if m >= 0x80000000:
            m -= 0x100000000
        assert hash(e) == m

    def test_hasheq_matches_murmur3_hash_ordered(self):
        e = MapEntry("a", 1)
        assert e.hasheq() == Murmur3.hash_ordered(["a", 1])

    def test_meta(self):
        e = MapEntry("a", 1)
        assert e.meta() is None
        e2 = e.with_meta({"line": 5})
        assert e2.meta() == {"line": 5}
        assert e == e2  # meta doesn't affect equality

    def test_seq_walks_pair(self):
        e = MapEntry("a", 1)
        s = e.seq()
        assert s.first() == "a"
        assert s.next().first() == 1
        assert s.next().next() is None
        assert list(s) == ["a", 1]

    def test_isinstance_imapentry(self):
        e = MapEntry("a", 1)
        assert isinstance(e, IMapEntry)
        assert isinstance(e, Indexed)
        assert isinstance(e, Counted)
        assert isinstance(e, Sequential)
        assert isinstance(e, IFn)


# =========================================================================
# PersistentArrayMap — basics
# =========================================================================

class TestArrayMapConstruction:
    def test_create_kv_pairs(self):
        m = PersistentArrayMap.create("a", 1, "b", 2)
        assert m.count() == 2

    def test_create_from_dict(self):
        m = PersistentArrayMap.create({"a": 1, "b": 2})
        assert m.count() == 2
        assert m.val_at("a") == 1

    def test_create_no_args(self):
        m = PersistentArrayMap.create()
        assert m.count() == 0

    def test_create_odd_args_raises(self):
        with pytest.raises(ValueError):
            PersistentArrayMap.create("a", 1, "b")

    def test_create_with_check_rejects_duplicates(self):
        with pytest.raises(ValueError):
            PersistentArrayMap.create_with_check(["a", 1, "a", 2])

    def test_create_as_if_by_assoc_dedups(self):
        m = PersistentArrayMap.create_as_if_by_assoc(["a", 1, "a", 2])
        assert m.val_at("a") == 2
        assert m.count() == 1


class TestArrayMapBasics:
    def test_count(self):
        m = PersistentArrayMap.create("a", 1, "b", 2, "c", 3)
        assert m.count() == 3
        assert len(m) == 3

    def test_val_at_present(self):
        m = PersistentArrayMap.create("a", 1, "b", 2)
        assert m.val_at("a") == 1
        assert m.val_at("b") == 2

    def test_val_at_missing_returns_none(self):
        assert PersistentArrayMap.create("a", 1).val_at("z") is None

    def test_val_at_with_default(self):
        assert PersistentArrayMap.create("a", 1).val_at("z", "X") == "X"

    def test_val_at_distinguish_nil_from_missing(self):
        # If a value happens to be None, val_at("k") returns None — same as missing.
        # Use entry_at to disambiguate.
        m = PersistentArrayMap.create("nil-val", None)
        assert m.val_at("nil-val") is None  # ambiguous
        assert m.entry_at("nil-val") is not None  # but we know it's there

    def test_contains_key(self):
        m = PersistentArrayMap.create("a", 1)
        assert m.contains_key("a")
        assert not m.contains_key("z")

    def test_entry_at_returns_map_entry(self):
        e = PersistentArrayMap.create("a", 1).entry_at("a")
        assert isinstance(e, MapEntry)
        assert e.key() == "a" and e.val() == 1

    def test_entry_at_missing_returns_none(self):
        assert PersistentArrayMap.create("a", 1).entry_at("z") is None


class TestArrayMapAssoc:
    def test_assoc_new_key(self):
        m = PersistentArrayMap.create("a", 1)
        m2 = m.assoc("b", 2)
        assert m2.val_at("a") == 1
        assert m2.val_at("b") == 2
        assert m.val_at("b") is None  # original unchanged

    def test_assoc_existing_key_replaces(self):
        m = PersistentArrayMap.create("a", 1)
        assert m.assoc("a", 99).val_at("a") == 99

    def test_assoc_same_value_returns_self(self):
        m = PersistentArrayMap.create("a", 1)
        assert m.assoc("a", 1) is m

    def test_assoc_ex_new_key_succeeds(self):
        m = PersistentArrayMap.create("a", 1)
        assert m.assoc_ex("b", 2).val_at("b") == 2

    def test_assoc_ex_existing_key_raises(self):
        m = PersistentArrayMap.create("a", 1)
        with pytest.raises(ValueError):
            m.assoc_ex("a", 2)

    def test_assoc_grows_past_threshold(self):
        # JVM spillovers to PersistentHashMap at 16 entries — we let array grow
        # without spillover until PHM lands. Verify correctness, not perf.
        m = PERSISTENT_ARRAY_MAP_EMPTY
        for i in range(50):
            m = m.assoc(i, i * 10)
        assert m.count() == 50
        assert m.val_at(25) == 250


class TestArrayMapWithout:
    def test_without_present_key(self):
        m = PersistentArrayMap.create("a", 1, "b", 2, "c", 3)
        m2 = m.without("b")
        assert m2.count() == 2
        assert m2.val_at("b") is None
        assert m2.val_at("a") == 1

    def test_without_missing_key_returns_self(self):
        m = PersistentArrayMap.create("a", 1)
        assert m.without("z") is m

    def test_without_last_key_returns_empty(self):
        m = PersistentArrayMap.create("a", 1)
        assert m.without("a").count() == 0


class TestArrayMapCons:
    def test_cons_map_entry(self):
        m = PersistentArrayMap.create("a", 1)
        m2 = m.cons(MapEntry("b", 2))
        assert m2.val_at("b") == 2

    def test_cons_pair_tuple(self):
        m = PersistentArrayMap.create("a", 1)
        m2 = m.cons(("b", 2))
        assert m2.val_at("b") == 2

    def test_cons_pair_list(self):
        m2 = PersistentArrayMap.create().cons(["x", 99])
        assert m2.val_at("x") == 99

    def test_cons_pair_vector(self):
        m2 = PersistentArrayMap.create().cons(PersistentVector.create("x", 99))
        assert m2.val_at("x") == 99

    def test_cons_seq_of_entries(self):
        # A 2-element list is treated as a (key, val) pair (vector-shaped),
        # so to add multiple entries use a longer list or a generator.
        m = PersistentArrayMap.create()
        m2 = m.cons([("a", 1), ("b", 2), ("c", 3)])
        assert m2.count() == 3
        assert m2.val_at("a") == 1
        assert m2.val_at("c") == 3

    def test_cons_two_element_list_is_a_pair_not_a_seq(self):
        # Document the boundary: [k v] (two elements) is a pair, NOT a seq of
        # two entries. To add multiple entries via a 2-element collection,
        # wrap in a generator.
        m = PersistentArrayMap.create()
        m2 = m.cons(["a", 1])
        assert m2.val_at("a") == 1
        assert m2.count() == 1

    def test_cons_none_returns_self(self):
        m = PersistentArrayMap.create("a", 1)
        assert m.cons(None) is m


class TestArrayMapEmpty:
    def test_empty_returns_empty(self):
        assert PersistentArrayMap.create("a", 1).empty().count() == 0

    def test_empty_no_meta_is_singleton(self):
        assert PersistentArrayMap.create("a", 1).empty() is PERSISTENT_ARRAY_MAP_EMPTY

    def test_empty_propagates_meta(self):
        m = PersistentArrayMap.create("a", 1).with_meta({"k": "v"})
        e = m.empty()
        assert e.meta() == {"k": "v"}


class TestArrayMapEquality:
    def test_equal_to_other_array_map(self):
        a = PersistentArrayMap.create("a", 1, "b", 2)
        b = PersistentArrayMap.create("a", 1, "b", 2)
        assert a == b

    def test_equal_ignores_entry_order(self):
        # Maps are unordered for equality.
        a = PersistentArrayMap.create("a", 1, "b", 2)
        b = PersistentArrayMap.create("b", 2, "a", 1)
        assert a == b

    def test_equal_to_python_dict(self):
        m = PersistentArrayMap.create("a", 1, "b", 2)
        assert m == {"a": 1, "b": 2}

    def test_different_size_not_equal(self):
        assert PersistentArrayMap.create("a", 1) != PersistentArrayMap.create("a", 1, "b", 2)

    def test_different_value_not_equal(self):
        a = PersistentArrayMap.create("a", 1)
        b = PersistentArrayMap.create("a", 2)
        assert a != b


class TestArrayMapHash:
    def test_hashable_in_set(self):
        m1 = PersistentArrayMap.create("a", 1)
        m2 = PersistentArrayMap.create("a", 1)
        assert {m1, m2} == {m1}

    def test_hash_independent_of_entry_order(self):
        a = PersistentArrayMap.create("a", 1, "b", 2)
        b = PersistentArrayMap.create("b", 2, "a", 1)
        # JVM AbstractMap.hashCode is sum-of-entry-hashes — order-independent.
        assert hash(a) == hash(b)

    def test_hasheq_matches_murmur3_unordered(self):
        m = PersistentArrayMap.create("a", 1, "b", 2)
        # Build the entry list ourselves to check.
        entries = [MapEntry("a", 1), MapEntry("b", 2)]
        assert m.hasheq() == Murmur3.hash_unordered(entries)


class TestArrayMapKeywordFastPath:
    def test_keyword_keys_use_identity(self):
        k = Keyword.intern("name")
        m = PersistentArrayMap.create(k, "Alice")
        assert m.val_at(k) == "Alice"
        # Fresh-interned keyword (same name) → same identity.
        assert m.val_at(Keyword.intern("name")) == "Alice"


class TestArrayMapIteration:
    def test_iter_yields_map_entries(self):
        m = PersistentArrayMap.create("a", 1, "b", 2)
        entries = list(m)
        assert all(isinstance(e, MapEntry) for e in entries)
        assert {(e.key(), e.val()) for e in entries} == {("a", 1), ("b", 2)}

    def test_keys(self):
        m = PersistentArrayMap.create("a", 1, "b", 2)
        assert set(m.keys()) == {"a", "b"}

    def test_values(self):
        m = PersistentArrayMap.create("a", 1, "b", 2)
        assert set(m.values()) == {1, 2}

    def test_in_operator_checks_key(self):
        m = PersistentArrayMap.create("a", 1)
        assert "a" in m
        assert "z" not in m

    def test_getitem_raises_for_missing(self):
        m = PersistentArrayMap.create("a", 1)
        assert m["a"] == 1
        with pytest.raises(KeyError):
            m["z"]

    def test_callable(self):
        m = PersistentArrayMap.create("a", 1)
        assert m("a") == 1
        assert m("z") is None
        assert m("z", "default") == "default"


class TestArrayMapSeq:
    def test_seq_walks_entries(self):
        m = PersistentArrayMap.create("a", 1, "b", 2)
        s = m.seq()
        assert s.count() == 2
        assert s.first() == MapEntry("a", 1)
        assert s.next().first() == MapEntry("b", 2)
        assert s.next().next() is None

    def test_empty_seq_is_none(self):
        assert PERSISTENT_ARRAY_MAP_EMPTY.seq() is None


class TestArrayMapKVReduce:
    def test_kv_reduce_sum_values(self):
        m = PersistentArrayMap.create("a", 1, "b", 2, "c", 3)
        result = m.kv_reduce(lambda acc, k, v: acc + v, 0)
        assert result == 6

    def test_kv_reduce_short_circuit(self):
        def f(acc, k, v):
            if v == 2:
                return Reduced("STOP")
            return acc + v
        m = PersistentArrayMap.create("a", 1, "b", 2, "c", 3)
        assert m.kv_reduce(f, 0) == "STOP"


class TestArrayMapDrop:
    def test_drop(self):
        m = PersistentArrayMap.create("a", 1, "b", 2, "c", 3)
        s = m.drop(1)
        # Two entries remain.
        assert s.count() == 2

    def test_drop_all(self):
        m = PersistentArrayMap.create("a", 1)
        assert m.drop(5) is None


class TestArrayMapMeta:
    def test_default_meta(self):
        assert PersistentArrayMap.create("a", 1).meta() is None

    def test_with_meta(self):
        m = PersistentArrayMap.create("a", 1)
        m2 = m.with_meta({"line": 5})
        assert m2.meta() == {"line": 5}
        assert m.meta() is None


class TestArrayMapStr:
    def test_str_keys_values(self):
        m = PersistentArrayMap.create("a", 1, "b", 2)
        # Order-preserving for ArrayMap (insertion order).
        assert str(m) == '{"a" 1, "b" 2}'

    def test_empty_str(self):
        assert str(PERSISTENT_ARRAY_MAP_EMPTY) == "{}"


class TestArrayMapInterfaces:
    def test_isinstance_ipersistent_map(self):
        m = PersistentArrayMap.create("a", 1)
        assert isinstance(m, IPersistentMap)
        assert isinstance(m, Associative)
        assert isinstance(m, ILookup)
        assert isinstance(m, Counted)
        assert isinstance(m, IFn)
        assert isinstance(m, IKVReduce)
        assert isinstance(m, IDrop)
        assert isinstance(m, IEditableCollection)


# =========================================================================
# TransientArrayMap
# =========================================================================

class TestTransientArrayMap:
    def test_basic_assoc(self):
        t = PersistentArrayMap.create().as_transient()
        for i in range(5):
            t.assoc(f"k{i}", i)
        assert t.persistent().count() == 5

    def test_assoc_replaces(self):
        t = PersistentArrayMap.create("a", 1).as_transient()
        t.assoc("a", 99)
        assert t.persistent().val_at("a") == 99

    def test_without(self):
        t = PersistentArrayMap.create("a", 1, "b", 2).as_transient()
        t.without("a")
        result = t.persistent()
        assert result.count() == 1
        assert result.val_at("a") is None

    def test_val_at(self):
        t = PersistentArrayMap.create("a", 1).as_transient()
        assert t.val_at("a") == 1

    def test_contains_key(self):
        t = PersistentArrayMap.create("a", 1).as_transient()
        assert t.contains_key("a")
        assert not t.contains_key("z")

    def test_entry_at(self):
        t = PersistentArrayMap.create("a", 1).as_transient()
        e = t.entry_at("a")
        assert e.key() == "a" and e.val() == 1

    def test_count(self):
        t = PersistentArrayMap.create("a", 1, "b", 2).as_transient()
        assert t.count() == 2
        t.assoc("c", 3)
        assert t.count() == 3

    def test_callable(self):
        t = PersistentArrayMap.create("a", 1).as_transient()
        assert t("a") == 1
        assert t("z", "default") == "default"

    def test_use_after_persistent_raises(self):
        t = PersistentArrayMap.create().as_transient()
        t.assoc("a", 1)
        t.persistent()
        with pytest.raises(RuntimeError):
            t.assoc("b", 2)

    def test_isinstance_transient_interfaces(self):
        t = PersistentArrayMap.create().as_transient()
        assert isinstance(t, ITransientMap)
        assert isinstance(t, ITransientAssociative)
        assert isinstance(t, ITransientCollection)

    def test_conj_pair(self):
        t = PersistentArrayMap.create().as_transient()
        t.conj(("a", 1))
        t.conj(MapEntry("b", 2))
        result = t.persistent()
        assert result.val_at("a") == 1
        assert result.val_at("b") == 2

    def test_grows_past_threshold(self):
        # Same as persistent: we let transient grow without HT spillover.
        t = PersistentArrayMap.create().as_transient()
        for i in range(50):
            t.assoc(i, i * 10)
        result = t.persistent()
        assert result.count() == 50
        assert result.val_at(30) == 300
