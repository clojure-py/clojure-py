"""Tests for core.clj batch 6 (lines 1071-1572):

>, >=, ==, max, min, abs, dec/dec',
unchecked-* family,
pos?, neg?, quot, rem, rationalize,
bit-not/and/or/xor/and-not/clear/set/flip/test,
bit-shift-left/right, unsigned-bit-shift-right,
integer?, even?, odd?, int?, pos-int?, neg-int?, nat-int?, double?,
complement, constantly, identity,
peek, pop,
map-entry?, contains?, get, dissoc, disj, find, select-keys
"""

import pytest

import clojure.core  # triggers load

from clojure.lang import (
    Compiler,
    read_string,
    Var, Symbol, Keyword,
    PersistentVector, PersistentList,
    Ratio, BigInt, MapEntry,
)


def E(src):
    return Compiler.eval(read_string(src))


def K(name):
    return Keyword.intern(None, name)


# --- > >= == --------------------------------------------------------

def test_gt_two_args():
    assert E("(clojure.core/> 2 1)") is True
    assert E("(clojure.core/> 1 2)") is False

def test_gt_variadic():
    assert E("(clojure.core/> 5 4 3 2 1)") is True
    assert E("(clojure.core/> 5 4 3 5 1)") is False

def test_gte_variadic():
    assert E("(clojure.core/>= 5 5 4 3)") is True
    assert E("(clojure.core/>= 5 4 5)") is False

def test_eq_eq_numeric_equiv():
    assert E("(clojure.core/== 1 1.0)") is True
    assert E("(clojure.core/== 1 1 1)") is True
    assert E("(clojure.core/== 1 2)") is False


# --- max/min/abs ---------------------------------------------------

def test_max():
    assert E("(clojure.core/max 1 2 3)") == 3
    assert E("(clojure.core/max -5 -1)") == -1
    assert E("(clojure.core/max 99)") == 99

def test_min():
    assert E("(clojure.core/min 1 2 3)") == 1
    assert E("(clojure.core/min -5 -1)") == -5

def test_abs():
    assert E("(clojure.core/abs -7)") == 7
    assert E("(clojure.core/abs 7)") == 7
    assert E("(clojure.core/abs -3.5)") == 3.5


# --- dec / dec' ----------------------------------------------------

def test_dec():
    assert E("(clojure.core/dec 5)") == 4
    assert E("(clojure.core/dec 0)") == -1


# --- unchecked-* (Python ints don't overflow → same as checked) -----

def test_unchecked_inc():
    assert E("(clojure.core/unchecked-inc 5)") == 6
    assert E("(clojure.core/unchecked-inc-int 5)") == 6

def test_unchecked_dec():
    assert E("(clojure.core/unchecked-dec 5)") == 4

def test_unchecked_add():
    assert E("(clojure.core/unchecked-add 10 32)") == 42
    assert E("(clojure.core/unchecked-add-int 1 2)") == 3

def test_unchecked_subtract():
    assert E("(clojure.core/unchecked-subtract 10 3)") == 7

def test_unchecked_multiply():
    assert E("(clojure.core/unchecked-multiply 6 7)") == 42

def test_unchecked_negate():
    assert E("(clojure.core/unchecked-negate 5)") == -5


# --- pos? / neg? / zero? -------------------------------------------

def test_pos_p():
    assert E("(clojure.core/pos? 1)") is True
    assert E("(clojure.core/pos? 0)") is False
    assert E("(clojure.core/pos? -1)") is False

def test_neg_p():
    assert E("(clojure.core/neg? -1)") is True
    assert E("(clojure.core/neg? 0)") is False


# --- quot / rem / rationalize --------------------------------------

def test_quot():
    assert E("(clojure.core/quot 7 3)") == 2
    assert E("(clojure.core/quot -7 3)") == -2  # truncate toward zero

def test_rem():
    assert E("(clojure.core/rem 7 3)") == 1

def test_rationalize_int():
    assert E("(clojure.core/rationalize 5)") == 5

def test_rationalize_float():
    r = E("(clojure.core/rationalize 0.5)")
    assert r == Ratio(1, 2)

def test_rationalize_already_ratio():
    r = E("(clojure.core/rationalize 3/4)")
    assert r == Ratio(3, 4)


# --- bit ops -------------------------------------------------------

def test_bit_not():
    assert E("(clojure.core/bit-not 0)") == -1
    assert E("(clojure.core/bit-not 5)") == -6

def test_bit_and():
    assert E("(clojure.core/bit-and 12 10)") == 8
    assert E("(clojure.core/bit-and 0xff 0x0f)") == 0x0f

def test_bit_or():
    assert E("(clojure.core/bit-or 12 10)") == 14
    assert E("(clojure.core/bit-or 1 2 4 8)") == 15

def test_bit_xor():
    assert E("(clojure.core/bit-xor 12 10)") == 6

def test_bit_and_not():
    assert E("(clojure.core/bit-and-not 12 10)") == 4

def test_bit_clear_set_flip_test():
    assert E("(clojure.core/bit-set 0 3)") == 8
    assert E("(clojure.core/bit-clear 15 1)") == 13
    assert E("(clojure.core/bit-flip 0 4)") == 16
    assert E("(clojure.core/bit-test 8 3)") is True
    assert E("(clojure.core/bit-test 8 0)") is False

def test_bit_shifts():
    assert E("(clojure.core/bit-shift-left 1 8)") == 256
    assert E("(clojure.core/bit-shift-right 256 4)") == 16
    assert E("(clojure.core/unsigned-bit-shift-right 16 1)") == 8


# --- type predicates -----------------------------------------------

def test_integer_p():
    assert E("(clojure.core/integer? 1)") is True
    assert E("(clojure.core/integer? 1.0)") is False
    assert E("(clojure.core/integer? 1/2)") is False
    assert E('(clojure.core/integer? "x")') is False

def test_even_p():
    assert E("(clojure.core/even? 0)") is True
    assert E("(clojure.core/even? 4)") is True
    assert E("(clojure.core/even? 3)") is False

def test_odd_p():
    assert E("(clojure.core/odd? 3)") is True
    assert E("(clojure.core/odd? 4)") is False

def test_even_p_non_integer_raises():
    with pytest.raises(ValueError):
        E("(clojure.core/even? 1.5)")

def test_int_p():
    assert E("(clojure.core/int? 1)") is True
    assert E("(clojure.core/int? 1.0)") is False
    assert E("(clojure.core/int? 1/2)") is False

def test_pos_int_neg_int_nat_int():
    assert E("(clojure.core/pos-int? 5)") is True
    assert E("(clojure.core/pos-int? 0)") is False
    assert E("(clojure.core/pos-int? -1)") is False
    assert E("(clojure.core/neg-int? -1)") is True
    assert E("(clojure.core/neg-int? 0)") is False
    assert E("(clojure.core/nat-int? 0)") is True
    assert E("(clojure.core/nat-int? 5)") is True
    assert E("(clojure.core/nat-int? -1)") is False

def test_double_p():
    assert E("(clojure.core/double? 1.5)") is True
    assert E("(clojure.core/double? 1)") is False


# --- complement / constantly / identity -----------------------------

def test_complement_zero_arity():
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb6-truefn"),
               lambda: True)
    E("(def tcb6-cf (clojure.core/complement user/tcb6-truefn))")
    assert E("(tcb6-cf)") is False

def test_complement_one_arity():
    E("(def tcb6-czero (clojure.core/complement clojure.core/zero?))")
    assert E("(tcb6-czero 0)") is False
    assert E("(tcb6-czero 5)") is True

def test_complement_variadic():
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb6-allge"),
               lambda *args: all(a >= 0 for a in args))
    E("(def tcb6-someneg (clojure.core/complement user/tcb6-allge))")
    assert E("(tcb6-someneg 1 2 -3 4)") is True
    assert E("(tcb6-someneg 1 2 3 4)") is False

def test_constantly():
    E("(def tcb6-c42 (clojure.core/constantly 42))")
    assert E("(tcb6-c42)") == 42
    assert E("(tcb6-c42 :anything)") == 42
    assert E("(tcb6-c42 1 2)") == 42
    assert E("(tcb6-c42 1 2 3 4 5)") == 42

def test_identity():
    assert E("(clojure.core/identity 99)") == 99
    assert E("(clojure.core/identity nil)") is None
    assert list(E("(clojure.core/identity [1 2 3])")) == [1, 2, 3]


# --- peek / pop ----------------------------------------------------

def test_peek_vector():
    """For vectors, peek returns the LAST item."""
    assert E("(clojure.core/peek [1 2 3])") == 3

def test_peek_list():
    """For lists, peek returns the FIRST item."""
    assert E("(clojure.core/peek '(1 2 3))") == 1

def test_peek_nil():
    assert E("(clojure.core/peek nil)") is None

def test_pop_vector_drops_last():
    v = E("(clojure.core/pop [1 2 3])")
    assert list(v) == [1, 2]

def test_pop_list_drops_first():
    s = E("(clojure.core/pop '(1 2 3))")
    assert list(s) == [2, 3]


# --- map ops -------------------------------------------------------

def test_contains_p_map():
    assert E("(clojure.core/contains? {:a 1} :a)") is True
    assert E("(clojure.core/contains? {:a 1} :b)") is False

def test_contains_p_vector_index():
    assert E("(clojure.core/contains? [10 20 30] 1)") is True
    assert E("(clojure.core/contains? [10 20 30] 99)") is False

def test_contains_p_set():
    assert E("(clojure.core/contains? #{1 2 3} 2)") is True
    assert E("(clojure.core/contains? #{1 2 3} 99)") is False

def test_get_present_key():
    assert E("(clojure.core/get {:a 1 :b 2} :a)") == 1

def test_get_missing_key_returns_nil():
    assert E("(clojure.core/get {:a 1} :z)") is None

def test_get_with_default():
    assert E("(clojure.core/get {:a 1} :z :default)") == K("default")

def test_get_on_vector_by_index():
    assert E("(clojure.core/get [10 20 30] 1)") == 20

def test_dissoc_single_key():
    m = E("(clojure.core/dissoc {:a 1 :b 2} :a)")
    assert m.val_at(K("b")) == 2
    assert m.val_at(K("a")) is None

def test_dissoc_multiple_keys():
    m = E("(clojure.core/dissoc {:a 1 :b 2 :c 3} :a :c)")
    assert m.count() == 1
    assert m.val_at(K("b")) == 2

def test_dissoc_no_keys_returns_input():
    """0-arg form returns the map unchanged."""
    m = E("(clojure.core/dissoc {:a 1})")
    assert m.val_at(K("a")) == 1

def test_disj_single():
    s = E("(clojure.core/disj #{1 2 3} 2)")
    assert s.count() == 2
    assert s.contains(1)
    assert not s.contains(2)

def test_disj_multiple():
    s = E("(clojure.core/disj #{1 2 3 4} 1 3)")
    assert s.count() == 2
    assert s.contains(2)
    assert s.contains(4)

def test_disj_nil_returns_nil():
    assert E("(clojure.core/disj nil :anything)") is None

def test_find_present():
    e = E("(clojure.core/find {:a 1 :b 2} :a)")
    assert isinstance(e, MapEntry)
    assert e.key() == K("a")
    assert e.val() == 1

def test_find_missing():
    assert E("(clojure.core/find {:a 1} :z)") is None

def test_select_keys_subset():
    m = E("(clojure.core/select-keys {:a 1 :b 2 :c 3} [:a :c])")
    assert m.val_at(K("a")) == 1
    assert m.val_at(K("c")) == 3
    assert m.count() == 2

def test_select_keys_missing_skipped():
    m = E("(clojure.core/select-keys {:a 1} [:a :z])")
    assert m.count() == 1
    assert m.val_at(K("a")) == 1


# --- map-entry? ----------------------------------------------------

def test_map_entry_p():
    """Find returns a MapEntry."""
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb6-e"),
               MapEntry(K("k"), 99))
    assert E("(clojure.core/map-entry? user/tcb6-e)") is True
    assert E("(clojure.core/map-entry? [1 2])") is False
