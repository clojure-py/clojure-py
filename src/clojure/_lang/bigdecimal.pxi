# Port of java.math.BigDecimal — well, a thin wrapper around Python's
# decimal.Decimal that gives us a distinct type tag.
#
# Subclasses Decimal so all arithmetic is inherited. Equality is structural
# (only equal to other BigDecimal) to match Java BigDecimal.equals semantics
# in the same way BigInt does. hasheq normalizes trailing zeros (matching
# JVM Numbers.hasheqFrom for BigDecimal); the underlying hash is Python's
# Decimal hash, so it's NOT bit-exact with JVM BigDecimal.hashCode but is
# self-consistent within the runtime.


class BigDecimal(Decimal):
    """Type tag for arbitrary-precision decimal floating point."""

    def __new__(cls, value="0", context=None):
        return Decimal.__new__(cls, value, context)

    def __repr__(self):
        return f"BigDecimal({Decimal.__str__(self)})"

    def __eq__(self, other):
        return isinstance(other, BigDecimal) and Decimal.__eq__(self, other)

    def __ne__(self, other):
        return not self.__eq__(other)

    def __hash__(self):
        return Decimal.__hash__(self)

    def hasheq(self):
        # Java: stripTrailingZeros.hashCode, with 0 → BigDecimal.ZERO.hashCode.
        # We approximate using Python's Decimal hash on the normalized form.
        # Not bit-exact JVM, but the (= a b) → (hash a) == (hash b) invariant
        # holds within our runtime.
        if Decimal.__eq__(self, 0):
            return _to_int32_mask(Decimal.__hash__(BigDecimal(0)))
        return _to_int32_mask(Decimal.__hash__(self.normalize()))


IHashEq.register(BigDecimal)
