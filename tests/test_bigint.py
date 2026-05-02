import pytest

from clojure.lang import BigInt, IHashEq, Numbers, Murmur3


class TestConstruction:
    def test_basic(self):
        b = BigInt(5)
        assert int(b) == 5

    def test_negative(self):
        assert int(BigInt(-100)) == -100

    def test_arbitrary_precision(self):
        big = 10 ** 50
        assert int(BigInt(big)) == big

    def test_default_value(self):
        assert int(BigInt()) == 0

    def test_bool_rejected(self):
        with pytest.raises(TypeError):
            BigInt(True)


class TestEquality:
    def test_structural_with_other_bigint(self):
        # BigInt(5) == BigInt(5) — Java BigInt.equals.
        assert BigInt(5) == BigInt(5)

    def test_not_equal_to_plain_int(self):
        # Java BigInt.equals(Long) is false even when value matches.
        assert BigInt(5) != 5
        assert 5 != BigInt(5)

    def test_not_equal_to_other_types(self):
        assert BigInt(5) != "5"
        assert BigInt(5) != 5.0


class TestHashing:
    def test_hashable(self):
        b1 = BigInt(5)
        b2 = BigInt(5)
        # Same int hash since __hash__ delegates to int.
        assert hash(b1) == hash(b2)

    def test_hash_eq_with_int_via_python_dict(self):
        # Python's hash(BigInt(5)) == hash(5) (both delegate to int.__hash__).
        # Even though BigInt(5) != 5 by our structural eq, this is still hash-
        # contract-legal (equal-hash without equal-objects is just a collision).
        assert hash(BigInt(5)) == hash(5)


class TestHasheq:
    def test_in_long_range_matches_int(self):
        # (= 5N 5) is true in Clojure, so (hash 5N) must equal (hash 5).
        for v in [0, 1, -1, 42, 1 << 40, -(1 << 40), (1 << 63) - 1, -(1 << 63)]:
            assert BigInt(v).hasheq() == Numbers.hasheq(v)

    def test_out_of_long_range_uses_big_integer_hashcode(self):
        big = 10 ** 25  # > 2^63
        assert BigInt(big).hasheq() == Numbers.hasheq(big)

    def test_isinstance_ihasheq(self):
        assert isinstance(BigInt(5), IHashEq)


class TestRepr:
    def test_repr(self):
        assert repr(BigInt(5)) == "BigInt(5)"

    def test_str_with_n_suffix(self):
        # Clojure literal syntax: 42N
        assert str(BigInt(42)) == "42N"


class TestFactories:
    def test_from_long(self):
        assert int(BigInt.from_long(42)) == 42

    def test_from_int(self):
        assert int(BigInt.from_int(99)) == 99

    def test_from_bigint(self):
        assert int(BigInt.from_bigint(BigInt(7))) == 7


class TestInheritsArithmeticFromInt:
    # The BigInt tag is preserved at the Numbers API boundary, not transparently
    # across Python `+`. So bare `BigInt + int` returns plain int. (Numbers.add
    # is the API to use if you want the tag to flow through.)
    def test_plain_add_returns_int(self):
        result = BigInt(5) + 1
        assert result == 6
        assert not isinstance(result, BigInt)

    def test_used_as_int(self):
        # Inheriting from int means BigInt is fully usable as an int.
        assert BigInt(5) + BigInt(7) == 12


class TestNumberCategoryAndIsNumber:
    def test_is_number(self):
        assert Numbers.is_number(BigInt(5))

    def test_bigint_is_integer_category(self):
        # Same category as plain int, so Numbers.equal sees them as same-category.
        assert Numbers.equal(BigInt(5), 5)
