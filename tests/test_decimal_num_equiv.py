"""Decimal `==` symmetry — Decimal↔Float widens to f64 like Ratio↔Float."""

from decimal import Decimal
from fractions import Fraction
from clojure._core import eval_string as e


# The Clojure `==` fn, resolved once at import time.
ne = e("==")


# ---------- The broken case (now fixed) ----------

def test_decimal_vs_float_uses_f64_promotion():
    # Vanilla: Decimal("0.1") == 0.1 because both widen via doubleValue.
    assert ne(Decimal("0.1"), 0.1) is True
    assert ne(0.1, Decimal("0.1")) is True


def test_decimal_vs_float_drift_still_distinguishes():
    # 0.1 + 0.2 != 0.3 in f64; Decimal("0.3") promoted to f64 is 0.3f64
    # which differs from 0.3000…0004. So returns false. Vanilla agrees.
    assert ne(Decimal("0.3"), 0.1 + 0.2) is False


# ---------- Regression: previously-working cases stay working ----------

def test_decimal_vs_int_unchanged():
    assert ne(Decimal("1"), 1) is True
    assert ne(Decimal("1.0"), 1) is True
    assert ne(Decimal("0"), 0) is True
    assert ne(1, Decimal(10**40)) is False


def test_decimal_vs_ratio_unchanged():
    assert ne(Decimal("0.5"), Fraction(1, 2)) is True
    assert ne(Decimal("1"), Fraction(1, 1)) is True   # Fraction reduces to int
    assert ne(Decimal("0.5"), Fraction(1, 3)) is False


def test_decimal_vs_decimal_unchanged():
    assert ne(Decimal("1.0"), Decimal("1.0")) is True
    assert ne(Decimal("1.0"), Decimal("2.0")) is False


def test_baseline_unchanged():
    assert ne(1, 1) is True
    assert ne(1, 2) is False
    assert ne(1.0, 1) is True
    assert ne(Fraction(1, 2), 0.5) is True
    assert ne(0.5, Fraction(1, 2)) is True
