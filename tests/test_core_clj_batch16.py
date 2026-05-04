"""Tests for core.clj batch 16 (lines 3503-3689):
numeric coercions + Number/Ratio/BigInt/BigDecimal predicates and coercions.

Forms (25):
  num, long, float, double, short, byte, char,
  unchecked-byte, unchecked-short, unchecked-char,
  unchecked-int, unchecked-long, unchecked-float, unchecked-double,
  number?, mod,
  ratio?, numerator, denominator,
  decimal?, float?, rational?,
  bigint, biginteger, bigdec

Backend additions (Numbers + RT):
  num,
  short_cast, byte_cast, float_cast, double_cast, char_cast,
  unchecked_short_cast, unchecked_byte_cast, unchecked_float_cast,
  unchecked_double_cast, unchecked_char_cast.

Adaptations from JVM source:
  number?  body uses Numbers/is_number rather than (instance? Number x)
           because Python's numbers.Number ABC includes bool and JVM's
           Number does not.
  bigint   collapses JVM's per-type branches (.toBigInteger, valueOf,
           .bigIntegerValue) into a single (number? x) branch — Python
           ints handle arbitrary precision and `long` truncates uniformly.
  bigdec   ratio path uses Decimal/Decimal division (default 28-digit
           context); JVM uses exact-precision and would throw on
           non-terminating expansions like 1/3.

dotimes redef now uses (long ~n) matching JVM source — `long` is
finally defined as part of this batch.
"""

import pytest

import clojure.core  # triggers load

from clojure.lang import (
    Compiler,
    read_string,
    BigInt, BigDecimal, Ratio,
    Numbers, RT,
    Var, Symbol,
)


def E(src):
    return Compiler.eval(read_string(src))


# --- num ----------------------------------------------------------

def test_num_passes_through_int():
    assert E("(clojure.core/num 5)") == 5

def test_num_passes_through_float():
    assert E("(clojure.core/num 1.5)") == 1.5

def test_num_passes_through_ratio():
    out = E("(clojure.core/num (clojure.core// 1 3))")
    assert isinstance(out, Ratio)

def test_num_rejects_bool():
    """JVM's Number excludes Boolean — match that."""
    with pytest.raises(TypeError):
        E("(clojure.core/num true)")

def test_num_rejects_keyword():
    with pytest.raises(TypeError):
        E("(clojure.core/num :a)")


# --- long / float / double / int / short / byte ------------------

def test_long_truncates_float():
    assert E("(clojure.core/long 3.7)") == 3
    assert E("(clojure.core/long -3.7)") == -3

def test_long_int_passthrough():
    assert E("(clojure.core/long 42)") == 42

def test_float_int_to_float():
    out = E("(clojure.core/float 3)")
    assert out == 3.0
    assert isinstance(out, float)

def test_double_same_as_float():
    """Python collapses Float and Double into one type."""
    assert E("(clojure.core/double 3)") == E("(clojure.core/float 3)")

def test_short_byte_collapse_to_int():
    """No fixed-width primitives in Python — all int casts collapse."""
    assert E("(clojure.core/short 300)") == 300  # JVM would overflow; Python int doesn't
    assert E("(clojure.core/byte 5)") == 5

def test_long_rejects_bool_via_int_cast():
    """RT.long_cast (= Numbers.int_cast) accepts bool as 0/1."""
    assert RT.long_cast(True) == 1
    assert RT.long_cast(False) == 0


# --- char --------------------------------------------------------

def test_char_from_codepoint():
    assert E("(clojure.core/char 65)") == "A"

def test_char_from_str_passthrough():
    assert E('(clojure.core/char "X")') == "X"

def test_char_rejects_multichar_str():
    with pytest.raises(ValueError):
        E('(clojure.core/char "AB")')

def test_char_rejects_bool():
    with pytest.raises(TypeError):
        E("(clojure.core/char true)")


# --- unchecked-* aliases -----------------------------------------

def test_unchecked_int_equals_long():
    """In Python all integer casts collapse — verify they agree on a
    sample input."""
    assert E("(clojure.core/unchecked-int 42.7)") == E("(clojure.core/long 42.7)")

def test_unchecked_long_equals_long():
    assert E("(clojure.core/unchecked-long -7)") == -7

def test_unchecked_byte_short_int():
    for fn in ("unchecked-byte", "unchecked-short", "unchecked-int", "unchecked-long"):
        assert E(f"(clojure.core/{fn} 5)") == 5

def test_unchecked_float_double():
    assert E("(clojure.core/unchecked-float 3)") == 3.0
    assert E("(clojure.core/unchecked-double 3)") == 3.0

def test_unchecked_char():
    assert E("(clojure.core/unchecked-char 66)") == "B"


# --- number? -----------------------------------------------------

def test_number_true_for_int():
    assert E("(clojure.core/number? 5)") is True

def test_number_true_for_float():
    assert E("(clojure.core/number? 5.0)") is True

def test_number_true_for_ratio():
    assert E("(clojure.core/number? (clojure.core// 1 3))") is True

def test_number_true_for_bigdec():
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb16-bd"), BigDecimal("1.5"))
    assert E("(clojure.core/number? user/tcb16-bd)") is True

def test_number_false_for_keyword():
    assert E("(clojure.core/number? :a)") is False

def test_number_false_for_bool():
    """Match JVM: Boolean is not a Number."""
    assert E("(clojure.core/number? true)") is False
    assert E("(clojure.core/number? false)") is False

def test_number_false_for_nil():
    assert E("(clojure.core/number? nil)") is False


# --- mod ---------------------------------------------------------

def test_mod_positive():
    assert E("(clojure.core/mod 10 3)") == 1

def test_mod_truncates_toward_neg_inf():
    """mod differs from rem on mixed-sign — floors toward -inf."""
    assert E("(clojure.core/mod -10 3)") == 2
    assert E("(clojure.core/mod 10 -3)") == -2
    assert E("(clojure.core/mod -10 -3)") == -1

def test_mod_zero_dividend():
    assert E("(clojure.core/mod 0 5)") == 0

def test_mod_by_one():
    assert E("(clojure.core/mod 7 1)") == 0


# --- ratio? / numerator / denominator ----------------------------

def test_ratio_true_for_ratio():
    assert E("(clojure.core/ratio? (clojure.core// 1 3))") is True

def test_ratio_false_for_int():
    assert E("(clojure.core/ratio? 1)") is False

def test_ratio_false_for_float():
    assert E("(clojure.core/ratio? 1.5)") is False

def test_numerator():
    assert E("(clojure.core/numerator (clojure.core// 22 7))") == 22

def test_denominator():
    assert E("(clojure.core/denominator (clojure.core// 22 7))") == 7

def test_numerator_negative():
    """Ratio normalizes sign onto numerator."""
    out = E("(clojure.core/numerator (clojure.core// -3 7))")
    assert out == -3
    assert E("(clojure.core/denominator (clojure.core// -3 7))") == 7


# --- decimal? / float? / rational? -------------------------------

def test_decimal_true_for_bigdecimal():
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb16-d"), BigDecimal("3.14"))
    assert E("(clojure.core/decimal? user/tcb16-d)") is True

def test_decimal_false_for_python_float():
    assert E("(clojure.core/decimal? 3.14)") is False

def test_decimal_false_for_int():
    assert E("(clojure.core/decimal? 1)") is False

def test_float_pred_true_for_python_float():
    assert E("(clojure.core/float? 3.14)") is True

def test_float_pred_false_for_int():
    assert E("(clojure.core/float? 1)") is False

def test_float_pred_false_for_ratio():
    assert E("(clojure.core/float? (clojure.core// 1 3))") is False

def test_rational_int():
    assert E("(clojure.core/rational? 1)") is True

def test_rational_ratio():
    assert E("(clojure.core/rational? (clojure.core// 1 3))") is True

def test_rational_bigdec():
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb16-rd"), BigDecimal("1.5"))
    assert E("(clojure.core/rational? user/tcb16-rd)") is True

def test_rational_false_for_python_float():
    """Python's float is not rational (matches JVM Double)."""
    assert E("(clojure.core/rational? 1.5)") is False


# --- bigint ------------------------------------------------------

def test_bigint_int():
    out = E("(clojure.core/bigint 5)")
    assert isinstance(out, BigInt)
    assert out == BigInt(5)

def test_bigint_already_bigint_passthrough():
    """First cond branch returns x as-is."""
    out = E("(clojure.core/bigint (clojure.core/bigint 7))")
    assert isinstance(out, BigInt)
    assert out == BigInt(7)

def test_bigint_float_truncates():
    out = E("(clojure.core/bigint 3.9)")
    assert out == BigInt(3)

def test_bigint_negative_float_truncates_toward_zero():
    out = E("(clojure.core/bigint -3.9)")
    assert out == BigInt(-3)

def test_bigint_ratio_truncates():
    out = E("(clojure.core/bigint (clojure.core// 22 7))")
    # 22/7 = 3.14...; truncate = 3
    assert out == BigInt(3)

def test_bigint_from_bigdec():
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb16-big"), BigDecimal("123.99"))
    out = E("(clojure.core/bigint user/tcb16-big)")
    assert out == BigInt(123)


# --- biginteger --------------------------------------------------

def test_biginteger_aliases_bigint():
    """clojure-py aliases BigInteger to BigInt."""
    out = E("(clojure.core/biginteger 5)")
    assert isinstance(out, BigInt)


# --- bigdec ------------------------------------------------------

def test_bigdec_int():
    out = E("(clojure.core/bigdec 5)")
    assert isinstance(out, BigDecimal)
    assert out == BigDecimal("5")

def test_bigdec_already_bigdec_passthrough():
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb16-bd2"), BigDecimal("1.5"))
    out = E("(clojure.core/bigdec user/tcb16-bd2)")
    assert isinstance(out, BigDecimal)

def test_bigdec_float_via_string():
    """0.1 round-trips through str(float) so we get a "0.1" BigDecimal,
    not the exact-binary 0.1000000000000000055511...."""
    out = E("(clojure.core/bigdec 0.1)")
    assert out == BigDecimal("0.1")

def test_bigdec_ratio_default_precision():
    """Non-terminating expansion. JVM throws; we use Python's default
    Decimal context (28 digits) and produce a finite approximation."""
    out = E("(clojure.core/bigdec (clojure.core// 1 3))")
    assert isinstance(out, BigDecimal)
    # Approx 0.333... within tolerance of the BigDecimal default precision
    assert abs(float(out) - 1.0/3.0) < 1e-15

def test_bigdec_bigint_input():
    out = E("(clojure.core/bigdec (clojure.core/bigint 100))")
    assert isinstance(out, BigDecimal)
    assert out == BigDecimal("100")


# --- dotimes still works with new (long ~n) body ------------------

def test_dotimes_with_long_redef_body():
    """The dotimes redef body now uses (long ~n) — sanity check that
    it still iterates the right number of times."""
    counter = []
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb16-poke!"),
               lambda i: counter.append(i))
    E("(clojure.core/dotimes [i 5] (user/tcb16-poke! i))")
    assert counter == [0, 1, 2, 3, 4]

def test_dotimes_with_long_n_arg():
    """Pass a Python int (Long-style) — long coercion is identity."""
    counter = []
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb16-poke2!"),
               lambda i: counter.append(i))
    E("(clojure.core/dotimes [i (clojure.core/long 4)] (user/tcb16-poke2! i))")
    assert counter == [0, 1, 2, 3]
