import math
import pytest

from clojure.lang import Numbers, Ratio, Murmur3


INT32_MIN = -(1 << 31)
INT32_MAX = (1 << 31) - 1


class TestIsNumber:
    def test_int(self):
        assert Numbers.is_number(0)
        assert Numbers.is_number(-5)
        assert Numbers.is_number(1 << 100)

    def test_float(self):
        assert Numbers.is_number(0.0)
        assert Numbers.is_number(1.5)
        assert Numbers.is_number(float("nan"))

    def test_ratio(self):
        assert Numbers.is_number(Ratio(1, 2))

    def test_bool_excluded(self):
        # Java separates Boolean from Number.
        assert not Numbers.is_number(True)
        assert not Numbers.is_number(False)

    def test_other_excluded(self):
        assert not Numbers.is_number("1")
        assert not Numbers.is_number(None)
        assert not Numbers.is_number([1, 2])


class TestEqual:
    """Same-category, value-equal. Used by Util.equiv for `(= a b)` semantics."""

    def test_int_int(self):
        assert Numbers.equal(2, 2)
        assert not Numbers.equal(2, 3)

    def test_int_float_different_category(self):
        # INTEGER vs FLOATING — different categories → false even though values match.
        assert not Numbers.equal(1, 1.0)
        assert not Numbers.equal(0, 0.0)

    def test_int_ratio_different_category(self):
        assert not Numbers.equal(1, Ratio(1, 1))

    def test_ratio_ratio_value_equal(self):
        # Same category (RATIO), and numerically equal → true.
        # Note: Numbers.equal is value-equal (cross-multiplied), unlike
        # Ratio.__eq__ which is component-wise.
        assert Numbers.equal(Ratio(1, 2), Ratio(2, 4))
        assert not Numbers.equal(Ratio(1, 2), Ratio(1, 3))

    def test_non_number_returns_false(self):
        assert not Numbers.equal(1, "1")


class TestEquiv:
    """Cross-category value equality. Used by Clojure `==`."""

    def test_int_float_cross_type(self):
        assert Numbers.equiv(1, 1.0)
        assert not Numbers.equiv(1, 2.0)

    def test_int_ratio_cross_type(self):
        assert Numbers.equiv(1, Ratio(2, 2))
        assert Numbers.equiv(2, Ratio(4, 2))
        assert not Numbers.equiv(1, Ratio(1, 2))

    def test_ratio_ratio_value_equal(self):
        assert Numbers.equiv(Ratio(1, 2), Ratio(2, 4))

    def test_non_number(self):
        assert not Numbers.equiv(1, "1")


class TestCompare:
    def test_int_int(self):
        assert Numbers.compare(1, 2) == -1
        assert Numbers.compare(2, 1) == 1
        assert Numbers.compare(3, 3) == 0

    def test_int_float(self):
        assert Numbers.compare(1, 2.5) == -1
        assert Numbers.compare(3, 2.5) == 1
        assert Numbers.compare(1, 1.0) == 0

    def test_ratio_int(self):
        assert Numbers.compare(Ratio(1, 2), 1) == -1
        assert Numbers.compare(Ratio(3, 2), 1) == 1
        assert Numbers.compare(Ratio(2, 1), 2) == 0

    def test_ratio_ratio(self):
        assert Numbers.compare(Ratio(1, 3), Ratio(1, 2)) == -1
        assert Numbers.compare(Ratio(2, 3), Ratio(1, 2)) == 1


class TestHasheq:
    def test_int_in_long_range(self):
        for v in [0, 1, -1, 1 << 40, -(1 << 40), (1 << 63) - 1, -(1 << 63)]:
            assert Numbers.hasheq(v) == Murmur3.hash_long(v)

    def test_int_out_of_long_range(self):
        # Outside i64 range → routes through Java BigInteger.hashCode.
        # Hand-validated values:
        # Java: new BigInteger("12345678901234567890").hashCode() == ?
        # We assert structural properties: int32, deterministic, distinct.
        big = 12345678901234567890
        h1 = Numbers.hasheq(big)
        h2 = Numbers.hasheq(big)
        assert h1 == h2
        assert INT32_MIN <= h1 <= INT32_MAX
        # Different big int → different hash (with extremely high probability)
        assert Numbers.hasheq(big) != Numbers.hasheq(big + 1)

    def test_neg_int_at_int64_boundary(self):
        # -(1<<63) is the minimum signed 64-bit value — boundary case.
        assert Numbers.hasheq(-(1 << 63)) == Murmur3.hash_long(-(1 << 63))

    def test_float_zero_and_neg_zero(self):
        # JVM carve-out: -0.0 hashes the same as 0.0 (= 0).
        assert Numbers.hasheq(0.0) == 0
        assert Numbers.hasheq(-0.0) == 0

    def test_float_nan(self):
        # All NaNs canonicalize to one bit pattern → same hash.
        assert Numbers.hasheq(float("nan")) == Numbers.hasheq(float("nan"))

    def test_float_known_value(self):
        # Java: Double.valueOf(1.0).hashCode() == 1072693248
        # bits = doubleToLongBits(1.0) = 0x3FF0000000000000
        # high = 0x3FF00000 = 1072693248, low = 0
        # high ^ low = 1072693248
        assert Numbers.hasheq(1.0) == 1072693248

    def test_ratio_hash(self):
        r = Ratio(1, 2)
        # Should equal numerator hash XOR denominator hash (per Java Ratio).
        assert Numbers.hasheq(r) == hash(r)

    def test_returns_int32(self):
        for v in [0, 1.5, Ratio(7, 3), 1 << 80]:
            assert INT32_MIN <= Numbers.hasheq(v) <= INT32_MAX


class TestNotInstantiable:
    def test_construct_raises(self):
        with pytest.raises(TypeError):
            Numbers()
