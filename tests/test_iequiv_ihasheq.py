from fractions import Fraction
from decimal import Decimal
from clojure._core import IEquiv, IHashEq, equiv, hash_eq, Protocol
from clojure._core import eval as _eval, read_string


def test_iequiv_is_registered():
    assert isinstance(IEquiv, Protocol)


def test_ihasheq_is_registered():
    assert isinstance(IHashEq, Protocol)


def test_equiv_on_ints():
    assert equiv(1, 1) is True
    assert equiv(1, 2) is False


def test_equiv_on_strings():
    assert equiv("a", "a") is True
    assert equiv("a", "b") is False


def test_equiv_on_none():
    assert equiv(None, None) is True


def test_equiv_cross_type_false():
    # Python's 1 == "1" is False — fallback should reflect that.
    assert equiv(1, "1") is False


def test_hash_eq_on_int():
    # hash_eq for ints uses Murmur3.hashLong (vanilla Clojure parity), so
    # it diverges from Python's `hash(int)` which is the int value itself.
    h = hash_eq(42)
    assert isinstance(h, int)
    assert h != 42  # Murmur3-mixed; not identity
    assert hash_eq(42) == hash_eq(42)
    # Distinct ints get distinct hashes (no trivial collisions).
    assert hash_eq(42) != hash_eq(43)


def test_hash_eq_on_string():
    assert hash_eq("hello") == hash("hello")


def test_hash_eq_equivalent_values_same_hash():
    assert hash_eq(42) == hash_eq(42)


def test_equiv_true_false_booleans():
    assert equiv(True, True) is True
    assert equiv(False, False) is True
    assert equiv(True, False) is False


def test_equiv_ratio_vs_float_is_false():
    # Vanilla: Ratio != Double under `=`. Even though numerically equal.
    assert _eval(read_string("(= 1/2 0.5)")) is False
    assert _eval(read_string("(= 0.5 1/2)")) is False


def test_equiv_ratio_vs_int_is_false_when_not_whole():
    assert _eval(read_string("(= 1/2 1)")) is False
    assert _eval(read_string("(= 1 1/2)")) is False


def test_equiv_ratio_vs_ratio():
    assert _eval(read_string("(= 1/2 1/2)")) is True
    assert _eval(read_string("(= 1/2 2/4)")) is True   # both reduce to 1/2
    assert _eval(read_string("(= 1/2 1/3)")) is False


def test_equiv_division_result_int_vs_int():
    # (/ 4 2) reduces to int 2 - comparison is Cat::Int vs Cat::Int.
    assert _eval(read_string("(= (/ 4 2) 2)")) is True


def test_equiv_decimal_vs_int_is_false():
    # Vanilla: BigDecimal != Long under `=`.
    assert equiv(Decimal("1"), 1) is False
    assert equiv(1, Decimal("1")) is False


def test_equiv_decimal_vs_decimal():
    assert equiv(Decimal("1.0"), Decimal("1.0")) is True
    assert equiv(Decimal("1.0"), Decimal("2.0")) is False


def test_numeric_equiv_ratio_vs_float_is_true():
    # `==` (num-equiv) DOES bridge categories.
    assert _eval(read_string("(== 1/2 0.5)")) is True
    assert _eval(read_string("(== (/ 4 2) 2.0)")) is True
