# Module-level cdef hash helpers used by Util, Ratio, Numbers, Symbol, and
# anyone else who needs them. Pulled out so each .pxi can include them via
# the umbrella lang.pyx without circular dependencies.

from libc.stdint cimport uint64_t, int64_t
from libc.string cimport memcpy


cdef inline int32_t _java_string_hashcode(str s) noexcept:
    # Java String.hashCode: h = 0; for c in chars: h = 31*h + c. 32-bit wrap.
    cdef uint32_t h = 0u
    cdef Py_ssize_t i
    cdef Py_ssize_t n = len(s)
    for i in range(n):
        h = (31u * h + <uint32_t>ord(s[i])) & 0xFFFFFFFFu
    return <int32_t>h


cdef inline int32_t _to_int32_mask(object pyhash):
    # Truncate any Python int to 32 bits with sign extension.
    cdef uint32_t v = <uint32_t>(pyhash & 0xFFFFFFFF)
    return <int32_t>v


cdef inline int32_t _double_hashcode(double x) noexcept:
    # Java Double.hashCode = (int)(bits ^ (bits >>> 32)) where
    # bits = doubleToLongBits(x). All NaNs canonicalize to 0x7ff8000000000000L.
    cdef uint64_t bits
    cdef uint32_t high, low
    if x != x:
        bits = 0x7ff8000000000000ull
    else:
        memcpy(&bits, &x, 8)
    high = <uint32_t>(bits >> 32)
    low = <uint32_t>(bits & 0xFFFFFFFFull)
    return <int32_t>(high ^ low)


cdef int32_t _big_integer_hashcode(object o):
    # Java BigInteger.hashCode applied to a Python arbitrary-precision int.
    # Algorithm: for each 32-bit big-endian magnitude chunk c, h = 31*h + c
    # (mod 2^32); then negate (mod 2^32) if sign is negative. Zero hashes to 0.
    if o == 0:
        return 0
    cdef int sign = 1 if o > 0 else -1
    cdef object mag = -o if o < 0 else o
    chunks = []
    while mag > 0:
        chunks.append(int(mag & 0xFFFFFFFF))
        mag = mag >> 32
    chunks.reverse()
    cdef uint32_t h = 0u
    cdef uint32_t c
    for chunk in chunks:
        c = <uint32_t>chunk
        h = (31u * h + c) & 0xFFFFFFFFu
    if sign < 0:
        h = (0u - h) & 0xFFFFFFFFu
    return <int32_t>h
