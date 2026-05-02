from decimal import Decimal

import pytest

from clojure.lang import BigDecimal, IHashEq, Numbers


class TestConstruction:
    def test_from_string(self):
        b = BigDecimal("1.5")
        assert Decimal.__eq__(b, Decimal("1.5"))

    def test_from_int(self):
        b = BigDecimal(5)
        assert Decimal.__eq__(b, Decimal(5))

    def test_default_zero(self):
        assert Decimal.__eq__(BigDecimal(), Decimal(0))


class TestEquality:
    def test_structural_with_other_bigdecimal(self):
        assert BigDecimal("1.5") == BigDecimal("1.5")

    def test_not_equal_to_plain_decimal(self):
        # Mirrors Java's structural BigDecimal.equals: only equal to other
        # BigDecimal.
        assert BigDecimal("1.5") != Decimal("1.5")

    def test_not_equal_to_int_or_float(self):
        assert BigDecimal("1") != 1
        assert BigDecimal("1.5") != 1.5


class TestHashing:
    def test_hashable(self):
        a = BigDecimal("1.5")
        b = BigDecimal("1.5")
        assert hash(a) == hash(b)


class TestHasheq:
    def test_zero_normalization(self):
        # JVM Numbers.hasheqFrom for BigDecimal: zero hashes the same regardless
        # of scale ("0" vs "0.000").
        assert BigDecimal("0").hasheq() == BigDecimal("0.000").hasheq()

    def test_trailing_zeros_normalized(self):
        # 1.5 and 1.50 should hash identically — JVM uses stripTrailingZeros.
        assert BigDecimal("1.5").hasheq() == BigDecimal("1.50").hasheq()

    def test_isinstance_ihasheq(self):
        assert isinstance(BigDecimal("1.5"), IHashEq)


class TestRepr:
    def test_repr_uses_str_form(self):
        assert repr(BigDecimal("1.5")) == "BigDecimal(1.5)"


class TestArithmeticInheritedFromDecimal:
    def test_add(self):
        # Inherited from Decimal — returns plain Decimal (tag lost). Numbers.add
        # is the API for tag-preservation work.
        result = BigDecimal("1.5") + BigDecimal("2.5")
        assert Decimal.__eq__(result, Decimal("4.0"))

    def test_multiply(self):
        result = BigDecimal("2") * BigDecimal("3")
        assert Decimal.__eq__(result, Decimal("6"))


class TestNumberCategory:
    def test_is_number(self):
        assert Numbers.is_number(BigDecimal("1.5"))

    def test_decimal_category_distinct_from_int(self):
        # BigDecimal is DECIMAL category, int is INTEGER → Numbers.equal false.
        assert not Numbers.equal(BigDecimal("1"), 1)

    def test_numbers_equiv_cross_type_with_int(self):
        # Numbers.equiv (cross-type, lossless for these): BigDecimal("1") == 1
        assert Numbers.equiv(BigDecimal("1"), 1)
