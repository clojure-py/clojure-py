"""Phase R1 — Source behavior (indirectly exercised via reader primitives).

The Source struct is Rust-internal; this test module exercises it indirectly by
confirming that ReaderError messages include line/col, which only works when
Source tracks position correctly.
"""

import pytest
from clojure._core import ReaderError, _test_parse_string, _test_parse_char


def test_error_includes_line_and_col():
    with pytest.raises(ReaderError) as ei:
        _test_parse_string('"unterminated')
    msg = str(ei.value)
    assert "line" in msg
    assert "col" in msg


def test_char_error_includes_position():
    with pytest.raises(ReaderError) as ei:
        _test_parse_char(r"\bogusname")
    msg = str(ei.value)
    assert "line" in msg
    assert "col" in msg


def test_string_escape_newline_tracked():
    # A well-formed newline inside a string should succeed (and not trip
    # line/col logic).
    s = _test_parse_string('"line1\nline2"')
    assert s == "line1\nline2"
