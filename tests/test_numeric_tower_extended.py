"""Numeric-tower tests — Ratio (fractions.Fraction), BigDecimal
(decimal.Decimal), BigInt (alias to Python int), division semantics,
width casts, and the full set of numeric predicates."""

from decimal import Decimal
from fractions import Fraction

import pytest
from clojure._core import eval_string


def _ev(src):
    return eval_string(src)


# --- Division: int/int returns Fraction (vanilla behavior) ---

def test_div_int_int_returns_fraction():
    r = _ev("(/ 1 2)")
    assert r == Fraction(1, 2)
    assert isinstance(r, Fraction)


def test_div_int_int_exact_returns_int():
    # If the quotient is a whole number, return int (not a Fraction 2/1).
    r = _ev("(/ 4 2)")
    assert r == 2
    assert isinstance(r, int) and not isinstance(r, Fraction)


def test_div_normalizes_fraction():
    assert _ev("(/ 6 4)") == Fraction(3, 2)


def test_div_preserves_float_path():
    # float operand short-circuits to Python's float division.
    assert _ev("(/ 1 2.0)") == 0.5


def test_div_by_zero():
    with pytest.raises(ZeroDivisionError):
        _ev("(/ 1 0)")


def test_div_negative_int():
    assert _ev("(/ -3 4)") == Fraction(-3, 4)


def test_div_single_arg_reciprocal():
    # (/ x) → 1/x
    assert _ev("(/ 4)") == Fraction(1, 4)
    assert _ev("(/ 2.0)") == 0.5


def test_div_multi_arg():
    # (/ 12 2 3) → ((12/2)/3) = 2
    assert _ev("(/ 12 2 3)") == 2


# --- Predicates ---

def test_ratio_pred():
    assert _ev("(ratio? (/ 1 2))") is True
    assert _ev("(ratio? 1)") is False
    assert _ev("(ratio? 1.0)") is False


def test_decimal_pred():
    assert _ev("(decimal? (bigdec 1))") is True
    assert _ev("(decimal? 1)") is False


def test_float_pred():
    assert _ev("(float? 3.14)") is True
    assert _ev("(float? 1)") is False
    assert _ev("(float? (/ 1 2))") is False


def test_rational_pred():
    assert _ev("(rational? 1)") is True
    assert _ev("(rational? (/ 1 2))") is True
    assert _ev("(rational? (bigdec 1))") is True
    assert _ev("(rational? 3.14)") is False  # Float isn't exact.
    assert _ev("(rational? :x)") is False


def test_number_pred_includes_fraction_and_decimal():
    assert _ev("(number? 1)") is True
    assert _ev("(number? 3.14)") is True
    assert _ev("(number? (/ 1 2))") is True
    assert _ev("(number? (bigdec 1))") is True
    assert _ev("(number? true)") is False
    assert _ev("(number? :x)") is False


def test_integer_pred_unchanged():
    # integer? is the existing pre-tower predicate.
    assert _ev("(integer? 1)") is True
    assert _ev("(integer? 3.14)") is False
    assert _ev("(integer? (/ 1 2))") is False


# --- Constructors ---

def test_bigint_from_string():
    assert _ev('(bigint "123456789012345678901234567890")') == 123456789012345678901234567890


def test_bigint_from_float():
    # Truncates toward zero (Python int(float) semantics).
    assert _ev("(bigint 3.9)") == 3
    assert _ev("(bigint -3.9)") == -3


def test_biginteger_equiv_bigint():
    assert _ev("(biginteger 42)") == _ev("(bigint 42)")


def test_bigdec_from_string():
    assert _ev('(bigdec "3.14159265358979323846")') == Decimal("3.14159265358979323846")


def test_bigdec_from_float_is_exact_str():
    # Going through str() avoids the float-bit-pattern artifacts.
    assert _ev("(bigdec 3.14)") == Decimal("3.14")


def test_bigdec_from_int():
    assert _ev("(bigdec 42)") == Decimal(42)


def test_rationalize_float():
    # 0.25 has an exact float representation.
    assert _ev("(rationalize 0.25)") == Fraction(1, 4)


def test_rationalize_int():
    assert _ev("(rationalize 42)") == 42


def test_rationalize_fraction_passthrough():
    # Already rational → unchanged.
    r = _ev("(rationalize (/ 3 4))")
    assert r == Fraction(3, 4)


# --- numerator / denominator ---

def test_numerator_denominator_fraction():
    assert _ev("(numerator (/ 3 4))") == 3
    assert _ev("(denominator (/ 3 4))") == 4


def test_numerator_denominator_int():
    assert _ev("(numerator 42)") == 42
    assert _ev("(denominator 42)") == 1


# --- Arithmetic through the tower ---

def test_fraction_plus_int():
    assert _ev("(+ (/ 1 2) 1)") == Fraction(3, 2)


def test_fraction_times_int_reduces_to_int():
    # 1/2 * 2 = 1 — Fraction's own __mul__ normalizes.
    assert _ev("(* (/ 1 2) 2)") == 1


def test_fraction_minus_fraction():
    assert _ev("(- (/ 3 4) (/ 1 4))") == Fraction(1, 2)


def test_mixed_int_float_fraction():
    # Float contaminates → result is float.
    r = _ev("(+ 1.0 (/ 1 2))")
    assert r == 1.5


def test_decimal_arithmetic():
    assert _ev("(+ (bigdec 1) (bigdec 2))") == Decimal(3)


# --- Width casts ---

def test_long_short_byte_char_alias():
    for form in ("(long 3.7)", "(short 3.7)", "(byte 3.7)", "(char 3.7)"):
        assert _ev(form) == 3


def test_double_and_float_are_float_coerce():
    # Fully-qualified — `double` is sometimes redefined in clojure.user
    # by other tests (notably test_compose), which would shadow core's.
    assert _ev("(clojure.core/double 5)") == 5.0
    assert _ev("(clojure.core/float 5)") == 5.0
    assert _ev("(clojure.core/double (/ 1 2))") == 0.5


def test_unchecked_variants_equal_checked():
    assert _ev("(clojure.core/unchecked-long 42)") == _ev("(clojure.core/long 42)")
    assert _ev("(clojure.core/unchecked-int 42)") == _ev("(clojure.core/int 42)")
    assert _ev("(clojure.core/unchecked-double 1)") == _ev("(clojure.core/double 1)")
