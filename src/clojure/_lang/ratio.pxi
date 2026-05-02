# Port of clojure.lang.Ratio.
#
# Stores numerator and denominator as Python ints (arbitrary precision —
# Python int substitutes for Java BigInteger). Construction does not reduce;
# arithmetic results are reduced and sign-normalized (denominator made
# positive) via _ratio_reduce.

# Decimal is imported at the top of lang.pyx so all .pxi files share it.


cdef object _python_gcd(object a, object b):
    while b != 0:
        a, b = b, a % b
    return a


cdef object _ratio_reduce(object num, object den):
    """Reduce num/den. Returns plain int when den=1, else a sign-normalized
    Ratio (denominator positive). Caller has already checked den != 0."""
    if num == 0:
        return 0
    cdef object g = _python_gcd(abs(num), abs(den))
    num = num // g
    den = den // g
    if den < 0:
        num = -num
        den = -den
    if den == 1:
        return num
    return Ratio(num, den)


cdef object _ratio_to_decimal(Ratio r):
    return Decimal(r.numerator) / Decimal(r.denominator)


cdef class Ratio:
    """An exact ratio of two arbitrary-precision integers."""

    cdef readonly object numerator
    cdef readonly object denominator

    def __cinit__(self, numerator, denominator):
        if isinstance(numerator, bool) or not isinstance(numerator, int):
            raise TypeError(f"Ratio numerator must be int, got {type(numerator).__name__}")
        if isinstance(denominator, bool) or not isinstance(denominator, int):
            raise TypeError(f"Ratio denominator must be int, got {type(denominator).__name__}")
        if denominator == 0:
            raise ZeroDivisionError("Ratio denominator must be nonzero")
        self.numerator = numerator
        self.denominator = denominator

    def __str__(self):
        return f"{self.numerator}/{self.denominator}"

    def __repr__(self):
        return self.__str__()

    def __hash__(self):
        return _big_integer_hashcode(self.numerator) ^ _big_integer_hashcode(self.denominator)

    def hasheq(self):
        return self.__hash__()

    def __eq__(self, other):
        # Component-wise structural equality. Numbers.equiv handles cross-type
        # value equality (e.g. 1/2 == 2/4) separately.
        if self is other:
            return True
        if not isinstance(other, Ratio):
            return False
        cdef Ratio r = <Ratio>other
        return self.numerator == r.numerator and self.denominator == r.denominator

    def __ne__(self, other):
        return not self.__eq__(other)

    def to_float(self):
        return self.numerator / self.denominator

    def __float__(self):
        return self.to_float()

    def __int__(self):
        # Truncate toward zero (Java BigInteger.divide).
        cdef object n = self.numerator
        cdef object d = self.denominator
        cdef object q, r
        q, r = divmod(n, d)
        if r != 0 and (n < 0) != (d < 0):
            q += 1
        return q

    def __bool__(self):
        return self.numerator != 0

    def __neg__(self):
        return Ratio(-self.numerator, self.denominator)

    def __pos__(self):
        return self

    def __abs__(self):
        return Ratio(abs(self.numerator), abs(self.denominator))

    # --- arithmetic dunders ---

    def __add__(self, other):
        cdef Ratio ro
        if isinstance(other, Ratio):
            ro = <Ratio>other
            return _ratio_reduce(
                self.numerator * ro.denominator + ro.numerator * self.denominator,
                self.denominator * ro.denominator)
        if isinstance(other, int) and not isinstance(other, bool):
            return _ratio_reduce(
                self.numerator + other * self.denominator, self.denominator)
        if isinstance(other, float):
            return self.to_float() + other
        if isinstance(other, Decimal):
            return _ratio_to_decimal(self) + other
        return NotImplemented

    def __radd__(self, other):
        return self.__add__(other)

    def __sub__(self, other):
        cdef Ratio ro
        if isinstance(other, Ratio):
            ro = <Ratio>other
            return _ratio_reduce(
                self.numerator * ro.denominator - ro.numerator * self.denominator,
                self.denominator * ro.denominator)
        if isinstance(other, int) and not isinstance(other, bool):
            return _ratio_reduce(
                self.numerator - other * self.denominator, self.denominator)
        if isinstance(other, float):
            return self.to_float() - other
        if isinstance(other, Decimal):
            return _ratio_to_decimal(self) - other
        return NotImplemented

    def __rsub__(self, other):
        if isinstance(other, int) and not isinstance(other, bool):
            return _ratio_reduce(
                other * self.denominator - self.numerator, self.denominator)
        if isinstance(other, float):
            return other - self.to_float()
        if isinstance(other, Decimal):
            return other - _ratio_to_decimal(self)
        return NotImplemented

    def __mul__(self, other):
        cdef Ratio ro
        if isinstance(other, Ratio):
            ro = <Ratio>other
            return _ratio_reduce(
                self.numerator * ro.numerator,
                self.denominator * ro.denominator)
        if isinstance(other, int) and not isinstance(other, bool):
            return _ratio_reduce(self.numerator * other, self.denominator)
        if isinstance(other, float):
            return self.to_float() * other
        if isinstance(other, Decimal):
            return _ratio_to_decimal(self) * other
        return NotImplemented

    def __rmul__(self, other):
        return self.__mul__(other)

    def __truediv__(self, other):
        cdef Ratio ro
        if isinstance(other, Ratio):
            ro = <Ratio>other
            if ro.numerator == 0:
                raise ZeroDivisionError("Ratio division by zero")
            return _ratio_reduce(
                self.numerator * ro.denominator,
                self.denominator * ro.numerator)
        if isinstance(other, int) and not isinstance(other, bool):
            if other == 0:
                raise ZeroDivisionError("division by zero")
            return _ratio_reduce(self.numerator, self.denominator * other)
        if isinstance(other, float):
            if other == 0.0:
                raise ZeroDivisionError("division by zero")
            return self.to_float() / other
        if isinstance(other, Decimal):
            if other == 0:
                raise ZeroDivisionError("division by zero")
            return _ratio_to_decimal(self) / other
        return NotImplemented

    def __rtruediv__(self, other):
        if self.numerator == 0:
            raise ZeroDivisionError("division by zero")
        if isinstance(other, int) and not isinstance(other, bool):
            return _ratio_reduce(other * self.denominator, self.numerator)
        if isinstance(other, float):
            return other / self.to_float()
        if isinstance(other, Decimal):
            return other / _ratio_to_decimal(self)
        return NotImplemented

    def __pow__(self, exp, modulo=None):
        if modulo is not None:
            return NotImplemented
        if not (isinstance(exp, int) and not isinstance(exp, bool)):
            # Non-integer exponent → fall back to float
            return self.to_float() ** exp
        if exp >= 0:
            return _ratio_reduce(self.numerator ** exp, self.denominator ** exp)
        if self.numerator == 0:
            raise ZeroDivisionError("0 raised to a negative power")
        # exp < 0: invert
        return _ratio_reduce(self.denominator ** (-exp), self.numerator ** (-exp))

    def __lt__(self, other):
        return Numbers.compare(self, other) < 0

    def __le__(self, other):
        return Numbers.compare(self, other) <= 0

    def __gt__(self, other):
        return Numbers.compare(self, other) > 0

    def __ge__(self, other):
        return Numbers.compare(self, other) >= 0
