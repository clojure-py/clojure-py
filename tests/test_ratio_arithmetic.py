from decimal import Decimal

import pytest

from clojure.lang import Ratio, BigDecimal


class TestUnary:
    def test_neg(self):
        assert -Ratio(1, 2) == Ratio(-1, 2)

    def test_pos(self):
        r = Ratio(1, 2)
        assert (+r) is r

    def test_abs(self):
        assert abs(Ratio(-1, 2)) == Ratio(1, 2)
        assert abs(Ratio(1, -2)) == Ratio(1, 2)

    def test_float_conversion(self):
        assert float(Ratio(1, 2)) == 0.5

    def test_int_truncates_toward_zero(self):
        # Java BigInteger.divide trunc-toward-zero.
        assert int(Ratio(7, 2)) == 3
        assert int(Ratio(-7, 2)) == -3   # NOT -4 (which is floor)

    def test_bool_zero(self):
        assert not bool(Ratio(0, 1))
        assert bool(Ratio(1, 2))


class TestRatioPlusRatio:
    def test_add_reduces(self):
        # 1/2 + 1/2 = 1 (reduces to int)
        assert Ratio(1, 2) + Ratio(1, 2) == 1

    def test_add_keeps_ratio(self):
        # 1/3 + 1/2 = 5/6
        assert Ratio(1, 3) + Ratio(1, 2) == Ratio(5, 6)

    def test_subtract(self):
        assert Ratio(3, 4) - Ratio(1, 4) == Ratio(1, 2)

    def test_multiply(self):
        assert Ratio(2, 3) * Ratio(3, 4) == Ratio(1, 2)

    def test_divide(self):
        assert Ratio(1, 2) / Ratio(1, 4) == 2

    def test_divide_by_zero(self):
        with pytest.raises(ZeroDivisionError):
            Ratio(1, 2) / Ratio(0, 1)


class TestRatioWithInt:
    def test_add_int(self):
        # 1/2 + 1 = 3/2
        assert Ratio(1, 2) + 1 == Ratio(3, 2)

    def test_radd_int(self):
        assert 1 + Ratio(1, 2) == Ratio(3, 2)

    def test_subtract_int(self):
        assert Ratio(3, 2) - 1 == Ratio(1, 2)

    def test_rsub_int(self):
        assert 1 - Ratio(1, 2) == Ratio(1, 2)

    def test_multiply_int(self):
        assert Ratio(1, 2) * 4 == 2
        assert 4 * Ratio(1, 2) == 2

    def test_divide_int(self):
        assert Ratio(1, 2) / 2 == Ratio(1, 4)

    def test_int_divided_by_ratio(self):
        # 1 / (1/2) = 2
        assert 1 / Ratio(1, 2) == 2


class TestRatioWithFloat:
    def test_add_float(self):
        # Mixed Ratio+float demotes to float.
        assert Ratio(1, 2) + 0.5 == 1.0

    def test_radd_float(self):
        assert 0.5 + Ratio(1, 2) == 1.0

    def test_multiply_float(self):
        assert Ratio(1, 2) * 4.0 == 2.0


class TestRatioWithDecimal:
    def test_add_decimal(self):
        # Mixed Ratio+Decimal demotes to Decimal.
        result = Ratio(1, 2) + Decimal("0.5")
        assert Decimal.__eq__(result, Decimal("1.0"))


class TestPow:
    def test_positive_exp(self):
        assert Ratio(2, 3) ** 2 == Ratio(4, 9)

    def test_zero_exp(self):
        assert Ratio(7, 11) ** 0 == 1

    def test_negative_exp(self):
        assert Ratio(2, 3) ** -2 == Ratio(9, 4)

    def test_negative_exp_zero_base_raises(self):
        with pytest.raises(ZeroDivisionError):
            Ratio(0, 1) ** -1

    def test_float_exp_demotes(self):
        # Non-int exponent → float result.
        result = Ratio(1, 4) ** 0.5
        assert result == 0.5
