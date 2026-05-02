# Port of clojure.lang.BigInt.
#
# Java distinguishes Long (fixed 64-bit) from BigInt (arbitrary precision)
# because the JVM has no native bigint. Python `int` is already arbitrary
# precision, so BigInt is mostly redundant here — it exists as a type tag so
# `(instance? BigInt x)` checks behave the way Clojure code expects, and so
# Numbers.* can preserve "this value was promoted past Long range" through
# the API surface.
#
# Implemented as a Python subclass of int. All arithmetic is inherited from
# int and produces plain int (the BigInt tag is preserved at the Numbers API
# boundary, not transparently across Python `+` / `*` / etc.). Equality is
# structural: BigInt(5) == 5 is False, matching Java's BigInt.equals.


class BigInt(int):
    """Type tag for arbitrary-precision integers, mirroring clojure.lang.BigInt."""

    def __new__(cls, value=0):
        if isinstance(value, bool):
            raise TypeError("BigInt does not accept bool")
        return super().__new__(cls, value)

    def __repr__(self):
        return f"BigInt({int(self)})"

    def __str__(self):
        # Clojure printer suffix for BigInt literals: 42N
        return f"{int(self)}N"

    def __eq__(self, other):
        return isinstance(other, BigInt) and int.__eq__(self, other)

    def __ne__(self, other):
        return not self.__eq__(other)

    def __hash__(self):
        return int.__hash__(self)

    def hasheq(self):
        # Match Numbers.hasheq for an Integer of this value: in-Long-range goes
        # through Murmur3.hashLong, out-of-range through BigInteger.hashCode.
        # Ensures (= 5N 5) implies (hash 5N) == (hash 5).
        cdef long long lv
        v = int(self)
        if -(1 << 63) <= v < (1 << 63):
            lv = v
            return Murmur3._hash_long(lv)
        return _big_integer_hashcode(v)

    @classmethod
    def from_long(cls, value):
        return cls(value)

    @classmethod
    def from_bigint(cls, value):
        return cls(int(value))

    @classmethod
    def from_int(cls, value):
        return cls(value)

    def to_python_int(self):
        return int(self)


IHashEq.register(BigInt)
