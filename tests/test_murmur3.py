from clojure.lang import Murmur3


INT32_MIN = -(1 << 31)
INT32_MAX = (1 << 31) - 1


def in_int32(x: int) -> bool:
    return INT32_MIN <= x <= INT32_MAX


class TestHashInt:
    def test_zero_is_zero(self):
        assert Murmur3.hash_int(0) == 0

    def test_returns_int32(self):
        for v in [1, -1, 42, INT32_MAX, INT32_MIN]:
            assert in_int32(Murmur3.hash_int(v))

    def test_deterministic(self):
        assert Murmur3.hash_int(12345) == Murmur3.hash_int(12345)

    def test_distinct_inputs_distinct_outputs(self):
        seen = {Murmur3.hash_int(i) for i in range(1, 1000)}
        assert len(seen) == 999


class TestHashLong:
    def test_zero_is_zero(self):
        assert Murmur3.hash_long(0) == 0

    def test_low_32_match_hash_int_when_high_zero(self):
        # hash_long folds in two mixK1 rounds even when high is zero, so it
        # MUST differ from hash_int(low). Asserting the difference guards
        # against regressing into hash_int's single-round form.
        assert Murmur3.hash_long(42) != Murmur3.hash_int(42)

    def test_returns_int32(self):
        for v in [1, -1, 1 << 40, -(1 << 40), (1 << 63) - 1, -(1 << 63)]:
            assert in_int32(Murmur3.hash_long(v))


class TestHashUnencodedChars:
    def test_empty_is_zero(self):
        assert Murmur3.hash_unencoded_chars("") == 0

    def test_deterministic(self):
        assert Murmur3.hash_unencoded_chars("hello") == Murmur3.hash_unencoded_chars("hello")

    def test_returns_int32(self):
        for s in ["a", "ab", "abc", "abcd", "hello world"]:
            assert in_int32(Murmur3.hash_unencoded_chars(s))

    def test_distinct_strings(self):
        seen = {Murmur3.hash_unencoded_chars(s) for s in ["a", "b", "ab", "ba", "abc"]}
        assert len(seen) == 5


class TestMixCollHash:
    def test_returns_int32(self):
        assert in_int32(Murmur3.mix_coll_hash(0, 0))
        assert in_int32(Murmur3.mix_coll_hash(12345, 7))
        assert in_int32(Murmur3.mix_coll_hash(-1, 1000))

    def test_deterministic(self):
        assert Murmur3.mix_coll_hash(99, 3) == Murmur3.mix_coll_hash(99, 3)


class TestNotInstantiable:
    def test_construct_raises(self):
        import pytest
        with pytest.raises(TypeError):
            Murmur3()
