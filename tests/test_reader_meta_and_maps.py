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
