"""Tests for the REPL core (shared read/eval/print machinery)."""

import io
import sys
import pytest

from clojure import _core
from clojure.repl import core


@pytest.fixture(autouse=True)
def _ensure_helpers():
    core._ensure_history_vars()
    core.install_repl_helpers()


def _stream_reader(lines):
    """Return a read_line callable that plays back `lines` then EOF."""
    it = iter(lines)
    def read_line(_continuation):
        return next(it, None)
    return read_line


# --- read_form -----------------------------------------------------------


def test_read_form_single_line():
    r = _stream_reader(["(+ 1 2)"])
    form = core.read_form(r)
    assert str(form) == "(+ 1 2)"


def test_read_form_multi_line():
    r = _stream_reader(["(defn f", "  [x] (* x 2))"])
    form = core.read_form(r)
    # Should parse as one form.
    assert "defn" in str(form)


def test_read_form_eof_empty_buffer():
    r = _stream_reader([])
    assert core.read_form(r) is core.EOF


def test_read_form_eof_mid_form_returns_eof():
    # Stream ends mid-form — we return EOF (the accumulated buffer is
    # discarded; the user has stopped typing).
    r = _stream_reader(["(defn f"])
    assert core.read_form(r) is core.EOF


def test_read_form_blank_lines_ignored():
    r = _stream_reader(["", "", "(+ 1 2)"])
    form = core.read_form(r)
    assert str(form) == "(+ 1 2)"


def test_read_form_malformed_raises():
    r = _stream_reader(["(:a :b :c"])  # complete per the reader until EOF; EOF returns EOF
    # Once there's no more input, this goes EOF; but if we feed a
    # *non-EOF* error like an unexpected closing paren, that surfaces:
    r2 = _stream_reader([")"])
    with pytest.raises(_core.ReaderError):
        core.read_form(r2)


# --- eval_and_print ------------------------------------------------------


def test_eval_and_print_result_goes_to_print_fn():
    out = []
    err = []
    form = _core.eval_string("(read-string \"(+ 1 2)\")")
    core.eval_and_print(
        form, print_fn=out.append, err_fn=err.append
    )
    assert out == ["3"]
    assert err == []


def test_eval_and_print_catches_exception_and_sets_star_e():
    out = []
    err = []
    form = _core.eval_string('(read-string "(+ 1 \\"x\\")")')
    core.eval_and_print(
        form, print_fn=out.append, err_fn=err.append
    )
    assert out == []  # nothing printed on error
    assert any("TypeError" in line for line in err)
    # *e should be bound to the exception.
    e_var = sys.modules["clojure.user"].__dict__["*e"]
    assert isinstance(e_var.deref(), BaseException)


def test_history_vars_rotate():
    out = []
    for src in ["42", '"hi"', ":a"]:
        form = _core.eval_string("(read-string %s)" % _clj_str(src))
        core.eval_and_print(form, print_fn=out.append, err_fn=lambda _s: None)
    # After three evals: *1=:a, *2="hi", *3=42.
    user_ns = sys.modules["clojure.user"]
    assert user_ns.__dict__["*1"].deref() == _core.eval_string(":a")
    assert user_ns.__dict__["*2"].deref() == "hi"
    assert user_ns.__dict__["*3"].deref() == 42


def _clj_str(s):
    """Wrap `s` as a Clojure string literal for read-string."""
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def test_print_uses_pr_str_not_repr():
    out = []
    form = _core.eval_string('(read-string "{:a 1 :b 2}")')
    core.eval_and_print(form, print_fn=out.append, err_fn=lambda _s: None)
    # pr-str renders Clojure maps with curly braces + Clojure syntax.
    assert out[0].startswith("{:")


# --- install_repl_helpers -----------------------------------------------


def test_install_repl_helpers_interns_pst_and_doc():
    user_ns = sys.modules["clojure.user"]
    assert "pst" in user_ns.__dict__
    assert "doc" in user_ns.__dict__


def test_pst_no_exception_calls_python_print(capfd):
    # pst uses Python's `print` directly (not Clojure `println`), so
    # capfd should see it.
    sys.modules["clojure.user"].__dict__["*e"].bind_root(None)
    _core.eval_string("(pst)")
    captured = capfd.readouterr()
    assert "no exception" in captured.err


def test_doc_prints_arglists():
    # Use `with-out-str` to capture what `doc` prints — pytest's
    # capsys/capfd don't intercept the original sys.stdout that the
    # Clojure `*out*` var was bound to at module-load time.
    out = _core.eval_string("(with-out-str (doc #'map))")
    assert "------" in out
    assert "lazy sequence" in out
