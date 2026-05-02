# Port of clojure.lang.Util — equality, hashing, misc helpers.
#
# Slice 4 update: equiv / hasheq now delegate to Numbers for numeric types,
# matching JVM semantics:
#   - Util.equiv(1, 1.0) → False  (Numbers.equal requires same category)
#   - Util.equiv(True, 1) → False (Java separates Boolean from Number)
#   - Util.hasheq(big_int) matches Java BigInteger.hashCode bit-for-bit


cdef class Util:
    """Static namespace mirroring clojure.lang.Util."""

    def __cinit__(self):
        raise TypeError("Util is a static namespace, not instantiable")

    @staticmethod
    def identical(a, b):
        return a is b

    @staticmethod
    def equals(a, b):
        # Java Util.equals: identity, then null-safe a.equals(b). Python's `==`
        # is permissive (1 == 1.0, True == 1) compared to Java's per-class
        # equals, which we accept here — Util.equiv is the strict variant.
        if a is b:
            return True
        if a is None:
            return False
        return a == b

    @staticmethod
    cdef int32_t _hash_combine(int32_t seed, int32_t hash) noexcept nogil:
        # Boost-style. `seed >> 2` uses signed shift to match Java's arithmetic-
        # shift semantics for negative seeds; everything else routes through
        # uint32 for well-defined modular arithmetic.
        cdef uint32_t us = <uint32_t>seed
        cdef uint32_t uh = <uint32_t>hash
        cdef uint32_t shr = <uint32_t>(seed >> 2)
        cdef uint32_t added = (uh + 0x9e3779b9u + (us << 6) + shr) & 0xFFFFFFFFu
        return <int32_t>(us ^ added)

    @staticmethod
    def hash_combine(int32_t seed, int32_t hash):
        return Util._hash_combine(seed, hash)

    @staticmethod
    def is_integer(x):
        return isinstance(x, int) and not isinstance(x, bool)

    @staticmethod
    def equiv(k1, k2):
        # Java Util.equiv: identity, null guard, Numbers.equal for two-Number
        # case (same-category required), pcequiv for collections, .equals
        # otherwise. Bool is excluded from the Number path; mismatched-bool
        # comparison returns False to match JVM Boolean.equals semantics.
        if k1 is k2:
            return True
        if k1 is None:
            return False
        if isinstance(k1, bool) or isinstance(k2, bool):
            if isinstance(k1, bool) != isinstance(k2, bool):
                return False
            return k1 == k2
        if Numbers._is_number(k1) and Numbers._is_number(k2):
            return Numbers.equal(k1, k2)
        if isinstance(k1, IPersistentCollection) or isinstance(k2, IPersistentCollection):
            return Util.pcequiv(k1, k2)
        return k1 == k2

    @staticmethod
    def pcequiv(k1, k2):
        if isinstance(k1, IPersistentCollection):
            return k1.equiv(k2)
        return k2.equiv(k1)

    @staticmethod
    def hash(o):
        # Java's Util.hash: 0 if null, else o.hashCode(). For Java strings we
        # compute Java's String.hashCode — Python's siphash would diverge.
        if o is None:
            return 0
        if isinstance(o, str):
            return _java_string_hashcode(o)
        return _to_int32_mask(hash(o))

    @staticmethod
    def hasheq(o):
        # Java Util.hasheq: dispatch by type. bool → 1231/1237 (Java
        # Boolean.hashCode); IHashEq → o.hasheq(); Number → Numbers.hasheq;
        # str → Murmur3.hashInt(java_string_hashcode); else Python hash
        # truncated to int32.
        if o is None:
            return 0
        if isinstance(o, bool):
            return 1231 if o else 1237
        if isinstance(o, IHashEq):
            return o.hasheq()
        if Numbers._is_number(o):
            return Numbers._hasheq(o)
        if isinstance(o, str):
            return Murmur3._hash_int(_java_string_hashcode(o))
        return _to_int32_mask(hash(o))

    @staticmethod
    def java_string_hashcode(str s):
        return _java_string_hashcode(s)
