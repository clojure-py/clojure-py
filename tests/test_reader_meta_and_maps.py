"""Reader tests for chained ^ meta merge and map duplicate-key rejection.

Vanilla Clojure JVM semantics:
- `^M1 ^M2 form`: M2 is read first as M2's meta on form, then M1's keys
  assoc into existing M2 — new (outermost) keys win on conflict.
- `{:a 1 :a 2}` raises at read time with "Duplicate key: :a".
"""

import pytest
from clojure._core import (
    eval as _eval, read_string, ReaderError,
)


# ---------- Meta merge ----------

def test_meta_single_keyword():
    # baseline: ^:a sym -> {:a true}
    assert _eval(read_string('(:a (meta (read-string "^:a sym")))')) is True


def test_meta_two_keywords_merge():
    # ^:a ^:b sym -> {:a true :b true}
    assert _eval(read_string('(:a (meta (read-string "^:a ^:b sym")))')) is True
    assert _eval(read_string('(:b (meta (read-string "^:a ^:b sym")))')) is True


def test_meta_three_keywords_merge():
    src = '"^:a ^:b ^:c sym"'
    assert _eval(read_string(f"(:a (meta (read-string {src})))")) is True
    assert _eval(read_string(f"(:b (meta (read-string {src})))")) is True
    assert _eval(read_string(f"(:c (meta (read-string {src})))")) is True


def test_meta_keyword_and_map_merge():
    # ^:a ^{:b 1} sym -> {:a true :b 1}
    src = '"^:a ^{:b 1} sym"'
    assert _eval(read_string(f"(:a (meta (read-string {src})))")) is True
    assert _eval(read_string(f"(:b (meta (read-string {src})))")) == 1


def test_meta_outer_wins_on_conflict():
    # ^{:a 1} ^{:a 2} sym -> {:a 1} — outer wins
    src = '"^{:a 1} ^{:a 2} sym"'
    assert _eval(read_string(f"(:a (meta (read-string {src})))")) == 1


def test_meta_baseline_no_chain():
    # Single map annotation still works.
    src = '"^{:foo :bar} sym"'
    expected = _eval(read_string(":bar"))
    assert _eval(read_string(f"(:foo (meta (read-string {src})))")) == expected


# ---------- Map duplicate keys ----------

def test_dup_key_simple():
    with pytest.raises(ReaderError) as ei:
        read_string('{:a 1 :a 2}')
    msg = str(ei.value)
    assert "Duplicate key" in msg
    assert ":a" in msg


def test_dup_key_int():
    with pytest.raises(ReaderError) as ei:
        read_string('{1 :x 1 :y}')
    msg = str(ei.value)
    assert "Duplicate key" in msg
    assert "1" in msg


def test_dup_key_int_float_distinct():
    # 1 ≠ 1.0 under `=`. Must NOT raise.
    assert _eval(read_string('(count {1 :x 1.0 :y})')) == 2


def test_dup_key_ratio_literal_reduces_to_int():
    # 4/2 in the reader literal reduces to int 2 (per ratio reader).
    # `{4/2 :x 2 :y}` — both keys reduce to int 2 → duplicate.
    with pytest.raises(ReaderError) as ei:
        read_string('{4/2 :x 2 :y}')
    msg = str(ei.value)
    assert "Duplicate key" in msg


def test_dup_key_baseline_no_dup():
    assert _eval(read_string('(:a {:a 1 :b 2})')) == 1
    assert _eval(read_string('(:b {:a 1 :b 2})')) == 2


def test_dup_key_message_includes_first_dup():
    with pytest.raises(ReaderError) as ei:
        read_string('{:a 1 :b 2 :a 3 :b 4}')
    msg = str(ei.value)
    assert "Duplicate key" in msg
    assert (":a" in msg) or (":b" in msg)


# ---------- Set duplicate keys ----------

def test_set_dup_message_includes_key():
    with pytest.raises(ReaderError) as ei:
        read_string('#{:a :a}')
    msg = str(ei.value)
    assert "Duplicate key" in msg
    assert ":a" in msg


def test_set_dup_message_includes_int_key():
    with pytest.raises(ReaderError) as ei:
        read_string('#{1 1}')
    msg = str(ei.value)
    assert "Duplicate key" in msg
    assert "1" in msg
