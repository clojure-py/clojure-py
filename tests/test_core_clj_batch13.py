"""Tests for core.clj batch 13 (lines 3353-3453): editable collections.

Forms (8):
  transient, persistent!,
  conj!, assoc!, dissoc!, pop!, disj!,
  into1 (private redef of into with transient batch support)
"""

import pytest

import clojure.core  # triggers load

from clojure.lang import (
    Compiler,
    read_string,
    Keyword,
    ITransientCollection,
    ITransientAssociative,
    ITransientMap,
    ITransientVector,
    ITransientSet,
    IEditableCollection,
    PersistentVector,
    PersistentArrayMap,
    PersistentHashMap,
    PersistentHashSet,
    PersistentList,
)


def E(src):
    return Compiler.eval(read_string(src))


def K(name):
    return Keyword.intern(None, name)


# --- transient -----------------------------------------------------

def test_transient_vector():
    t = E("(clojure.core/transient [1 2 3])")
    assert isinstance(t, ITransientVector)
    assert t.count() == 3

def test_transient_map():
    t = E("(clojure.core/transient {:a 1 :b 2})")
    assert isinstance(t, ITransientMap)

def test_transient_set():
    t = E("(clojure.core/transient #{:a :b})")
    assert isinstance(t, ITransientSet)

def test_transient_empty_vector():
    t = E("(clojure.core/transient [])")
    assert isinstance(t, ITransientVector)
    assert t.count() == 0

def test_transient_requires_editable_collection():
    """A non-IEditableCollection input should raise."""
    with pytest.raises((AttributeError, TypeError)):
        E("(clojure.core/transient (clojure.core/list 1 2 3))")


# --- persistent! ---------------------------------------------------

def test_persistent_bang_vector():
    out = E("(clojure.core/persistent! (clojure.core/transient [1 2 3]))")
    assert isinstance(out, PersistentVector)
    assert list(out) == [1, 2, 3]

def test_persistent_bang_map():
    out = E("(clojure.core/persistent! (clojure.core/transient {:a 1}))")
    assert dict(out) == {K("a"): 1}

def test_persistent_bang_set():
    out = E("(clojure.core/persistent! (clojure.core/transient #{1 2 3}))")
    assert set(out) == {1, 2, 3}

def test_persistent_bang_round_trip_preserves_contents():
    """transient → persistent! is an identity round-trip on data."""
    out = E("(clojure.core/persistent! (clojure.core/transient [1 2 3 4 5]))")
    assert list(out) == [1, 2, 3, 4, 5]


# --- conj! ---------------------------------------------------------

def test_conj_bang_zero_arg_returns_empty_transient_vector():
    out = E("(clojure.core/conj!)")
    assert isinstance(out, ITransientVector)
    assert out.count() == 0
    # And it persistents to an empty vector.
    assert list(E("(clojure.core/persistent! (clojure.core/conj!))")) == []

def test_conj_bang_one_arg_passthrough():
    """1-arity returns its argument unchanged."""
    out = E("(clojure.core/persistent! (clojure.core/conj! (clojure.core/transient [1 2])))")
    assert list(out) == [1, 2]

def test_conj_bang_two_arg_appends_to_vector():
    out = E("(clojure.core/persistent! (clojure.core/conj! (clojure.core/transient [1 2]) 3))")
    assert list(out) == [1, 2, 3]

def test_conj_bang_into_map_takes_pair():
    out = E("(clojure.core/persistent! (clojure.core/conj! (clojure.core/transient {}) [:a 1]))")
    assert dict(out) == {K("a"): 1}

def test_conj_bang_into_set():
    out = E("(clojure.core/persistent! (clojure.core/conj! (clojure.core/transient #{:a}) :b))")
    assert set(out) == {K("a"), K("b")}


# --- assoc! --------------------------------------------------------

def test_assoc_bang_map_single():
    out = E("(clojure.core/persistent! (clojure.core/assoc! (clojure.core/transient {}) :a 1))")
    assert dict(out) == {K("a"): 1}

def test_assoc_bang_map_overrides_existing():
    out = E("(clojure.core/persistent! (clojure.core/assoc! (clojure.core/transient {:a 1}) :a 99))")
    assert dict(out) == {K("a"): 99}

def test_assoc_bang_map_multi():
    out = E("(clojure.core/persistent! (clojure.core/assoc! (clojure.core/transient {}) :a 1 :b 2 :c 3))")
    assert dict(out) == {K("a"): 1, K("b"): 2, K("c"): 3}

def test_assoc_bang_vector_overwrites_index():
    out = E("(clojure.core/persistent! (clojure.core/assoc! (clojure.core/transient [10 20 30]) 1 99))")
    assert list(out) == [10, 99, 30]

def test_assoc_bang_vector_extend_at_count():
    """JVM: assoc! at index = count is allowed (acts as conj)."""
    out = E("(clojure.core/persistent! (clojure.core/assoc! (clojure.core/transient [1 2]) 2 3))")
    assert list(out) == [1, 2, 3]

def test_assoc_bang_vector_multi_pairs():
    out = E("(clojure.core/persistent! (clojure.core/assoc! (clojure.core/transient [0 0 0]) 0 :a 1 :b 2 :c))")
    assert list(out) == [K("a"), K("b"), K("c")]


# --- dissoc! -------------------------------------------------------

def test_dissoc_bang_single():
    out = E("(clojure.core/persistent! (clojure.core/dissoc! (clojure.core/transient {:a 1 :b 2}) :a))")
    assert dict(out) == {K("b"): 2}

def test_dissoc_bang_multi():
    out = E("(clojure.core/persistent! (clojure.core/dissoc! (clojure.core/transient {:a 1 :b 2 :c 3}) :a :b))")
    assert dict(out) == {K("c"): 3}

def test_dissoc_bang_missing_key_no_op():
    out = E("(clojure.core/persistent! (clojure.core/dissoc! (clojure.core/transient {:a 1}) :missing))")
    assert dict(out) == {K("a"): 1}


# --- pop! ----------------------------------------------------------

def test_pop_bang_basic():
    out = E("(clojure.core/persistent! (clojure.core/pop! (clojure.core/transient [1 2 3 4])))")
    assert list(out) == [1, 2, 3]

def test_pop_bang_to_empty():
    out = E("(clojure.core/persistent! (clojure.core/pop! (clojure.core/transient [1])))")
    assert list(out) == []

def test_pop_bang_empty_raises():
    """JVM: popping an empty transient vector throws."""
    with pytest.raises(Exception):
        E("(clojure.core/pop! (clojure.core/transient []))")


# --- disj! ---------------------------------------------------------

def test_disj_bang_single_arg_passthrough():
    """1-arity returns its argument unchanged (mirrors JVM)."""
    assert E("(clojure.core/disj! :unchanged)") == K("unchanged")

def test_disj_bang_removes_key():
    out = E("(clojure.core/persistent! (clojure.core/disj! (clojure.core/transient #{:a :b :c}) :b))")
    assert set(out) == {K("a"), K("c")}

def test_disj_bang_missing_key_no_op():
    out = E("(clojure.core/persistent! (clojure.core/disj! (clojure.core/transient #{:a}) :missing))")
    assert set(out) == {K("a")}

def test_disj_bang_multi():
    out = E("(clojure.core/persistent! (clojure.core/disj! (clojure.core/transient #{:a :b :c :d}) :b :d))")
    assert set(out) == {K("a"), K("c")}


# --- into1 ---------------------------------------------------------

def test_into1_vector_uses_transient_path():
    """to-coll is a vector (IEditableCollection) — should hit the
    transient-batched path, returning a persistent vector."""
    out = E("(clojure.core/into1 [1 2] (clojure.core/list 3 4 5))")
    assert isinstance(out, PersistentVector)
    assert list(out) == [1, 2, 3, 4, 5]

def test_into1_map_from_kv_pairs():
    out = E("(clojure.core/into1 {:a 1} [[:b 2] [:c 3]])")
    assert dict(out) == {K("a"): 1, K("b"): 2, K("c"): 3}

def test_into1_set():
    out = E("(clojure.core/into1 #{1 2} [3 4 5 1])")  # 1 is a dup
    assert set(out) == {1, 2, 3, 4, 5}

def test_into1_list_uses_non_transient_path():
    """Lists are not IEditableCollection — should fall back to (reduce1 conj).
    conj on a list prepends, so order is reversed from `from`."""
    out = E("(clojure.core/into1 () [1 2 3])")
    assert list(out) == [3, 2, 1]

def test_into1_empty_from():
    out = E("(clojure.core/into1 [1 2] [])")
    assert list(out) == [1, 2]

def test_into1_nil_from():
    """from = nil → no items conjoined; to is returned as-is."""
    out = E("(clojure.core/into1 [1 2] nil)")
    assert list(out) == [1, 2]


# --- end-to-end pattern (the canonical transient idiom) ------------

def test_transient_build_idiom():
    """Standard pattern: transient → many !-ops → persistent!. Verify
    the result equals what you'd get from persistent assocs."""
    out = E("""
      (clojure.core/persistent!
        (clojure.core/assoc!
          (clojure.core/assoc!
            (clojure.core/assoc! (clojure.core/transient {}) :a 1)
            :b 2)
          :c 3))
    """)
    assert dict(out) == {K("a"): 1, K("b"): 2, K("c"): 3}

def test_transient_build_via_reduce1():
    """into1 IS this pattern, factored out."""
    out = E("(clojure.core/into1 [] (clojure.core/range 5))")
    assert list(out) == [0, 1, 2, 3, 4]
