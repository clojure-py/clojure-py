# Port of clojure.lang.Numbers.
#
# Java's Numbers is 4242 lines because it implements double-dispatch through
# Ops/BitOps classes for every (Long, Double, Ratio, BigInt, BigDecimal) pair.
# In Python we rely on the fact that int is already arbitrary-precision and
# that Python's operators handle most cross-type arithmetic; the per-type
# Op classes collapse to a small amount of inline isinstance dispatch here.
#
# What we still must do explicitly:
#   - Ratio arithmetic (handled in ratio.pxi via __add__/__sub__/etc.)
#   - Clojure division semantics (int/int → Ratio when not divisible)
#   - quotient / remainder with truncate-toward-zero (Python // floors)
#   - BigInt tag preservation when both operands are int-like
#   - BigDecimal + Double → Double (lossy, matches Java DoubleOps combine)
#   - Predicates (is_zero / is_pos / is_neg / is_nan / is_infinite)
#   - Bit ops as Java spells them

import math


cdef inline void _check_number(object x):
    if not Numbers._is_number(x):
        raise TypeError(f"not a number: {type(x).__name__}")


cdef inline void _check_int_like(object x):
    if isinstance(x, bool) or not isinstance(x, int):
        raise TypeError(f"bit operation requires integer, got {type(x).__name__}")


cdef inline bint _is_int_like(object x):
    return isinstance(x, int) and not isinstance(x, bool)


cdef class Numbers:
    """Static namespace mirroring clojure.lang.Numbers."""

    def __cinit__(self):
        raise TypeError("Numbers is a static namespace, not instantiable")

    # --- type tests ---

    @staticmethod
    cdef bint _is_number(object o) noexcept:
        if isinstance(o, bool):
            return False
        return isinstance(o, (int, float, Ratio, Decimal))

    @staticmethod
    def is_number(o):
        return Numbers._is_number(o)

    @staticmethod
    cdef int _category(object o) except -1:
        # 0 INTEGER, 1 FLOATING, 2 RATIO, 3 DECIMAL.
        if isinstance(o, bool):
            raise TypeError(f"not a number: {type(o).__name__}")
        if isinstance(o, int):
            return 0
        if isinstance(o, float):
            return 1
        if isinstance(o, Ratio):
            return 2
        if isinstance(o, Decimal):
            return 3
        raise TypeError(f"not a number: {type(o).__name__}")

    # --- equality ---

    @staticmethod
    cdef bint _equiv(object x, object y) except *:
        # Cross-type numeric equality. Caller has verified both are numbers.
        cdef Ratio rx, ry
        if isinstance(x, Ratio):
            rx = <Ratio>x
            if isinstance(y, Ratio):
                ry = <Ratio>y
                return rx.numerator * ry.denominator == ry.numerator * rx.denominator
            if _is_int_like(y):
                return rx.numerator == y * rx.denominator
            if isinstance(y, float):
                return rx.to_float() == y
            if isinstance(y, Decimal):
                return _ratio_to_decimal(rx) == y
            return False
        if isinstance(y, Ratio):
            return Numbers._equiv(y, x)
        # int vs int (covers BigInt, since BigInt subclasses int): compare values
        # (BigInt's structural __eq__ would say False otherwise).
        if _is_int_like(x) and _is_int_like(y):
            return int(x) == int(y)
        # Decimal cross-type: Decimal+Decimal compare by value (bypass BigDecimal's
        # structural __eq__); Decimal+float lossy; Decimal+int handled by Decimal.
        if isinstance(x, Decimal) and isinstance(y, Decimal):
            return Decimal.__eq__(x, y)
        if isinstance(x, Decimal) and isinstance(y, float):
            return float(x) == y
        if isinstance(x, float) and isinstance(y, Decimal):
            return x == float(y)
        if isinstance(x, Decimal) and _is_int_like(y):
            return Decimal.__eq__(x, Decimal(int(y)))
        if _is_int_like(x) and isinstance(y, Decimal):
            return Decimal.__eq__(Decimal(int(x)), y)
        return x == y

    @staticmethod
    def equiv(x, y):
        if not (Numbers._is_number(x) and Numbers._is_number(y)):
            return False
        return Numbers._equiv(x, y)

    @staticmethod
    def equal(x, y):
        if not (Numbers._is_number(x) and Numbers._is_number(y)):
            return False
        if Numbers._category(x) != Numbers._category(y):
            return False
        return Numbers._equiv(x, y)

    # --- comparison ---

    @staticmethod
    cdef bint _lt(object x, object y) except *:
        cdef Ratio rx, ry
        cdef object cross
        if isinstance(x, Ratio):
            rx = <Ratio>x
            if isinstance(y, Ratio):
                ry = <Ratio>y
                cross = rx.numerator * ry.denominator - ry.numerator * rx.denominator
                if rx.denominator * ry.denominator > 0:
                    return cross < 0
                return cross > 0
            if _is_int_like(y):
                cross = rx.numerator - y * rx.denominator
                if rx.denominator > 0:
                    return cross < 0
                return cross > 0
            if isinstance(y, float):
                return rx.to_float() < y
            if isinstance(y, Decimal):
                return _ratio_to_decimal(rx) < y
            raise TypeError(f"cannot compare Ratio to {type(y).__name__}")
        if isinstance(y, Ratio):
            ry = <Ratio>y
            if _is_int_like(x):
                cross = x * ry.denominator - ry.numerator
                if ry.denominator > 0:
                    return cross < 0
                return cross > 0
            if isinstance(x, float):
                return x < ry.to_float()
            if isinstance(x, Decimal):
                return x < _ratio_to_decimal(ry)
            raise TypeError(f"cannot compare {type(x).__name__} to Ratio")
        # Decimal+float must coerce (Python rejects direct comparison).
        if isinstance(x, Decimal) and isinstance(y, float):
            return float(x) < y
        if isinstance(x, float) and isinstance(y, Decimal):
            return x < float(y)
        return x < y

    @staticmethod
    def lt(x, y):
        _check_number(x); _check_number(y)
        return Numbers._lt(x, y)

    @staticmethod
    def lte(x, y):
        _check_number(x); _check_number(y)
        return not Numbers._lt(y, x)

    @staticmethod
    def gt(x, y):
        _check_number(x); _check_number(y)
        return Numbers._lt(y, x)

    @staticmethod
    def gte(x, y):
        _check_number(x); _check_number(y)
        return not Numbers._lt(x, y)

    @staticmethod
    def compare(x, y):
        if Numbers._lt(x, y):
            return -1
        if Numbers._lt(y, x):
            return 1
        return 0

    @staticmethod
    def max(x, y):
        _check_number(x); _check_number(y)
        return y if Numbers._lt(x, y) else x

    @staticmethod
    def min(x, y):
        _check_number(x); _check_number(y)
        return x if Numbers._lt(x, y) else y

    # --- predicates ---

    @staticmethod
    def is_zero(x):
        _check_number(x)
        if isinstance(x, Ratio):
            return (<Ratio>x).numerator == 0
        return x == 0

    @staticmethod
    def is_pos(x):
        _check_number(x)
        return Numbers._lt(0, x)

    @staticmethod
    def is_neg(x):
        _check_number(x)
        return Numbers._lt(x, 0)

    @staticmethod
    def is_nan(x):
        if isinstance(x, float):
            return x != x
        if isinstance(x, Decimal):
            return x.is_nan()
        return False

    @staticmethod
    def is_infinite(x):
        if isinstance(x, float):
            return math.isinf(x)
        if isinstance(x, Decimal):
            return x.is_infinite()
        return False

    # --- arithmetic ---

    @staticmethod
    cdef tuple _coerce_for_arith(object x, object y):
        # Python rejects Decimal + float; Java's combine() rule is that
        # BigDecimal + Double demotes to Double (lossy). Apply that coercion
        # before handing off to Python's `+` / `-` / `*`.
        if isinstance(x, Decimal) and isinstance(y, float):
            return (float(x), y)
        if isinstance(x, float) and isinstance(y, Decimal):
            return (x, float(y))
        return (x, y)

    @staticmethod
    def add(x, y):
        _check_number(x); _check_number(y)
        cx, cy = Numbers._coerce_for_arith(x, y)
        return Numbers._promote_arithmetic(x, y, cx + cy)

    @staticmethod
    def minus(x, y=None):
        _check_number(x)
        if y is None:
            return Numbers._negate(x)
        _check_number(y)
        cx, cy = Numbers._coerce_for_arith(x, y)
        return Numbers._promote_arithmetic(x, y, cx - cy)

    @staticmethod
    def multiply(x, y):
        _check_number(x); _check_number(y)
        cx, cy = Numbers._coerce_for_arith(x, y)
        return Numbers._promote_arithmetic(x, y, cx * cy)

    @staticmethod
    def divide(x, y):
        # Clojure / : int / int returns Ratio (or int when divisible).
        # float / x: float result. Ratio in either: Ratio path.
        _check_number(x); _check_number(y)
        if Numbers.is_zero(y):
            if isinstance(x, float) or isinstance(y, float):
                if Numbers.is_zero(x):
                    return float("nan")
                return float("inf") if Numbers.is_pos(x) else float("-inf")
            raise ZeroDivisionError("Divide by zero")
        if _is_int_like(x) and _is_int_like(y):
            # Reduce to int or Ratio. BigInt tag preserved if either was BigInt.
            result = _ratio_reduce(int(x), int(y))
            if isinstance(x, BigInt) or isinstance(y, BigInt):
                if _is_int_like(result) and not isinstance(result, BigInt):
                    return BigInt(int(result))
            return result
        # Python's / handles float/Decimal/Ratio combinations correctly via dunders.
        return x / y

    @staticmethod
    def quotient(x, y):
        # Truncate-toward-zero division (Java BigInteger.divide). Python's //
        # floors, which differs for mixed-sign operands.
        _check_number(x); _check_number(y)
        if Numbers.is_zero(y):
            raise ZeroDivisionError("Divide by zero")
        if isinstance(x, float) or isinstance(y, float):
            return float(int(x / y))
        if _is_int_like(x) and _is_int_like(y):
            return Numbers._trunc_div_int(int(x), int(y))
        # Ratio / Decimal: divide then truncate.
        q = x / y
        if isinstance(q, Ratio):
            return int(q)  # Ratio.__int__ truncates toward zero
        if isinstance(q, Decimal):
            return int(q.to_integral_value(rounding="ROUND_DOWN"))
        return int(q)

    @staticmethod
    def remainder(x, y):
        # Java IEEEremainder-style: x - quotient(x, y) * y. Sign follows dividend.
        _check_number(x); _check_number(y)
        if Numbers.is_zero(y):
            raise ZeroDivisionError("Divide by zero")
        q = Numbers.quotient(x, y)
        return Numbers.minus(x, Numbers.multiply(q, y))

    @staticmethod
    def abs(x):
        _check_number(x)
        return abs(x)

    @staticmethod
    def negate(x):
        _check_number(x)
        return Numbers._negate(x)

    @staticmethod
    cdef object _negate(object x):
        if isinstance(x, BigInt):
            return BigInt(-int(x))
        return -x

    @staticmethod
    def inc(x):
        _check_number(x)
        return Numbers._promote_arithmetic(x, 1, x + 1)

    @staticmethod
    def dec(x):
        _check_number(x)
        return Numbers._promote_arithmetic(x, 1, x - 1)

    # --- JVM auto-promoting / unchecked aliases ------------------------
    # On the JVM these are distinct from `add`/`multiply` etc. (the
    # checked ones throw on overflow, the P-suffixed ones promote to
    # BigInteger). Python ints don't overflow, so for our port the two
    # families collapse onto the same implementation. Kept as separate
    # names so the 1:1 core.clj translation finds the JVM names.

    @staticmethod
    def add_p(x, y):
        return Numbers.add(x, y)

    @staticmethod
    def multiply_p(x, y):
        return Numbers.multiply(x, y)

    @staticmethod
    def minus_p(x, y=None):
        return Numbers.minus(x, y)

    @staticmethod
    def inc_p(x):
        return Numbers.inc(x)

    @staticmethod
    def dec_p(x):
        return Numbers.dec(x)

    @staticmethod
    def unchecked_add(x, y):
        return Numbers.add(x, y)

    @staticmethod
    def unchecked_multiply(x, y):
        return Numbers.multiply(x, y)

    @staticmethod
    def unchecked_minus(x, y=None):
        return Numbers.minus(x, y)

    @staticmethod
    def unchecked_inc(x):
        return Numbers.inc(x)

    @staticmethod
    def unchecked_dec(x):
        return Numbers.dec(x)

    @staticmethod
    def int_cast(x):
        """Coerce x to a Python int. Mirrors JVM's RT.intCast."""
        if isinstance(x, bool):
            return 1 if x else 0
        if isinstance(x, int):
            return x
        if isinstance(x, float):
            return int(x)
        return int(x)

    @staticmethod
    def unchecked_int_cast(x):
        return Numbers.int_cast(x)

    # JVM has Long/Int/Short/Byte as separate fixed-width primitive types.
    # Python's int is arbitrary-precision, so all integer casts collapse to
    # int_cast. The named methods exist so the JVM source's
    # `RT/<X>Cast` lookups all resolve.
    @staticmethod
    def short_cast(x): return Numbers.int_cast(x)

    @staticmethod
    def byte_cast(x): return Numbers.int_cast(x)

    @staticmethod
    def unchecked_short_cast(x): return Numbers.int_cast(x)

    @staticmethod
    def unchecked_byte_cast(x): return Numbers.int_cast(x)

    @staticmethod
    def float_cast(x):
        """Coerce to Python float. JVM has Float (32-bit) and Double
        (64-bit) as distinct types; Python collapses both to float."""
        if isinstance(x, bool):
            return 1.0 if x else 0.0
        return float(x)

    @staticmethod
    def double_cast(x): return Numbers.float_cast(x)

    @staticmethod
    def unchecked_float_cast(x): return Numbers.float_cast(x)

    @staticmethod
    def unchecked_double_cast(x): return Numbers.float_cast(x)

    @staticmethod
    def char_cast(x):
        """Coerce to a 1-char Python str — Python's analog of JVM char.
        int input is treated as a Unicode codepoint."""
        if isinstance(x, str):
            if len(x) != 1:
                raise ValueError(
                    "char_cast: str must have length 1, got " + str(len(x)))
            return x
        if isinstance(x, bool):
            raise TypeError("char_cast: cannot coerce bool")
        if isinstance(x, int):
            return chr(x)
        raise TypeError(
            "char_cast: cannot coerce " + type(x).__name__)

    @staticmethod
    def unchecked_char_cast(x): return Numbers.char_cast(x)

    @staticmethod
    def num(x):
        """JVM Numbers.num — validate that x is a Number and return as-is.
        JVM treats Boolean as not-a-Number; we match that."""
        if isinstance(x, bool):
            raise TypeError("num: cannot coerce bool to Number")
        if Numbers._is_number(x):
            return x
        raise TypeError(
            "num: not a Number: " + type(x).__name__)

    # JVM has separate int (32-bit) variants of unchecked ops. Python
    # ints don't overflow so they all behave identically — keep the
    # JVM-named entry points so the 1:1 translation finds them.

    @staticmethod
    def unchecked_int_inc(x):
        return Numbers.inc(x)

    @staticmethod
    def unchecked_int_dec(x):
        return Numbers.dec(x)

    @staticmethod
    def unchecked_int_negate(x):
        return Numbers.minus(x)

    @staticmethod
    def unchecked_int_add(x, y):
        return Numbers.add(x, y)

    @staticmethod
    def unchecked_int_subtract(x, y):
        return Numbers.minus(x, y)

    @staticmethod
    def unchecked_int_multiply(x, y):
        return Numbers.multiply(x, y)

    @staticmethod
    def unchecked_int_divide(x, y):
        return Numbers.quotient(x, y)

    @staticmethod
    def unchecked_int_remainder(x, y):
        return Numbers.remainder(x, y)

    @staticmethod
    def unchecked_long_cast(x):
        """JVM RT.uncheckedLongCast — coerce to int (long)."""
        return Numbers.int_cast(x)

    @staticmethod
    def rationalize(x):
        """Return a rational equivalent of x (int, BigInt, or Ratio).
        Floats are converted via Python's Fraction → Ratio."""
        if isinstance(x, (int, BigInt, Ratio)):
            return x
        if isinstance(x, float):
            from fractions import Fraction
            f = Fraction(x).limit_denominator(10**18)
            return Ratio(f.numerator, f.denominator)
        if isinstance(x, Decimal):
            from fractions import Fraction
            f = Fraction(x)
            return Ratio(f.numerator, f.denominator)
        raise TypeError("Cannot rationalize " + type(x).__name__)

    # --- BigInt tag preservation ---

    @staticmethod
    cdef object _promote_arithmetic(object x, object y, object result):
        # Java promotion: BigInt + Long → BigInt, Long + Long → Long. We preserve
        # the BigInt tag whenever it appeared on either side AND the result is
        # still an int-ish thing (the result might already be a wider type, e.g.
        # float, in which case we leave it alone).
        if isinstance(result, BigInt):
            return result
        if _is_int_like(result):
            if isinstance(x, BigInt) or isinstance(y, BigInt):
                return BigInt(int(result))
        return result

    # --- truncate-toward-zero division on ints ---

    @staticmethod
    cdef object _trunc_div_int(object a, object b):
        cdef object q, r
        q, r = divmod(a, b)
        if r != 0 and (a < 0) != (b < 0):
            q += 1
        return q

    # --- bit ops ---
    #
    # Java BitOps requires Long-cast operands. Python int handles arbitrary
    # precision natively, so we just use Python operators after a cheap
    # type-check that the operand is an integer (and not a bool).

    @staticmethod
    def bit_and(x, y):
        _check_int_like(x); _check_int_like(y)
        return int(x) & int(y)

    @staticmethod
    def bit_or(x, y):
        _check_int_like(x); _check_int_like(y)
        return int(x) | int(y)

    @staticmethod
    def bit_xor(x, y):
        _check_int_like(x); _check_int_like(y)
        return int(x) ^ int(y)

    @staticmethod
    def bit_not(x):
        _check_int_like(x)
        return ~int(x)

    @staticmethod
    def bit_and_not(x, y):
        _check_int_like(x); _check_int_like(y)
        return int(x) & ~int(y)

    @staticmethod
    def bit_clear(x, n):
        _check_int_like(x); _check_int_like(n)
        return int(x) & ~(1 << int(n))

    @staticmethod
    def bit_set(x, n):
        _check_int_like(x); _check_int_like(n)
        return int(x) | (1 << int(n))

    @staticmethod
    def bit_flip(x, n):
        _check_int_like(x); _check_int_like(n)
        return int(x) ^ (1 << int(n))

    @staticmethod
    def bit_test(x, n):
        _check_int_like(x); _check_int_like(n)
        return ((int(x) >> int(n)) & 1) != 0

    @staticmethod
    def shift_left(x, n):
        _check_int_like(x); _check_int_like(n)
        return int(x) << int(n)

    @staticmethod
    def shift_right(x, n):
        _check_int_like(x); _check_int_like(n)
        return int(x) >> int(n)  # Python: arithmetic shift for negatives

    @staticmethod
    def unsigned_shift_right(x, n):
        # Java's >>> on long: take the 64-bit two's-complement bit pattern,
        # logical-shift right. For positive x we degenerate to >>; for negative
        # we wrap into the unsigned 64-bit space first.
        _check_int_like(x); _check_int_like(n)
        cdef object xv = int(x)
        cdef int nn = int(n) & 63
        if xv >= 0:
            return xv >> nn
        return ((xv + (1 << 64)) & 0xFFFFFFFFFFFFFFFF) >> nn

    @staticmethod
    def bit_count(x):
        # Java Long.bitCount: number of 1-bits in the 64-bit two's-complement
        # representation. For consistency with Python int.bit_count, we operate
        # on the value directly (works for any size).
        _check_int_like(x)
        return int(x).bit_count()

    # --- hash ---

    @staticmethod
    cdef int32_t _hasheq(object o) except *:
        cdef long long lv
        cdef Ratio r
        if isinstance(o, BigInt):
            return (<object>o).hasheq()
        if isinstance(o, BigDecimal):
            return (<object>o).hasheq()
        if isinstance(o, int):
            if -(1 << 63) <= o < (1 << 63):
                lv = o
                return Murmur3._hash_long(lv)
            return _big_integer_hashcode(o)
        if isinstance(o, float):
            if o == 0.0:
                return 0
            return _double_hashcode(o)
        if isinstance(o, Ratio):
            r = <Ratio>o
            return _big_integer_hashcode(r.numerator) ^ _big_integer_hashcode(r.denominator)
        if isinstance(o, Decimal):
            # Approximate: Python Decimal hash on normalized form. Not bit-exact
            # JVM but consistent within our runtime.
            if Decimal.__eq__(o, 0):
                return _to_int32_mask(Decimal.__hash__(Decimal(0)))
            return _to_int32_mask(Decimal.__hash__(o.normalize()))
        return _to_int32_mask(hash(o))

    @staticmethod
    def hasheq(o):
        return Numbers._hasheq(o)

    # --- typed array constructors (back clojure.core's int-array etc) ---
    #
    # JVM's Numbers.int_array / float_array / etc. each accept either:
    #   (size_or_seq)              — int N → zeros of length N;
    #                                 seq    → array sized to seq.
    #   (size, init_val_or_seq)    — int + Number → fill with value;
    #                                 int + seq    → prefix from seq, rest 0.

    @staticmethod
    def _build_typed_array(typecode, init_default, coerce, args):
        import array as _array_mod
        if typecode is None:
            empty_arr = lambda n: [init_default] * n
            wrap = lambda items: list(items)
        else:
            empty_arr = lambda n: _array_mod.array(typecode, [init_default] * n)
            wrap = lambda items: _array_mod.array(typecode, items)

        if len(args) == 1:
            x = args[0]
            if isinstance(x, int) and not isinstance(x, bool):
                return empty_arr(x)
            # Treat as seq.
            items = []
            s = RT.seq(x) if x is not None else None
            while s is not None:
                items.append(coerce(s.first()))
                s = s.next()
            return wrap(items)

        if len(args) == 2:
            size, init = args
            if not isinstance(size, int) or isinstance(size, bool):
                raise TypeError(
                    "array size must be int, got " + type(size).__name__)
            # Single value if it's a number/str/bool-as-init; else seq.
            if isinstance(init, bool):
                # Bool is also int — handle first to avoid misclassification.
                return wrap([coerce(init)] * size)
            if isinstance(init, (int, float, Decimal, Ratio, str)):
                return wrap([coerce(init)] * size)
            # Treat as seq, prefix-fill, rest = init_default.
            items = [init_default] * size
            s = RT.seq(init) if init is not None else None
            i = 0
            while s is not None and i < size:
                items[i] = coerce(s.first())
                s = s.next()
                i += 1
            return wrap(items)

        raise TypeError(
            "typed array factory takes 1 or 2 args, got " + str(len(args)))

    @staticmethod
    def int_array(*args):
        return Numbers._build_typed_array("q", 0, int, args)

    @staticmethod
    def long_array(*args):
        return Numbers._build_typed_array("q", 0, int, args)

    @staticmethod
    def short_array(*args):
        return Numbers._build_typed_array("h", 0, int, args)

    @staticmethod
    def byte_array(*args):
        return Numbers._build_typed_array("b", 0, int, args)

    @staticmethod
    def float_array(*args):
        return Numbers._build_typed_array("f", 0.0, float, args)

    @staticmethod
    def double_array(*args):
        return Numbers._build_typed_array("d", 0.0, float, args)

    @staticmethod
    def boolean_array(*args):
        # Python's array module has no native bool typecode; use a list.
        return Numbers._build_typed_array(None, False, bool, args)

    @staticmethod
    def char_array(*args):
        # Python char = 1-char str. Coerce int → chr, 1-char str → as-is.
        def _coerce_char(v):
            if isinstance(v, str):
                if len(v) != 1:
                    raise ValueError(
                        "char_array element must be 1-char str, got len "
                        + str(len(v)))
                return v
            if isinstance(v, bool):
                raise TypeError("char_array: cannot coerce bool")
            if isinstance(v, int):
                return chr(v)
            raise TypeError(
                "char_array: cannot coerce " + type(v).__name__)
        # Use a list — Python's 'u' typecode is deprecated as of 3.16.
        return Numbers._build_typed_array(None, "\x00", _coerce_char, args)
