"""Tests for the numeric tower ports (vanilla 868-1100)."""

import pytest
from clojure._core import eval_string


def _ev(src):
    return eval_string(src)


# --- Predicates ---

def test_zero_pos_neg():
    assert _ev("(zero? 0)") is True
    assert _ev("(zero? 1)") is False
    assert _ev("(pos? 5)") is True
    assert _ev("(pos? -3)") is False
    assert _ev("(pos? 0)") is False
    assert _ev("(neg? -3)") is True
    assert _ev("(neg? 0)") is False


# --- Variadic arithmetic ---

def test_plus_arities():
    assert _ev("(+)") == 0
    assert _ev("(+ 5)") == 5
    assert _ev("(+ 1 2)") == 3
    assert _ev("(+ 1 2 3 4)") == 10


def test_times_arities():
    assert _ev("(*)") == 1
    assert _ev("(* 3)") == 3
    assert _ev("(* 2 3 4)") == 24


def test_minus_arities():
    assert _ev("(- 5)") == -5
    assert _ev("(- 10 3)") == 7
    assert _ev("(- 10 3 2)") == 5


def test_divide_arities():
    assert _ev("(/ 4)") == 0.25
    assert _ev("(/ 10 2)") == 5
    assert _ev("(/ 12 2 3)") == 2


def test_inc_dec():
    assert _ev("(inc 5)") == 6
    assert _ev("(dec 5)") == 4


# --- Comparisons ---

def test_lt_monotonic():
    assert _ev("(< 1 2)") is True
    assert _ev("(< 1 2 3)") is True
    assert _ev("(< 1 3 2)") is False
    assert _ev("(< 5)") is True


def test_gt_monotonic():
    assert _ev("(> 3 2 1)") is True
    assert _ev("(> 3 1 2)") is False


def test_le_ge():
    assert _ev("(<= 1 1 2)") is True
    assert _ev("(<= 1 2 1)") is False
    assert _ev("(>= 2 1 1)") is True


def test_eq_numeric():
    assert _ev("(== 1 1.0)") is True
    assert _ev("(== 1 1 1)") is True
    assert _ev("(== 1 2)") is False


# --- quot / rem (truncating, NOT floor) ---

def test_quot_truncates_toward_zero():
    assert _ev("(quot 7 2)") == 3
    assert _ev("(quot -7 2)") == -3
    assert _ev("(quot 7 -2)") == -3
    assert _ev("(quot -7 -2)") == 3


def test_rem_matches_quot_sign():
    # Clojure: rem has the sign of the dividend.
    assert _ev("(rem 7 2)") == 1
    assert _ev("(rem -7 2)") == -1
    assert _ev("(rem 7 -2)") == 1
    assert _ev("(rem -7 -2)") == -1


# --- max, min, abs ---

def test_max_min():
    assert _ev("(max 5)") == 5
    assert _ev("(max 1 5 3)") == 5
    assert _ev("(min 4 1 3)") == 1


def test_abs():
    assert _ev("(abs -7)") == 7
    assert _ev("(abs 7)") == 7
    assert _ev("(abs 0)") == 0


# --- count / nth ---

def test_count_nil_and_coll():
    assert _ev("(count nil)") == 0
    assert _ev("(count [1 2 3])") == 3
    assert _ev("(count '(1 2 3 4))") == 4


def test_nth_vector_list_string():
    assert _ev("(nth [:a :b :c] 1)") == eval_string(":b")
    assert _ev("(nth '(10 20 30) 2)") == 30
    assert _ev("(nth [1 2 3] 5 :miss)") == eval_string(":miss")


def test_nth_out_of_bounds_raises_without_default():
    with pytest.raises(IndexError):
        _ev("(nth [1 2 3] 9)")


# --- reverse ---

def test_reverse_vector_and_list():
    assert list(_ev("(reverse [1 2 3 4])")) == [4, 3, 2, 1]
    assert list(_ev("(reverse '(:a :b :c))")) == [
        eval_string(":c"), eval_string(":b"), eval_string(":a"),
    ]


# --- Unchecked aliases ---

def test_unchecked_aliases_match_checked():
    assert _ev("(unchecked-inc 7)") == 8
    assert _ev("(unchecked-add 3 4)") == 7
    assert _ev("(unchecked-multiply 3 4)") == 12
    assert _ev("(unchecked-negate 7)") == -7


# --- Bit operations ---

def test_bitwise_and_or_xor():
    assert _ev("(bit-and 12 10)") == 8
    assert _ev("(bit-or 1 2 4 8)") == 15
    assert _ev("(bit-xor 10 6)") == 12


def test_bit_not():
    assert _ev("(bit-not 0)") == -1
    assert _ev("(bit-not 5)") == -6


def test_bit_shift():
    assert _ev("(bit-shift-left 1 5)") == 32
    assert _ev("(bit-shift-right 32 2)") == 8


def test_bit_individual_ops():
    assert _ev("(bit-set 0 3)") == 8
    assert _ev("(bit-clear 15 2)") == 11
    assert _ev("(bit-flip 5 0)") == 4
    assert _ev("(bit-test 8 3)") is True
    assert _ev("(bit-test 8 0)") is False


def test_bit_and_not():
    # x & ~y
    assert _ev("(bit-and-not 15 10)") == 5


def test_unsigned_shift_right_treats_as_u64():
    # On JVM, (unsigned-bit-shift-right -1 60) → 15 (top 4 bits of all-ones).
    assert _ev("(unsigned-bit-shift-right -1 60)") == 15
