# Port of clojure.lang.Murmur3 (MurmurHash3_x86_32).
# Original C++ by Austin Appleby (public domain).
# Java/Clojure port from Guava under Apache 2.0.

cdef uint32_t _M3_SEED = 0u
cdef uint32_t _M3_C1 = 0xcc9e2d51u
cdef uint32_t _M3_C2 = 0x1b873593u


cdef inline uint32_t _m3_rotl32(uint32_t x, int n) noexcept nogil:
    return ((x << n) | (x >> (32 - n))) & 0xFFFFFFFFu


cdef inline uint32_t _m3_mix_k1(uint32_t k1) noexcept nogil:
    k1 = (k1 * _M3_C1) & 0xFFFFFFFFu
    k1 = _m3_rotl32(k1, 15)
    k1 = (k1 * _M3_C2) & 0xFFFFFFFFu
    return k1


cdef inline uint32_t _m3_mix_h1(uint32_t h1, uint32_t k1) noexcept nogil:
    h1 ^= k1
    h1 = _m3_rotl32(h1, 13)
    h1 = ((h1 * 5u) + 0xe6546b64u) & 0xFFFFFFFFu
    return h1


cdef inline uint32_t _m3_fmix(uint32_t h1, uint32_t length) noexcept nogil:
    h1 ^= length
    h1 ^= h1 >> 16
    h1 = (h1 * 0x85ebca6bu) & 0xFFFFFFFFu
    h1 ^= h1 >> 13
    h1 = (h1 * 0xc2b2ae35u) & 0xFFFFFFFFu
    h1 ^= h1 >> 16
    return h1


cdef class Murmur3:
    """Static namespace mirroring clojure.lang.Murmur3."""

    def __cinit__(self):
        raise TypeError("Murmur3 is a static namespace, not instantiable")

    @staticmethod
    cdef int32_t _hash_int(int32_t input) noexcept nogil:
        cdef uint32_t k1, h1
        if input == 0:
            return 0
        k1 = _m3_mix_k1(<uint32_t>input)
        h1 = _m3_mix_h1(_M3_SEED, k1)
        return <int32_t>_m3_fmix(h1, 4u)

    @staticmethod
    def hash_int(int32_t input):
        return Murmur3._hash_int(input)

    @staticmethod
    cdef int32_t _hash_long(long long input) noexcept nogil:
        cdef uint32_t low, high, k1, h1
        if input == 0:
            return 0
        low = <uint32_t>(<unsigned long long>input & 0xFFFFFFFFull)
        high = <uint32_t>((<unsigned long long>input >> 32) & 0xFFFFFFFFull)
        k1 = _m3_mix_k1(low)
        h1 = _m3_mix_h1(_M3_SEED, k1)
        k1 = _m3_mix_k1(high)
        h1 = _m3_mix_h1(h1, k1)
        return <int32_t>_m3_fmix(h1, 8u)

    @staticmethod
    def hash_long(long long input):
        return Murmur3._hash_long(input)

    @staticmethod
    cdef int32_t _hash_unencoded_chars(str input):
        # Java treats CharSequence as UTF-16 code units. Python str holds Unicode
        # code points; values <= 0xFFFF match JVM bit-for-bit, supplementary chars
        # do not (a future revision can encode-to-UTF-16 first if exact JVM parity
        # outside the BMP becomes important).
        cdef uint32_t h1 = _M3_SEED
        cdef uint32_t k1
        cdef Py_ssize_t i = 1
        cdef Py_ssize_t n = len(input)

        while i < n:
            k1 = (<uint32_t>ord(input[i - 1]) | (<uint32_t>ord(input[i]) << 16)) & 0xFFFFFFFFu
            k1 = _m3_mix_k1(k1)
            h1 = _m3_mix_h1(h1, k1)
            i += 2

        if (n & 1) == 1:
            k1 = <uint32_t>ord(input[n - 1])
            k1 = _m3_mix_k1(k1)
            h1 ^= k1

        return <int32_t>_m3_fmix(h1, <uint32_t>(2 * n))

    @staticmethod
    def hash_unencoded_chars(str input):
        return Murmur3._hash_unencoded_chars(input)

    @staticmethod
    cdef int32_t _mix_coll_hash(int32_t hash, int32_t count) noexcept nogil:
        cdef uint32_t k1, h1
        h1 = _M3_SEED
        k1 = _m3_mix_k1(<uint32_t>hash)
        h1 = _m3_mix_h1(h1, k1)
        return <int32_t>_m3_fmix(h1, <uint32_t>count)

    @staticmethod
    def mix_coll_hash(int32_t hash, int32_t count):
        return Murmur3._mix_coll_hash(hash, count)

    @staticmethod
    def hash_ordered(xs):
        # Java Murmur3.hashOrdered: hash = 1; for x: hash = 31*hash + Util.hasheq(x).
        # Then mix_coll_hash(hash, count).
        cdef uint32_t h = 1u
        cdef int32_t n = 0
        for x in xs:
            h = (31u * h + <uint32_t><int32_t>Util.hasheq(x)) & 0xFFFFFFFFu
            n += 1
        return Murmur3._mix_coll_hash(<int32_t>h, n)

    @staticmethod
    def hash_unordered(xs):
        # Java Murmur3.hashUnordered: hash = 0; for x: hash += Util.hasheq(x).
        # Then mix_coll_hash(hash, count).
        cdef uint32_t h = 0u
        cdef int32_t n = 0
        for x in xs:
            h = (h + <uint32_t><int32_t>Util.hasheq(x)) & 0xFFFFFFFFu
            n += 1
        return Murmur3._mix_coll_hash(<int32_t>h, n)
