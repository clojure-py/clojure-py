"""Tests for the arithmetic / predicate / bit-op surface of Numbers."""
from decimal import Decimal

import pytest

from clojure.lang import Numbers, Ratio, BigInt, BigDecimal


# ---- arithmetic ----

class TestAdd:
    def test_int_int(self):
        assert Numbers.add(2, 3) == 5

    def test_int_float(self):
        assert Numbers.add(1, 2.5) == 3.5

    def test_int_ratio(self):
        assert Numbers.add(1, Ratio(1, 2)) == Ratio(3, 2)

    def test_ratio_ratio(self):
        assert Numbers.add(Ratio(1, 2), Ratio(1, 3)) == Ratio(5, 6)

    def test_int_decimal(self):
        result = Numbers.add(1, Decimal("0.5"))
        assert Decimal.__eq__(result, Decimal("1.5"))

    def test_decimal_float_lossy(self):
        # Java BigDecimal+Double → Double (lossy). We match.
        result = Numbers.add(Decimal("0.1"), 0.1)
        assert isinstance(result, float)

    def test_bigint_tag_preserved(self):
        # Java: Long + BigInt → BigInt. Our Numbers.add preserves the tag.
        result = Numbers.add(BigInt(5), 1)
        assert isinstance(result, BigInt)
        assert result == BigInt(6)

    def test_bool_rejected(self):
        with pytest.raises(TypeError):
            Numbers.add(True, 1)


class TestMinus:
    def test_unary_int(self):
        assert Numbers.minus(5) == -5

    def test_unary_bigint_preserved(self):
        result = Numbers.minus(BigInt(5))
        assert isinstance(result, BigInt) and result == BigInt(-5)

    def test_binary(self):
        assert Numbers.minus(5, 3) == 2

    def test_binary_ratio(self):
        assert Numbers.minus(Ratio(3, 4), Ratio(1, 4)) == Ratio(1, 2)


class TestMultiply:
    def test_int(self):
        assert Numbers.multiply(2, 3) == 6

    def test_ratio(self):
        assert Numbers.multiply(Ratio(2, 3), Ratio(3, 2)) == 1

    def test_bigint_preserved(self):
        assert isinstance(Numbers.multiply(BigInt(2), 3), BigInt)


class TestDivide:
    def test_int_int_divisible(self):
        # Clojure: divisible int/int returns int, not Ratio.
        assert Numbers.divide(6, 2) == 3
        assert isinstance(Numbers.divide(6, 2), int)

    def test_int_int_not_divisible_returns_ratio(self):
        assert Numbers.divide(1, 3) == Ratio(1, 3)

    def test_int_int_reduces(self):
        assert Numbers.divide(6, 4) == Ratio(3, 2)

    def test_negative_normalization(self):
        # Result should have positive denominator.
        r = Numbers.divide(1, -3)
        assert isinstance(r, Ratio)
        assert r.denominator > 0

    def test_float_division(self):
        assert Numbers.divide(1.0, 2.0) == 0.5

    def test_int_zero_raises(self):
        with pytest.raises(ZeroDivisionError):
            Numbers.divide(1, 0)

    def test_float_zero_returns_inf(self):
        # Clojure: 1.0/0 → Infinity (matches IEEE).
        assert Numbers.divide(1.0, 0) == float("inf")
        assert Numbers.divide(-1.0, 0) == float("-inf")
        # 0.0/0 → NaN
        assert Numbers.divide(0.0, 0) != Numbers.divide(0.0, 0)  # NaN never equals itself


class TestQuotientAndRemainder:
    def test_quotient_truncates_toward_zero(self):
        # Clojure quot: trunc-toward-zero, NOT floor (which Python's // does).
        assert Numbers.quotient(7, 2) == 3
        assert Numbers.quotient(-7, 2) == -3   # NOT -4 (floor)
        assert Numbers.quotient(7, -2) == -3
        assert Numbers.quotient(-7, -2) == 3

    def test_remainder_sign_follows_dividend(self):
        # Java: x - quotient(x,y)*y. Sign follows dividend.
        assert Numbers.remainder(7, 2) == 1
        assert Numbers.remainder(-7, 2) == -1   # NOT 1 (Python's % gives 1)
        assert Numbers.remainder(7, -2) == 1
        assert Numbers.remainder(-7, -2) == -1


class TestIncDec:
    def test_inc_int(self):
        assert Numbers.inc(5) == 6

    def test_dec_int(self):
        assert Numbers.dec(5) == 4

    def test_inc_bigint_preserved(self):
        assert isinstance(Numbers.inc(BigInt(5)), BigInt)


class TestAbsAndNegate:
    def test_abs_int(self):
        assert Numbers.abs(-5) == 5

    def test_abs_ratio(self):
        assert Numbers.abs(Ratio(-1, 2)) == Ratio(1, 2)

    def test_negate_int(self):
        assert Numbers.negate(5) == -5

    def test_negate_bigint_preserved(self):
        assert isinstance(Numbers.negate(BigInt(5)), BigInt)


# ---- comparison ----

class TestComparisonOps:
    def test_lt_int(self):
        assert Numbers.lt(1, 2)
        assert not Numbers.lt(2, 1)
        assert not Numbers.lt(1, 1)

    def test_lte(self):
        assert Numbers.lte(1, 1)
        assert Numbers.lte(1, 2)
        assert not Numbers.lte(2, 1)

    def test_gt(self):
        assert Numbers.gt(2, 1)
        assert not Numbers.gt(1, 1)

    def test_gte(self):
        assert Numbers.gte(1, 1)
        assert Numbers.gte(2, 1)

    def test_max(self):
        assert Numbers.max(1, 2) == 2
        assert Numbers.max(2.5, 2) == 2.5

    def test_min(self):
        assert Numbers.min(1, 2) == 1


# ---- predicates ----

class TestPredicates:
    def test_is_zero(self):
        assert Numbers.is_zero(0)
        assert Numbers.is_zero(0.0)
        assert Numbers.is_zero(Ratio(0, 5))
        assert not Numbers.is_zero(1)
        assert not Numbers.is_zero(0.5)

    def test_is_pos(self):
        assert Numbers.is_pos(1)
        assert Numbers.is_pos(0.5)
        assert not Numbers.is_pos(0)
        assert not Numbers.is_pos(-1)

    def test_is_neg(self):
        assert Numbers.is_neg(-1)
        assert not Numbers.is_neg(0)
        assert not Numbers.is_neg(1)

    def test_is_nan(self):
        assert Numbers.is_nan(float("nan"))
        assert not Numbers.is_nan(1.5)
        assert not Numbers.is_nan(float("inf"))

    def test_is_infinite(self):
        assert Numbers.is_infinite(float("inf"))
        assert Numbers.is_infinite(float("-inf"))
        assert not Numbers.is_infinite(1.5)
        assert not Numbers.is_infinite(float("nan"))


# ---- bit ops ----

class TestBitOps:
    def test_and(self):
        assert Numbers.bit_and(0b1100, 0b1010) == 0b1000

    def test_or(self):
        assert Numbers.bit_or(0b1100, 0b1010) == 0b1110

    def test_xor(self):
        assert Numbers.bit_xor(0b1100, 0b1010) == 0b0110

    def test_not(self):
        assert Numbers.bit_not(0) == -1

    def test_and_not(self):
        # x & ~y
        assert Numbers.bit_and_not(0b1111, 0b1010) == 0b0101

    def test_set_clear_flip_test(self):
        x = 0
        x = Numbers.bit_set(x, 3)
        assert x == 0b1000
        assert Numbers.bit_test(x, 3)
        x = Numbers.bit_flip(x, 0)
        assert x == 0b1001
        x = Numbers.bit_clear(x, 3)
        assert x == 0b0001

    def test_shift_left(self):
        assert Numbers.shift_left(1, 4) == 16

    def test_shift_right_arithmetic(self):
        # Sign-extending: -8 >> 2 == -2.
        assert Numbers.shift_right(-8, 2) == -2

    def test_unsigned_shift_right_positive(self):
        assert Numbers.unsigned_shift_right(8, 2) == 2

    def test_unsigned_shift_right_negative(self):
        # Java: (-1L) >>> 1 == 0x7FFFFFFFFFFFFFFFL
        assert Numbers.unsigned_shift_right(-1, 1) == 0x7FFFFFFFFFFFFFFF

    def test_bit_count(self):
        assert Numbers.bit_count(0b1011010) == 4
        assert Numbers.bit_count(0) == 0

    def test_bit_op_rejects_float(self):
        with pytest.raises(TypeError):
            Numbers.bit_and(1.5, 1)

    def test_bit_op_rejects_bool(self):
        with pytest.raises(TypeError):
            Numbers.bit_and(True, 1)


# ---- mixed-type Numbers.equiv ----

class TestEquivAcrossTypes:
    def test_int_decimal(self):
        assert Numbers.equiv(1, Decimal("1"))
        assert Numbers.equiv(1, Decimal("1.0"))

    def test_decimal_decimal_value_equal(self):
        # Plain Decimal vs BigDecimal — both DECIMAL, value equal.
        # Numbers.equiv routes through value-equal regardless of subclass tag.
        assert Numbers.equiv(BigDecimal("1.5"), Decimal("1.5"))

    def test_bigint_int(self):
        assert Numbers.equiv(BigInt(5), 5)

    def test_bigint_ratio(self):
        # BigInt(2) == Ratio(2, 1) == Ratio(4, 2)
        assert Numbers.equiv(BigInt(2), Ratio(4, 2))
