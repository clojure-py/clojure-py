"""Tests for core.clj batch 43: update-vals / update-keys / parse-* /
NaN? / infinite? (JVM 8136-8233).

This is the LAST batch of JVM core.clj — fully through line 8233.

Adaptations from JVM:
  - Long/valueOf → (py.__builtins__/int s). NumberFormatException →
    Python ValueError.
  - Double/valueOf → (py.__builtins__/float s). Likewise.
  - java.util.UUID/fromString → (py.uuid/UUID s). IllegalArgumentException
    → ValueError.
  - Double/isNaN → (py.math/isnan num).
  - Double/isInfinite → (py.math/isinf num).
"""

import math as _math

import pytest

import clojure.core  # bootstrap

from clojure.lang import (
    Compiler,
    read_string,
    Keyword,
)


def E(src):
    return Compiler.eval(read_string(src))


def K(name):
    return Keyword.intern(None, name)


# --- update-vals ----------------------------------------------

def test_update_vals_basic():
    out = E("(update-vals {:a 1 :b 2 :c 3} inc)")
    assert dict(out) == {K("a"): 2, K("b"): 3, K("c"): 4}

def test_update_vals_preserves_keys():
    """Keys unchanged; only values transformed."""
    out = E('(update-vals {"x" 10 "y" 20} (fn [v] (* v v)))')
    assert dict(out) == {"x": 100, "y": 400}

def test_update_vals_empty():
    out = E("(update-vals {} inc)")
    assert dict(out) == {}

def test_update_vals_preserves_meta():
    """Metadata on the input map carries to the output."""
    out = E("""
      (let [m (with-meta {:a 1} {:tag :important})]
        (meta (update-vals m inc)))""")
    assert dict(out)[K("tag")] == K("important")

def test_update_vals_uses_transient_for_editable():
    """For IEditableCollection inputs the impl uses (transient m)
    so the underlying map type is preserved through the transform.
    Verify the transform produced correct values; the iteration
    order may differ from the input map type's natural order
    (transient/persistent! roundtrip semantics)."""
    out = E("(update-vals (sorted-map :z 1 :a 2 :m 3) inc)")
    assert dict(out) == {K("z"): 2, K("a"): 3, K("m"): 4}


# --- update-keys ----------------------------------------------

def test_update_keys_basic():
    out = E("(update-keys {:a 1 :b 2} name)")
    assert dict(out) == {"a": 1, "b": 2}

def test_update_keys_preserves_values():
    out = E('(update-keys {"x" :v1 "y" :v2} #(str % "!"))')
    assert dict(out) == {"x!": K("v1"), "y!": K("v2")}

def test_update_keys_empty():
    out = E("(update-keys {} identity)")
    assert dict(out) == {}

def test_update_keys_preserves_meta():
    out = E("""
      (let [m (with-meta {:a 1} {:tag :marker})]
        (meta (update-keys m name)))""")
    assert dict(out)[K("tag")] == K("marker")


# --- parse-long ----------------------------------------------

def test_parse_long_decimal():
    assert E('(parse-long "42")') == 42

def test_parse_long_negative():
    assert E('(parse-long "-7")') == -7

def test_parse_long_positive_sign():
    assert E('(parse-long "+10")') == 10

def test_parse_long_zero():
    assert E('(parse-long "0")') == 0

def test_parse_long_bad_returns_nil():
    assert E('(parse-long "foo")') is None

def test_parse_long_float_string_returns_nil():
    """JVM Long/valueOf rejects floats; Python int() does too."""
    assert E('(parse-long "1.5")') is None

def test_parse_long_empty_string():
    assert E('(parse-long "")') is None

def test_parse_long_non_string_throws():
    with pytest.raises(Exception, match="Expected string"):
        E("(parse-long 42)")

def test_parse_long_nil_throws():
    with pytest.raises(Exception, match="Expected string"):
        E("(parse-long nil)")


# --- parse-double --------------------------------------------

def test_parse_double_basic():
    assert E('(parse-double "1.5")') == 1.5

def test_parse_double_int_string():
    """Integer strings parse to floats."""
    assert E('(parse-double "42")') == 42.0

def test_parse_double_negative():
    assert E('(parse-double "-3.14")') == -3.14

def test_parse_double_scientific():
    assert E('(parse-double "1e3")') == 1000.0

def test_parse_double_bad_returns_nil():
    assert E('(parse-double "foo")') is None

def test_parse_double_non_string_throws():
    with pytest.raises(Exception, match="Expected string"):
        E("(parse-double 1.5)")


# --- parse-uuid ----------------------------------------------

def test_parse_uuid_basic():
    out = E('(parse-uuid "12345678-1234-5678-1234-567812345678")')
    import uuid
    assert isinstance(out, uuid.UUID)
    assert str(out) == "12345678-1234-5678-1234-567812345678"

def test_parse_uuid_random_round_trip():
    out = E("""
      (let [u (random-uuid)]
        (= u (parse-uuid (str u))))""")
    assert out is True

def test_parse_uuid_bad_returns_nil():
    assert E('(parse-uuid "not-a-uuid")') is None

def test_parse_uuid_empty_returns_nil():
    assert E('(parse-uuid "")') is None

def test_parse_uuid_non_string_throws():
    """JVM swallows non-string in the catch; Python is stricter and
    raises in our adaptation. Aligns with parse-long / parse-double."""
    with pytest.raises(Exception, match="Expected string"):
        E("(parse-uuid 42)")


# --- parse-boolean -------------------------------------------

def test_parse_boolean_true():
    assert E('(parse-boolean "true")') is True

def test_parse_boolean_false():
    assert E('(parse-boolean "false")') is False

def test_parse_boolean_unknown():
    """Anything other than exactly "true" / "false" returns nil."""
    assert E('(parse-boolean "TRUE")') is None
    assert E('(parse-boolean "yes")') is None
    assert E('(parse-boolean "")') is None

def test_parse_boolean_non_string_throws():
    with pytest.raises(Exception, match="Expected string"):
        E("(parse-boolean true)")


# --- NaN? ----------------------------------------------------

def test_nan_pred_true_for_nan():
    assert E("(NaN? (/ 0.0 0.0))") is True

def test_nan_pred_false_for_finite():
    assert E("(NaN? 0)") is False
    assert E("(NaN? 1.5)") is False
    assert E("(NaN? -3.14)") is False

def test_nan_pred_false_for_inf():
    assert E("(NaN? (/ 1.0 0.0))") is False


# --- infinite? -----------------------------------------------

def test_infinite_pred_true_for_inf():
    assert E("(infinite? (/ 1.0 0.0))") is True

def test_infinite_pred_true_for_neg_inf():
    assert E("(infinite? (/ -1.0 0.0))") is True

def test_infinite_pred_false_for_finite():
    assert E("(infinite? 0)") is False
    assert E("(infinite? 1.5)") is False
    assert E("(infinite? 1e308)") is False

def test_infinite_pred_false_for_nan():
    assert E("(infinite? (/ 0.0 0.0))") is False
