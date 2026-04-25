"""Property-based fuzzing of Decimal↔Float `==` symmetry."""

from decimal import Decimal

from hypothesis import given, strategies as st, assume
from clojure._core import eval_string as e


ne = e("==")


@given(f=st.floats(min_value=-1e10, max_value=1e10,
                   allow_nan=False, allow_infinity=False, allow_subnormal=False))
def test_decimal_from_str_float_equiv_float(f):
    """Decimal(str(f)) == f for every finite float f.

    `Decimal(str(f))` round-trips through f64 cleanly: `float(Decimal(str(f))) == f`.
    Therefore the f64-promotion path returns true symmetrically.
    """
    d = Decimal(str(f))
    # Sanity: float(d) should round-trip to f.
    assert float(d) == f
    assert ne(d, f) is True
    assert ne(f, d) is True


@given(
    n=st.integers(min_value=-(10**10), max_value=10**10),
    f=st.floats(min_value=-1e10, max_value=1e10,
                allow_nan=False, allow_infinity=False, allow_subnormal=False),
)
def test_decimal_int_distinct_from_unrelated_float(n, f):
    """Decimal-from-int compared with an unrelated float — answer matches int↔float."""
    assume(float(n) != f)
    d = Decimal(n)
    expected = (n == f) if isinstance(f, float) else (d == f)
    assert ne(d, f) is bool(expected)
