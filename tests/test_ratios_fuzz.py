"""Property-based fuzzing of Clojure ratios — read/print round-trip,
normalize-to-int, equiv-vs-num-equiv, arithmetic invariants.
"""

from fractions import Fraction
import pytest
from hypothesis import given, strategies as st, assume

from clojure._core import (
    eval as _eval, read_string, pr_str,
    equiv,
)

# `num_equiv` is Clojure's `==` — not exported from _core, so resolve once.
num_equiv = _eval(read_string("=="))


# ---------- strategies ----------

# Ints bounded but covering negatives, zero, and a few-digit values.
ints = st.integers(min_value=-10_000, max_value=10_000)

# Nonzero ints for denominators.
nonzero_ints = ints.filter(lambda n: n != 0)

# Reasonable-magnitude floats, excluding NaN (NaN comparisons break property
# reasoning). Excludes infinities for the same reason on round-trip props.
finite_floats = st.floats(
    min_value=-1e6, max_value=1e6,
    allow_nan=False, allow_infinity=False,
    allow_subnormal=False,
)


def _ratio_str(n: int, d: int) -> str:
    # Build a Clojure-readable ratio literal. Sign rides on numerator; reader
    # rejects sign on denominator and we'd hit ReaderError, so we always put
    # the sign on n.
    if d < 0:
        n, d = -n, -d
    return f"{n}/{d}"


# ---------- properties ----------

@given(n=ints, d=nonzero_ints)
def test_reader_produces_canonical_form(n, d):
    """Reader normalizes to lowest terms; if denominator becomes 1, return int."""
    src = _ratio_str(n, d)
    val = _eval(read_string(src))
    expected = Fraction(n, d)
    if expected.denominator == 1:
        assert val == expected.numerator
        assert type(val) is int
    else:
        assert val == expected
        assert type(val) is Fraction


@given(n=ints, d=nonzero_ints)
def test_print_round_trip(n, d):
    """`(read-string (pr-str x))` == x for every Ratio."""
    src = _ratio_str(n, d)
    val = _eval(read_string(src))
    printed = pr_str(val)
    round = _eval(read_string(printed))
    assert round == val


@given(n=ints, d=nonzero_ints)
def test_pr_str_reduced_form(n, d):
    """pr-str of a non-whole Ratio is `{numerator}/{denominator}` in reduced form."""
    expected = Fraction(n, d)
    if expected.denominator == 1:
        # whole — pr_str should be the int form
        assert pr_str(expected.numerator) == str(expected.numerator)
    else:
        # exact reduced "n/d"
        assert pr_str(expected) == f"{expected.numerator}/{expected.denominator}"


@given(a_n=ints, a_d=nonzero_ints, b_n=ints, b_d=nonzero_ints)
def test_equiv_ratio_vs_ratio(a_n, a_d, b_n, b_d):
    """`(= 1/2 1/2)` true iff Fractions equal; never crosses categories."""
    a = Fraction(a_n, a_d)
    b = Fraction(b_n, b_d)
    py_eq = (a == b)
    cl_eq = equiv(a, b)
    assert cl_eq is py_eq


@given(n=ints, d=nonzero_ints, f=finite_floats)
def test_equiv_ratio_vs_float_always_false(n, d, f):
    """Vanilla rule: Ratio != Double under `=`, regardless of numeric value."""
    a = Fraction(n, d)
    if a.denominator == 1:
        # Ratio reduces to int — that's not the case we test here.
        return
    assert equiv(a, f) is False
    assert equiv(f, a) is False


@given(n=ints, d=nonzero_ints, f=finite_floats)
def test_num_equiv_ratio_vs_float_matches_float_promotion(n, d, f):
    """`==` between Ratio and Float matches JVM Numbers.equiv: promote ratio->f64."""
    a = Fraction(n, d)
    if a.denominator == 1:
        return
    expected = (float(a) == f)
    assert num_equiv(a, f) is expected
    assert num_equiv(f, a) is expected


@given(n1=ints, d1=nonzero_ints, n2=ints, d2=nonzero_ints)
def test_arithmetic_addition_commutative(n1, d1, n2, d2):
    """(+ a b) = (+ b a) for ratios."""
    a = Fraction(n1, d1)
    b = Fraction(n2, d2)
    expected = a + b
    # Python comparison handles auto-reduce. Our + should produce same value.
    src1 = f"(+ {_ratio_str(n1, d1)} {_ratio_str(n2, d2)})"
    src2 = f"(+ {_ratio_str(n2, d2)} {_ratio_str(n1, d1)})"
    r1 = _eval(read_string(src1))
    r2 = _eval(read_string(src2))
    assert r1 == r2 == expected


@given(n1=ints, d1=nonzero_ints, n2=ints, d2=nonzero_ints, n3=ints, d3=nonzero_ints)
def test_arithmetic_addition_associative(n1, d1, n2, d2, n3, d3):
    """(+ a (+ b c)) = (+ (+ a b) c) for ratios."""
    a = _ratio_str(n1, d1)
    b = _ratio_str(n2, d2)
    c = _ratio_str(n3, d3)
    left  = _eval(read_string(f"(+ {a} (+ {b} {c}))"))
    right = _eval(read_string(f"(+ (+ {a} {b}) {c})"))
    assert left == right


@given(n1=ints, d1=nonzero_ints, n2=ints, d2=nonzero_ints)
def test_arithmetic_subtract_inverse(n1, d1, n2, d2):
    """(- (+ a b) b) = a."""
    a = _ratio_str(n1, d1)
    b = _ratio_str(n2, d2)
    src = f"(- (+ {a} {b}) {b})"
    result = _eval(read_string(src))
    expected = Fraction(n1, d1)
    if expected.denominator == 1:
        assert result == expected.numerator
    else:
        assert result == expected


@given(n=ints, d=nonzero_ints)
def test_normalize_collapses_denominator_one(n, d):
    """Whenever the reduced form has denominator 1, the runtime hands back an int (not Fraction(n,1))."""
    expected = Fraction(n, d)
    if expected.denominator == 1:
        # Multiple paths that should produce a whole number from a ratio:
        # 1. Reader literal:
        from_reader = _eval(read_string(_ratio_str(n, d)))
        assert type(from_reader) is int
        # 2. Arithmetic: `(+ n/d 0)` should also collapse.
        from_add = _eval(read_string(f"(+ {_ratio_str(n, d)} 0)"))
        assert type(from_add) is int


@given(n=ints, d=nonzero_ints)
def test_hash_respects_equality(n, d):
    """`(= a b) => (hash a) == (hash b)`. For Ratio that reduces to int, hashes match."""
    src = _ratio_str(n, d)
    val = _eval(read_string(src))
    expected = Fraction(n, d)
    if expected.denominator == 1:
        assert hash(val) == hash(expected.numerator)
    # No assertion when not reducing to int — Python handles Fraction's hash.


@given(n=ints, d=nonzero_ints)
def test_negation(n, d):
    """`(- 0 x)` == `(- x)` (already true at the language level for unary -)."""
    src = _ratio_str(n, d)
    a = _eval(read_string(f"(- {src})"))
    b = _eval(read_string(f"(- 0 {src})"))
    assert a == b


@given(n=ints, d=nonzero_ints, k=ints)
def test_multiplication_by_int_is_consistent(n, d, k):
    """`(* (/ n d) k)` matches Python Fraction(n, d) * k."""
    src = f"(* {_ratio_str(n, d)} {k})"
    result = _eval(read_string(src))
    expected = Fraction(n, d) * k
    if isinstance(expected, Fraction) and expected.denominator == 1:
        assert result == expected.numerator
        assert type(result) is int
    else:
        assert result == expected
