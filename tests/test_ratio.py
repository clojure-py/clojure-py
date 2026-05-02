import pytest

from clojure.lang import Ratio


class TestConstruction:
    def test_basic(self):
        r = Ratio(1, 2)
        assert r.numerator == 1 and r.denominator == 2

    def test_negative_numerator(self):
        r = Ratio(-3, 4)
        assert r.numerator == -3 and r.denominator == 4

    def test_negative_denominator_preserved(self):
        # JVM Ratio doesn't normalize. We preserve the given pair.
        r = Ratio(1, -2)
        assert r.denominator == -2

    def test_arbitrary_precision(self):
        big = 10**40
        r = Ratio(big, big + 1)
        assert r.numerator == big

    def test_zero_denominator_raises(self):
        with pytest.raises(ZeroDivisionError):
            Ratio(1, 0)

    def test_non_int_numerator_raises(self):
        with pytest.raises(TypeError):
            Ratio(1.5, 2)

    def test_bool_numerator_raises(self):
        with pytest.raises(TypeError):
            Ratio(True, 2)


class TestStr:
    def test_simple(self):
        assert str(Ratio(1, 2)) == "1/2"

    def test_negative(self):
        assert str(Ratio(-1, 2)) == "-1/2"

    def test_repr_matches_str(self):
        r = Ratio(7, 3)
        assert repr(r) == str(r) == "7/3"


class TestEquality:
    def test_structural(self):
        assert Ratio(1, 2) == Ratio(1, 2)

    def test_unreduced_distinct(self):
        # Java Ratio.equals is component-wise; (1/2) != (2/4) at the equals
        # level even though they're numerically equal. Numbers.equiv handles
        # the value-equal case separately.
        assert Ratio(1, 2) != Ratio(2, 4)

    def test_not_equal_to_int(self):
        assert Ratio(1, 1) != 1

    def test_not_equal_to_non_ratio(self):
        assert Ratio(1, 2) != "1/2"
        assert Ratio(1, 2) is not None


class TestHashing:
    def test_hashable_in_set(self):
        a = Ratio(1, 2)
        b = Ratio(1, 2)
        assert {a, b} == {a}

    def test_hash_is_int(self):
        assert isinstance(hash(Ratio(1, 2)), int)


class TestComparison:
    def test_lt_simple(self):
        # 1/2 < 2/3
        assert Ratio(1, 2) < Ratio(2, 3)

    def test_lt_with_negative_denominator(self):
        # 1/-2 == -1/2; -1/2 < 1/2, so 1/-2 < 1/2.
        assert Ratio(1, -2) < Ratio(1, 2)

    def test_lt_against_int(self):
        assert Ratio(1, 2) < 1
        assert Ratio(3, 2) > 1

    def test_lt_against_float(self):
        assert Ratio(1, 2) < 0.6
        assert Ratio(3, 4) > 0.5

    def test_eq_when_same_components(self):
        # Same numerator/denominator → compare returns 0 → not <, not >.
        a, b = Ratio(2, 5), Ratio(2, 5)
        assert not (a < b)
        assert not (a > b)


class TestToFloat:
    def test_simple(self):
        assert Ratio(1, 2).to_float() == 0.5

    def test_negative(self):
        assert Ratio(-1, 4).to_float() == -0.25
