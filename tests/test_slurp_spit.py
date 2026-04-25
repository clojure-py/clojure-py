"""Slurp / spit edge cases — Python adaptation of the portable subset of
clojure/test/clojure/test_clojure/java/io.clj.

Vanilla java/io.clj covers `slurp`, `spit`, `reader`, `writer`, `input-stream`,
`output-stream`, `copy`, `as-file`, `file`, `as-url`, `delete-file`,
`make-parents`, and socket I/O. Of those we have only `slurp` and `spit`,
backed by Python `open()` (rt_ns.rs::slurp-impl / spit-impl). What's tested
here is just the contract those two need to honor; the file-abstraction
helpers (`as-file`, `copy`, etc.) are deferred until we add a clojure.java.io
analog.

What's deliberately not tested:
  - `:append true` and `:encoding` opts to spit (not supported in spit-impl).
  - `slurp` of a URL / classpath resource (no resource loader).
  - Reader / writer abstractions (no analog yet).
"""

import os
import tempfile

import pytest

from clojure._core import eval_string as _ev


# ---------------------------------------------------------------------------
# Helper
# ---------------------------------------------------------------------------

@pytest.fixture
def tmp_path_str(tmp_path):
    """tmp_path as a forward-slash str (avoids backslash escaping in Clojure)."""
    return str(tmp_path).replace("\\", "/")


# ---------------------------------------------------------------------------
# Round-trip
# ---------------------------------------------------------------------------

def test_spit_then_slurp_ascii(tmp_path_str):
    p = f"{tmp_path_str}/hello.txt"
    _ev(f'(spit "{p}" "hello world")')
    assert _ev(f'(slurp "{p}")') == "hello world"


def test_spit_then_slurp_empty_string(tmp_path_str):
    p = f"{tmp_path_str}/empty.txt"
    _ev(f'(spit "{p}" "")')
    assert _ev(f'(slurp "{p}")') == ""


def test_spit_then_slurp_with_newlines(tmp_path_str):
    p = f"{tmp_path_str}/multiline.txt"
    _ev(f'(spit "{p}" "line1\\nline2\\nline3")')
    assert _ev(f'(slurp "{p}")') == "line1\nline2\nline3"


def test_spit_then_slurp_utf8(tmp_path_str):
    p = f"{tmp_path_str}/utf8.txt"
    # Lambda + checkmark + Mandarin — exercise multibyte UTF-8.
    _ev(f'(spit "{p}" "λ ✓ 你好")')
    assert _ev(f'(slurp "{p}")') == "λ ✓ 你好"


def test_spit_then_slurp_long_content(tmp_path_str):
    p = f"{tmp_path_str}/big.txt"
    _ev(f'(spit "{p}" (apply str (repeat 10000 "abc")))')
    s = _ev(f'(slurp "{p}")')
    assert len(s) == 30000
    assert s == "abc" * 10000


# ---------------------------------------------------------------------------
# Spit semantics
# ---------------------------------------------------------------------------

def test_spit_overwrites(tmp_path_str):
    """Default spit must overwrite, not append."""
    p = f"{tmp_path_str}/over.txt"
    _ev(f'(spit "{p}" "first")')
    _ev(f'(spit "{p}" "second")')
    assert _ev(f'(slurp "{p}")') == "second"


def test_spit_returns_nil(tmp_path_str):
    p = f"{tmp_path_str}/ret.txt"
    assert _ev(f'(spit "{p}" "x")') is None


def test_spit_stringifies_non_string(tmp_path_str):
    """Vanilla spit accepts any value and writes its string form."""
    p = f"{tmp_path_str}/num.txt"
    _ev(f'(spit "{p}" 42)')
    assert _ev(f'(slurp "{p}")') == "42"


def test_spit_stringifies_keyword(tmp_path_str):
    p = f"{tmp_path_str}/kw.txt"
    _ev(f'(spit "{p}" :hello)')
    assert _ev(f'(slurp "{p}")') == ":hello"


def test_spit_stringifies_collection(tmp_path_str):
    """A vector should print in its readable form."""
    p = f"{tmp_path_str}/vec.txt"
    _ev(f'(spit "{p}" [1 2 3])')
    # The exact form is printer-dependent; just verify it round-trips
    # and contains the elements.
    s = _ev(f'(slurp "{p}")')
    assert "1" in s and "2" in s and "3" in s


# ---------------------------------------------------------------------------
# Slurp errors
# ---------------------------------------------------------------------------

def test_slurp_missing_file_raises(tmp_path_str):
    """vanilla `clj-2783-slurp-maintains-backward-compatibility-errors` —
    slurp on a non-existent path must raise, not silently return nil."""
    p = f"{tmp_path_str}/does-not-exist.txt"
    assert not os.path.exists(p)
    with pytest.raises(Exception):  # FileNotFoundError or our IllegalStateException
        _ev(f'(slurp "{p}")')


def test_slurp_on_directory_raises(tmp_path_str):
    """slurp on a directory must raise (Python `open(dir, "r")` raises IsADirectoryError)."""
    with pytest.raises(Exception):
        _ev(f'(slurp "{tmp_path_str}")')


# ---------------------------------------------------------------------------
# Composition with line-seq / read-line via slurp output
# ---------------------------------------------------------------------------

def test_slurp_then_split_lines(tmp_path_str):
    p = f"{tmp_path_str}/lines.txt"
    _ev(f'(spit "{p}" "a\\nb\\nc")')
    _ev('(require (quote [clojure.string :as s]))')
    result = _ev(f'(s/split-lines (slurp "{p}"))')
    assert list(result) == ["a", "b", "c"]


def test_slurp_then_count_chars(tmp_path_str):
    p = f"{tmp_path_str}/n.txt"
    _ev(f'(spit "{p}" "hello")')
    assert _ev(f'(count (slurp "{p}"))') == 5
