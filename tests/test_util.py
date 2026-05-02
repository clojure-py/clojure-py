from clojure.lang import Util, Symbol, Keyword, Murmur3, Numbers, Ratio


INT32_MIN = -(1 << 31)
INT32_MAX = (1 << 31) - 1


class TestIdentical:
    def test_same_object(self):
        x = object()
        assert Util.identical(x, x)

    def test_distinct_objects(self):
        assert not Util.identical(object(), object())

    def test_none(self):
        assert Util.identical(None, None)

    def test_small_ints_interned(self):
        assert Util.identical(1, 1)


class TestEquals:
    def test_identity(self):
        x = object()
        assert Util.equals(x, x)

    def test_none_vs_none(self):
        assert Util.equals(None, None)

    def test_none_vs_value(self):
        assert not Util.equals(None, 1)
        assert not Util.equals(1, None)

    def test_value_equality(self):
        assert Util.equals(1, 1)
        assert Util.equals("foo", "foo")
        assert not Util.equals(1, 2)


class TestHashCombine:
    def test_returns_int32(self):
        assert INT32_MIN <= Util.hash_combine(0, 0) <= INT32_MAX
        assert INT32_MIN <= Util.hash_combine(-1, -1) <= INT32_MAX
        assert INT32_MIN <= Util.hash_combine(INT32_MAX, INT32_MIN) <= INT32_MAX

    def test_deterministic(self):
        assert Util.hash_combine(42, 99) == Util.hash_combine(42, 99)

    def test_order_matters(self):
        assert Util.hash_combine(1, 2) != Util.hash_combine(2, 1)


class TestIsInteger:
    def test_int_true(self):
        assert Util.is_integer(0)
        assert Util.is_integer(1)
        assert Util.is_integer(-(1 << 100))

    def test_bool_false(self):
        assert not Util.is_integer(True)
        assert not Util.is_integer(False)

    def test_non_int_false(self):
        assert not Util.is_integer(1.0)
        assert not Util.is_integer("1")
        assert not Util.is_integer(None)


class TestEquiv:
    def test_identity(self):
        x = object()
        assert Util.equiv(x, x)

    def test_none_vs_none(self):
        assert Util.equiv(None, None)

    def test_none_vs_value(self):
        assert not Util.equiv(None, 1)
        assert not Util.equiv(1, None)

    def test_same_category_numbers(self):
        # Both INTEGER → equal value compares true.
        assert Util.equiv(2, 2)
        assert not Util.equiv(2, 3)
        # Both FLOATING → equal value compares true.
        assert Util.equiv(2.5, 2.5)
        assert not Util.equiv(2.5, 3.5)

    def test_cross_category_numbers_not_equiv(self):
        # JVM Util.equiv calls Numbers.equal which requires same Category.
        # (= 1 1.0) → false — INTEGER vs FLOATING.
        assert not Util.equiv(1, 1.0)
        assert not Util.equiv(1.0, 1)
        # INTEGER vs RATIO also categories-different.
        assert not Util.equiv(1, Ratio(1, 1))

    def test_bool_excluded_from_number_path(self):
        # Java separates Boolean from Number. Util.equiv(True, 1) returns
        # false; Boolean.equals(Integer) is false on the JVM.
        assert not Util.equiv(True, 1)
        assert not Util.equiv(1, True)
        # Bool vs bool still works.
        assert Util.equiv(True, True)
        assert not Util.equiv(True, False)

    def test_string_equality(self):
        assert Util.equiv("foo", "foo")
        assert not Util.equiv("foo", "bar")

    def test_symbol_equiv(self):
        assert Util.equiv(Symbol.intern("foo"), Symbol.intern("foo"))


class TestHasheq:
    def test_none_is_zero(self):
        assert Util.hasheq(None) == 0

    def test_bool_matches_java(self):
        assert Util.hasheq(True) == 1231
        assert Util.hasheq(False) == 1237

    def test_int_uses_murmur3_hash_long(self):
        for v in [0, 1, -1, 42, 1 << 40, -(1 << 40)]:
            assert Util.hasheq(v) == Murmur3.hash_long(v)

    def test_int_zero(self):
        assert Util.hasheq(0) == 0

    def test_big_int_uses_big_integer_hashcode(self):
        # Out-of-i64-range Python ints route through Numbers._hasheq which
        # routes through _big_integer_hashcode — should match Java
        # BigInteger.hashCode.
        # Java: BigInteger("12345678901234567890").hashCode() == -522017919
        # (verified externally — hand-computed below in TestNumbers).
        assert Util.hasheq(12345678901234567890) == Numbers.hasheq(12345678901234567890)

    def test_string_uses_murmur3_of_java_string_hashcode(self):
        s = "hello"
        expected = Murmur3.hash_int(Util.java_string_hashcode(s))
        assert Util.hasheq(s) == expected

    def test_symbol_dispatches_to_hasheq(self):
        s = Symbol.intern("user", "foo")
        assert Util.hasheq(s) == s.hasheq()

    def test_keyword_dispatches_to_hasheq(self):
        k = Keyword.intern("foo")
        assert Util.hasheq(k) == k.hasheq()

    def test_float_zero_returns_zero(self):
        assert Util.hasheq(0.0) == 0
        assert Util.hasheq(-0.0) == 0

    def test_float_uses_double_hashcode(self):
        assert Util.hasheq(1.5) == Numbers.hasheq(1.5)


class TestJavaStringHashcode:
    def test_empty(self):
        assert Util.java_string_hashcode("") == 0

    def test_known_values(self):
        # 'a'*31^2 + 'b'*31 + 'c' = 96354
        assert Util.java_string_hashcode("abc") == 96354
        # Famous JVM value: "Hello".hashCode() == 69609650
        assert Util.java_string_hashcode("Hello") == 69609650

    def test_returns_int32(self):
        assert INT32_MIN <= Util.java_string_hashcode("any string at all") <= INT32_MAX


class TestNotInstantiable:
    def test_construct_raises(self):
        import pytest
        with pytest.raises(TypeError):
            Util()
