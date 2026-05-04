"""Tests for core.clj batch 3 (lines 339-538):

to-array, cast, vector, vec, hash-map/sorted-map(-by),
hash-set/sorted-set(-by), nil?, defmacro, when/when-not,
false?/true?/boolean?/not/some?
"""

import pytest

import clojure.core  # triggers load

from clojure.lang import (
    Compiler,
    read_string,
    Var, Symbol, Keyword,
    PersistentVector, PersistentArrayMap, PersistentHashMap,
    PersistentHashSet, PersistentTreeMap, PersistentTreeSet,
)


def E(src):
    return Compiler.eval(read_string(src))


# --- to-array ---------------------------------------------------------

def test_to_array_returns_python_list():
    assert E("(clojure.core/to-array [1 2 3])") == [1, 2, 3]

def test_to_array_on_seq():
    assert E("(clojure.core/to-array '(7 8 9))") == [7, 8, 9]

def test_to_array_on_nil():
    assert E("(clojure.core/to-array nil)") == []


# --- cast -------------------------------------------------------------

def test_cast_passes_through_when_isinstance():
    assert E("(clojure.core/cast Integer 42)") == 42

def test_cast_raises_on_mismatch():
    with pytest.raises(TypeError):
        E('(clojure.core/cast Integer "x")')

def test_cast_nil_passes_through():
    assert E("(clojure.core/cast Integer nil)") is None


# --- vector / vec -----------------------------------------------------

def test_vector_no_args_is_empty():
    v = E("(clojure.core/vector)")
    assert isinstance(v, PersistentVector)
    assert v.count() == 0

def test_vector_single_arg():
    v = E("(clojure.core/vector 99)")
    assert list(v) == [99]

def test_vector_six_args_optimized_path():
    v = E("(clojure.core/vector 1 2 3 4 5 6)")
    assert list(v) == [1, 2, 3, 4, 5, 6]

def test_vector_more_than_six_args_uses_lazy_path():
    v = E("(clojure.core/vector 1 2 3 4 5 6 7 8 9 10)")
    assert list(v) == [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]

def test_vec_from_seq():
    v = E("(clojure.core/vec '(1 2 3))")
    assert isinstance(v, PersistentVector)
    assert list(v) == [1, 2, 3]

def test_vec_from_existing_vector_strips_meta():
    """`vec` on an IObj vector returns it with-meta nil."""
    v = E("(clojure.core/vec [1 2 3])")
    assert list(v) == [1, 2, 3]


# --- hash-map / sorted-map -------------------------------------------

def test_hash_map_empty():
    m = E("(clojure.core/hash-map)")
    assert m.count() == 0

def test_hash_map_alternating_kv():
    m = E("(clojure.core/hash-map :a 1 :b 2 :c 3)")
    assert m.val_at(Keyword.intern(None, "a")) == 1
    assert m.val_at(Keyword.intern(None, "b")) == 2
    assert m.val_at(Keyword.intern(None, "c")) == 3

def test_sorted_map():
    m = E("(clojure.core/sorted-map :c 3 :a 1 :b 2)")
    assert isinstance(m, PersistentTreeMap)
    keys = [e.key().get_name() for e in m.seq()]
    assert keys == ["a", "b", "c"]

def test_sorted_map_by_with_comparator():
    """Reverse ordering via a comparator that returns -1 when a > b."""
    Var.intern(Compiler.current_ns(),
               Symbol.intern("rev-cmp"),
               lambda a, b: -1 if a.get_name() > b.get_name()
               else 1 if a.get_name() < b.get_name() else 0)
    m = E("(clojure.core/sorted-map-by user/rev-cmp :a 1 :c 3 :b 2)")
    keys = [e.key().get_name() for e in m.seq()]
    assert keys == ["c", "b", "a"]


# --- hash-set / sorted-set -------------------------------------------

def test_hash_set_empty():
    s = E("(clojure.core/hash-set)")
    assert s.count() == 0

def test_hash_set_with_args():
    s = E("(clojure.core/hash-set 1 2 3)")
    assert s.count() == 3
    for x in (1, 2, 3):
        assert s.contains(x)

def test_sorted_set():
    s = E("(clojure.core/sorted-set 3 1 2)")
    assert isinstance(s, PersistentTreeSet)
    assert list(s) == [1, 2, 3]

def test_sorted_set_by_with_comparator():
    Var.intern(Compiler.current_ns(),
               Symbol.intern("rev-num-cmp"),
               lambda a, b: -1 if a > b else 1 if a < b else 0)
    s = E("(clojure.core/sorted-set-by user/rev-num-cmp 1 3 2)")
    assert list(s) == [3, 2, 1]


# --- nil? -------------------------------------------------------------

def test_nil_true_for_nil():
    assert E("(clojure.core/nil? nil)") is True

def test_nil_false_for_anything_else():
    assert E("(clojure.core/nil? 0)") is False
    assert E("(clojure.core/nil? false)") is False
    assert E('(clojure.core/nil? "")') is False
    assert E("(clojure.core/nil? [])") is False


# --- defmacro --------------------------------------------------------

def test_defmacro_creates_macro():
    E("(clojure.core/defmacro tcb-id [x] x)")
    v = E("(var tcb-id)")
    assert v.is_macro()

def test_defmacro_with_implicit_args():
    """defmacro injects &form &env automatically."""
    E("(clojure.core/defmacro tcb-double [x] (clojure.core/list 'clojure.core/cons x nil))")
    # Expansion would give (cons N nil); evaluating gives a 1-elem list
    expanded = Compiler.macroexpand_1(read_string("(tcb-double 99)"))
    assert expanded == read_string("(clojure.core/cons 99 nil)")


# --- when / when-not -------------------------------------------------

def test_when_true_branch():
    assert E("(clojure.core/when true 1 2 3)") == 3

def test_when_false_branch_is_nil():
    assert E("(clojure.core/when false 1 2 3)") is None

def test_when_not_true_branch():
    assert E("(clojure.core/when-not false 1 2 3)") == 3

def test_when_not_false_branch_is_nil():
    assert E("(clojure.core/when-not true 1 2 3)") is None

def test_when_macroexpansion():
    assert Compiler.macroexpand_1(read_string("(clojure.core/when t a b)")) == \
        read_string("(if t (do a b))")


# --- predicates ------------------------------------------------------

def test_true_p():
    assert E("(clojure.core/true? true)") is True
    assert E("(clojure.core/true? false)") is False
    assert E("(clojure.core/true? 1)") is False
    assert E("(clojure.core/true? nil)") is False

def test_false_p():
    assert E("(clojure.core/false? false)") is True
    assert E("(clojure.core/false? true)") is False
    assert E("(clojure.core/false? 0)") is False
    assert E("(clojure.core/false? nil)") is False

def test_boolean_p():
    assert E("(clojure.core/boolean? true)") is True
    assert E("(clojure.core/boolean? false)") is True
    assert E("(clojure.core/boolean? 1)") is False
    assert E("(clojure.core/boolean? nil)") is False

def test_not():
    assert E("(clojure.core/not nil)") is True
    assert E("(clojure.core/not false)") is True
    assert E("(clojure.core/not 0)") is False
    assert E("(clojure.core/not true)") is False
    assert E('(clojure.core/not "")') is False

def test_some_p():
    assert E("(clojure.core/some? nil)") is False
    assert E("(clojure.core/some? false)") is True
    assert E("(clojure.core/some? 0)") is True
    assert E("(clojure.core/some? [])") is True
