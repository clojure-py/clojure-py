"""Tests for core.clj batch 5 (lines 746-1070):

delay/force/delay?, if-not, identical?, =, not=, compare, and, or,
zero?, count, int, nth, <, inc/inc', reduce1, reverse, +/+',
*/*', /, -/-', <=
"""

import pytest

import clojure.core  # triggers load

from clojure.lang import (
    Compiler,
    read_string,
    Var, Symbol, Keyword,
    PersistentVector, PersistentList, Ratio, BigInt,
)


def E(src):
    return Compiler.eval(read_string(src))


# --- delay/force/delay? -----------------------------------------------

def test_delay_returns_delay_obj():
    d = E("(clojure.core/delay 42)")
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb5-d-obj"), d)
    assert E("(clojure.core/delay? user/tcb5-d-obj)") is True
    assert E("(clojure.core/delay? 42)") is False

def test_delay_lazy_evaluates_once():
    counter = []
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb5-bump!"),
               lambda: (counter.append(1), len(counter))[1])
    d = E("(clojure.core/delay (user/tcb5-bump!))")
    assert counter == []  # not yet forced
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb5-d"), d)
    v1 = E("(clojure.core/force user/tcb5-d)")
    v2 = E("(clojure.core/force user/tcb5-d)")
    assert v1 == 1
    assert v2 == 1  # cached, not re-evaluated
    assert counter == [1]

def test_force_passes_through_non_delay():
    assert E("(clojure.core/force 99)") == 99
    assert E("(clojure.core/force nil)") is None


# --- if-not -----------------------------------------------------------

def test_if_not_basic():
    assert E("(clojure.core/if-not false :y :n)") == Keyword.intern(None, "y")
    assert E("(clojure.core/if-not true :y :n)") == Keyword.intern(None, "n")

def test_if_not_no_else_is_nil_when_truthy():
    assert E("(clojure.core/if-not true :y)") is None

def test_if_not_no_else_is_then_when_falsy():
    assert E("(clojure.core/if-not nil :y)") == Keyword.intern(None, "y")


# --- identical? -------------------------------------------------------

def test_identical_same_object():
    assert E("(clojure.core/identical? :a :a)") is True

def test_identical_different_objects():
    assert E("(clojure.core/identical? [1] [1])") is False


# --- = and not= -------------------------------------------------------

def test_eq_two_args():
    assert E("(clojure.core/= 1 1)") is True
    assert E("(clojure.core/= 1 2)") is False

def test_eq_value_equality_for_collections():
    assert E("(clojure.core/= [1 2] [1 2])") is True
    assert E("(clojure.core/= '(1 2) '(1 2))") is True

def test_eq_variadic_all_equal():
    assert E("(clojure.core/= 1 1 1 1)") is True

def test_eq_variadic_one_different():
    assert E("(clojure.core/= 1 1 1 2)") is False

def test_eq_single_arg_is_true():
    assert E("(clojure.core/= 99)") is True

def test_not_eq():
    assert E("(clojure.core/not= 1 2)") is True
    assert E("(clojure.core/not= 1 1)") is False
    assert E("(clojure.core/not= 1 1 1)") is False
    assert E("(clojure.core/not= 1 1 2)") is True


# --- compare ----------------------------------------------------------

def test_compare_numbers():
    assert E("(clojure.core/compare 1 2)") < 0
    assert E("(clojure.core/compare 2 1)") > 0
    assert E("(clojure.core/compare 1 1)") == 0

def test_compare_nil_first():
    assert E("(clojure.core/compare nil 1)") < 0
    assert E("(clojure.core/compare 1 nil)") > 0
    assert E("(clojure.core/compare nil nil)") == 0


# --- and / or macros --------------------------------------------------

def test_and_no_args_is_true():
    assert E("(clojure.core/and)") is True

def test_and_returns_last_when_all_truthy():
    assert E("(clojure.core/and 1 2 3)") == 3

def test_and_short_circuits_on_false():
    assert E("(clojure.core/and 1 false 3)") is False

def test_and_short_circuits_on_nil():
    assert E("(clojure.core/and 1 nil 3)") is None

def test_or_no_args_is_nil():
    assert E("(clojure.core/or)") is None

def test_or_returns_first_truthy():
    assert E("(clojure.core/or false nil :got)") == Keyword.intern(None, "got")

def test_or_returns_last_when_all_falsy():
    assert E("(clojure.core/or false nil)") is None


# --- zero? / count / int / nth ----------------------------------------

def test_zero_p():
    assert E("(clojure.core/zero? 0)") is True
    assert E("(clojure.core/zero? 0.0)") is True
    assert E("(clojure.core/zero? 1)") is False

def test_count():
    assert E("(clojure.core/count [1 2 3])") == 3
    assert E("(clojure.core/count nil)") == 0
    assert E('(clojure.core/count "hello")') == 5

def test_int_coerce():
    assert E("(clojure.core/int 3.7)") == 3
    assert E("(clojure.core/int 5)") == 5

def test_nth_in_range():
    assert E("(clojure.core/nth [10 20 30] 1)") == 20

def test_nth_out_of_range_raises():
    with pytest.raises(IndexError):
        E("(clojure.core/nth [1 2 3] 99)")

def test_nth_with_not_found():
    assert E("(clojure.core/nth [1 2 3] 99 :nf)") == Keyword.intern(None, "nf")


# --- < and <= ---------------------------------------------------------

def test_lt_two_args():
    assert E("(clojure.core/< 1 2)") is True
    assert E("(clojure.core/< 2 1)") is False
    assert E("(clojure.core/< 1 1)") is False

def test_lt_variadic_monotonic():
    assert E("(clojure.core/< 1 2 3 4)") is True
    assert E("(clojure.core/< 1 2 3 2)") is False

def test_lt_single_arg_is_true():
    assert E("(clojure.core/< 99)") is True

def test_lte_two_args():
    assert E("(clojure.core/<= 1 1)") is True
    assert E("(clojure.core/<= 1 2)") is True
    assert E("(clojure.core/<= 2 1)") is False

def test_lte_variadic():
    assert E("(clojure.core/<= 1 1 2 3)") is True
    assert E("(clojure.core/<= 1 1 0)") is False


# --- inc / inc' -------------------------------------------------------

def test_inc():
    assert E("(clojure.core/inc 5)") == 6
    assert E("(clojure.core/inc -1)") == 0

def test_inc_p():
    assert E("(clojure.core/inc' 5)") == 6
    # Python ints don't overflow so inc' behaves like inc
    big = E("(clojure.core/inc' 9999999999999999999)")
    assert big == 10000000000000000000


# --- arithmetic -------------------------------------------------------

def test_plus_no_args():
    assert E("(clojure.core/+)") == 0

def test_plus_single_arg():
    assert E("(clojure.core/+ 42)") == 42

def test_plus_two_args():
    assert E("(clojure.core/+ 10 32)") == 42

def test_plus_variadic():
    assert E("(clojure.core/+ 1 2 3 4 5)") == 15

def test_minus_negation():
    assert E("(clojure.core/- 7)") == -7

def test_minus_two_args():
    assert E("(clojure.core/- 10 3)") == 7

def test_minus_variadic():
    assert E("(clojure.core/- 100 10 20 30)") == 40

def test_mul_empty():
    assert E("(clojure.core/*)") == 1

def test_mul_variadic():
    assert E("(clojure.core/* 2 3 4)") == 24

def test_div_two_ints_returns_ratio():
    r = E("(clojure.core// 10 4)")
    assert isinstance(r, Ratio)
    assert r == Ratio(5, 2)

def test_div_clean_integer():
    assert E("(clojure.core// 10 5)") == 2

def test_div_inverse():
    r = E("(clojure.core// 4)")
    assert r == Ratio(1, 4)


# --- reverse (uses reduce1 + conj) ------------------------------------

def test_reverse_vector():
    r = E("(clojure.core/reverse [1 2 3])")
    assert list(r) == [3, 2, 1]

def test_reverse_list():
    r = E("(clojure.core/reverse '(:a :b :c))")
    assert list(r) == [
        Keyword.intern(None, "c"),
        Keyword.intern(None, "b"),
        Keyword.intern(None, "a"),
    ]

def test_reverse_empty():
    r = E("(clojure.core/reverse [])")
    assert r is None or len(list(r)) == 0
