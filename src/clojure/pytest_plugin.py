"""Pytest plugin: discover and run clojure.test deftest forms in
*_test.clj files.

How it works:
  - pytest_collect_file picks up any file matching *_test.clj.
  - The custom ClojureTestFile collector loads the file as a Clojure
    namespace (via clojure.lang.Compiler.load_file), then walks the
    namespace's interns looking for vars whose meta carries :test.
  - Each such var becomes a ClojureTestItem.
  - At runtest() time, the item rebinds clojure.test/report (a
    multimethod) to capture :fail / :error events into a list, calls
    (test-var v), then translates collected events to pytest.fail
    or a regular exception.

Drop this in to your project's pyproject.toml entry_points or just
import it via a top-level conftest.py with:

    pytest_plugins = ["clojure.pytest_plugin"]

The repo's own tests don't use this — Python tests live under
tests/ and verify clojure-py internals. This plugin is for
downstream projects writing tests in .clj files.
"""

from __future__ import annotations

import io
from pathlib import Path

import pytest


# Lazy imports — pytest discovers this plugin before the user's test
# code triggers clojure.core load.
def _bootstrap():
    import clojure.core  # noqa: F401
    from clojure.lang import (
        Compiler,
        Symbol,
        Keyword,
        Namespace,
        Var,
        read_string,
    )
    return Compiler, Symbol, Keyword, Namespace, Var, read_string


def _path_to_ns_name(path):
    """Convert a path like .../foo/bar_test.clj to the Clojure
    namespace symbol the file likely declares ('foo.bar-test').

    We use this only as a hint when the file lacks an explicit (ns
    ...) form; if the load picks up a different ns we use the actual
    one.
    """
    p = Path(path)
    stem = p.stem
    return stem.replace("_", "-")


def _ns_name_from_clj_file(path):
    """Read the file's leading (ns ...) form (skipping comments) and
    return the namespace symbol name as a string. Returns None if no
    (ns ...) form is found in the first few forms."""
    Compiler, Symbol, Keyword, Namespace, Var, read_string = _bootstrap()
    text = Path(path).read_text(encoding="utf-8")
    # Wrap in a do form so we can read all top-level forms in one go,
    # then walk for the (ns ...) form.
    forms = read_string("(do " + text + "\nnil)")
    seq = forms.next()  # skip the leading 'do'
    while seq is not None:
        form = seq.first()
        if (hasattr(form, "first")
                and isinstance(form.first(), Symbol)
                and form.first().get_name() in ("ns", "in-ns")):
            # (ns name ...) — name is the second form. (in-ns 'name) —
            # name is a (quote name) form; pull the inner sym.
            second = form.next().first() if form.next() else None
            if isinstance(second, Symbol):
                return str(second)
            if (hasattr(second, "first")
                    and isinstance(second.first(), Symbol)
                    and second.first().get_name() == "quote"):
                inner = second.next().first()
                if isinstance(inner, Symbol):
                    return str(inner)
        seq = seq.next()
    return None


def _load_clj_file(path):
    """Load `path` as a Clojure file and return the Namespace it
    declared. Falls back to current *ns* if no (ns ...) form was
    found."""
    Compiler, Symbol, Keyword, Namespace, _, _ = _bootstrap()
    expected_ns_name = _ns_name_from_clj_file(path)
    Compiler.load_file(str(path))
    if expected_ns_name:
        ns = Namespace.find(Symbol.intern(expected_ns_name))
        if ns is not None:
            return ns
    # Fallback: use *ns* (probably user).
    star_ns = Namespace.find(Symbol.intern("clojure.core")).find_interned_var(
        Symbol.intern("*ns*"))
    return star_ns.deref()


def _deftest_vars(ns_obj):
    """Yield (name-string, var) pairs for every var in `ns_obj` whose
    meta has a callable :test."""
    Compiler, Symbol, Keyword, Namespace, Var, _ = _bootstrap()
    test_kw = Keyword.intern(None, "test")
    interns = ns_obj.get_mappings()
    for entry in interns:
        sym = entry.key()
        v = entry.val()
        if not isinstance(v, Var):
            continue
        # Only vars interned in this ns (skip refers from clojure.core etc.)
        if v.ns is not ns_obj:
            continue
        meta = v.meta()
        if meta is None:
            continue
        test_fn = meta[test_kw] if test_kw in meta else None
        if test_fn is None or not callable(test_fn):
            continue
        yield (sym.get_name(), v)


def _eval(src):
    Compiler, _, _, _, _, read_string = _bootstrap()
    return Compiler.eval(read_string(src))


def _run_test_var_capturing(v):
    """Run a test var, capturing :fail / :error events from the
    clojure.test/report multimethod. Returns (events, captured_out)."""
    Compiler, Symbol, Keyword, Namespace, Var, read_string = _bootstrap()
    test_ns = Namespace.find(Symbol.intern("clojure.test"))
    if test_ns is None:
        # Make sure clojure.test is loaded.
        Compiler.eval(read_string("(require 'clojure.test)"))
        test_ns = Namespace.find(Symbol.intern("clojure.test"))

    test_var_v = test_ns.find_interned_var(Symbol.intern("test-var"))
    report_v = test_ns.find_interned_var(Symbol.intern("report"))
    test_out_v = test_ns.find_interned_var(Symbol.intern("*test-out*"))

    events = []

    type_kw = Keyword.intern(None, "type")
    fail_kw = Keyword.intern(None, "fail")
    error_kw = Keyword.intern(None, "error")

    def _capture_report(m):
        try:
            t = m[type_kw] if type_kw in m else None
        except Exception:
            t = None
        if t is fail_kw or t is error_kw:
            events.append(dict(m))
        return None

    captured = io.StringIO()

    from clojure.lang import PersistentArrayMap
    Var.push_thread_bindings(
        PersistentArrayMap.create(
            report_v, _capture_report,
            test_out_v, captured))
    try:
        test_var_v.deref()(v)
    finally:
        Var.pop_thread_bindings()

    return events, captured.getvalue()


def _format_event(event):
    """Render a captured fail/error event as a multi-line string for
    pytest's assertion message."""
    Compiler, Symbol, Keyword, Namespace, Var, read_string = _bootstrap()
    type_kw = Keyword.intern(None, "type")
    msg_kw = Keyword.intern(None, "message")
    expected_kw = Keyword.intern(None, "expected")
    actual_kw = Keyword.intern(None, "actual")
    file_kw = Keyword.intern(None, "file")
    line_kw = Keyword.intern(None, "line")

    t = event.get(type_kw)
    msg = event.get(msg_kw)
    expected = event.get(expected_kw)
    actual = event.get(actual_kw)
    file_ = event.get(file_kw)
    line = event.get(line_kw)

    pr_str = Compiler.eval(read_string("clojure.core/pr-str"))

    parts = [f"{t}"]
    if msg:
        parts.append(f"  {msg}")
    if file_ or line:
        parts.append(f"  at {file_}:{line}")
    parts.append(f"  expected: {pr_str(expected)}")
    parts.append(f"  actual:   {pr_str(actual)}")
    return "\n".join(parts)


# --- pytest hooks ---------------------------------------------


def pytest_collect_file(parent, file_path):
    if file_path.suffix == ".clj" and file_path.name.endswith("_test.clj"):
        return ClojureTestFile.from_parent(parent, path=file_path)
    return None


class ClojureTestFile(pytest.File):
    """Collector for a single .clj test file."""

    def collect(self):
        ns_obj = _load_clj_file(self.path)
        for name, var in _deftest_vars(ns_obj):
            yield ClojureTestItem.from_parent(self, name=name, deftest_var=var)


class ClojureTestItem(pytest.Item):
    """A single deftest var, runnable as a pytest test."""

    def __init__(self, *, deftest_var, **kwargs):
        super().__init__(**kwargs)
        self._deftest_var = deftest_var

    def runtest(self):
        events, output = _run_test_var_capturing(self._deftest_var)
        if events:
            details = "\n\n".join(_format_event(e) for e in events)
            tail = ""
            if output.strip():
                tail = "\n\n--- captured clojure.test output ---\n" + output
            pytest.fail(details + tail, pytrace=False)

    def reportinfo(self):
        # (file_path, line_or_None, test_id_for_display)
        return (self.path, None, f"deftest {self.name}")
