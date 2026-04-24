"""pytest integration for `clojure.test`.

Discovers `.clj` test files matching any of:
  1. Basename `test_*.clj` or `*_test.clj`, OR
  2. First top-level form is `(ns X ...)` where X's name ends in `-test`.

Each `deftest` becomes one pytest item. `is` failures and uncaught exceptions
inside a deftest are aggregated into a single item failure with pytest-style
expected/actual output.

Loaded automatically by pytest via the `pytest11` entry point declared in
`pyproject.toml` — users don't need a `conftest.py`.
"""
from __future__ import annotations

import threading
from pathlib import Path

import pytest

import clojure  # installs meta-path finder
from clojure._core import (
    Symbol,
    Var,
    create_ns,
    eval_string,
    load_file_into_ns,
    read_first_form_from_file,
    symbol,
)

_BRIDGE_INSTALLED = False
_BRIDGE_LOCK = threading.Lock()
_CURRENT_EVENTS: threading.local = threading.local()
# Active :once-fixture teardown callbacks, keyed by file path (string). Drained
# in pytest_sessionfinish.
_ONCE_TEARDOWNS: dict[str, callable] = {}


def _push_event(m):
    """Called by clojure.test's report methods (via a Clojure Var we install
    pointing at this fn). Appends the event map to the per-thread buffer.
    No-op if no buffer is active (the deftest isn't pytest-scoped)."""
    buf = getattr(_CURRENT_EVENTS, "buf", None)
    if buf is not None:
        buf.append(m)
    return None


_BRIDGE_SRC = '''
(ns clojure.test.pytest-bridge
  (:require [clojure.test :refer [inc-report-counter report]]))

(def pytest-push nil)
(def pytest-current-var nil)
(def pytest-current-ns nil)
(def pytest-once-body nil)

(defmethod report :pass [m]
  (inc-report-counter :pass)
  (pytest-push m))

(defmethod report :fail [m]
  (inc-report-counter :fail)
  (pytest-push m))

(defmethod report :error [m]
  (inc-report-counter :error)
  (pytest-push m))
'''


def _install_bridge():
    """Install the pytest/clojure.test bridge on first use. Idempotent."""
    global _BRIDGE_INSTALLED
    with _BRIDGE_LOCK:
        if _BRIDGE_INSTALLED:
            return
        eval_string("(require 'clojure.test)")
        # Load the bridge as a file so `(ns ...)` takes effect between forms.
        import sys, tempfile, os
        with tempfile.NamedTemporaryFile(
            suffix=".clj", mode="w", delete=False
        ) as fh:
            fh.write(_BRIDGE_SRC)
            path = fh.name
        try:
            bridge_ns = create_ns(symbol("clojure.test.pytest-bridge"))
            load_file_into_ns(path, bridge_ns)
        finally:
            os.unlink(path)
        bridge = sys.modules["clojure.test.pytest-bridge"]
        bridge.__dict__["pytest-push"].bind_root(_push_event)
        _BRIDGE_INSTALLED = True


# ---- discovery ---------------------------------------------------------------


def _is_ns_form(first_form) -> tuple[bool, str | None]:
    """Return (is-ns-form, ns-name-string)."""
    try:
        iter_ok = hasattr(first_form, "__iter__") or hasattr(first_form, "__len__")
        if not iter_ok:
            return (False, None)
        head = next(iter(first_form), None)
        if not isinstance(head, Symbol) or head.name != "ns":
            return (False, None)
        # 2nd element is the ns name symbol.
        items = list(first_form)
        if len(items) < 2 or not isinstance(items[1], Symbol):
            return (False, None)
        return (True, items[1].name)
    except Exception:
        return (False, None)


def _looks_like_test_file(path: Path) -> bool:
    """Is this `.clj` a test file?

    A file is a test file if ANY of:
      1. Basename matches `test_*.clj` or `*_test.clj` (pytest style).
      2. The `(ns X …)` form's name ends in `-test` or `_test`
         (standard Clojure convention: `my-lib.core-test`).
      3. The file contains at least one `deftest` form. This covers ported
         Clojure-core-style tests where the ns is `clojure.test-clojure.X`
         and the file itself is named `X.clj` with no suffix — the vanilla
         layout. Checked via a cheap text scan that ignores commented lines.
    """
    if path.suffix != ".clj":
        return False
    stem = path.stem
    if stem.startswith("test_") or stem.endswith("_test"):
        return True
    try:
        first = read_first_form_from_file(str(path))
    except Exception:
        return False
    ok, name = _is_ns_form(first)
    if ok and name and (name.endswith("-test") or name.endswith("_test")):
        return True
    # Rule 3: text scan for `(deftest ` outside line comments.
    try:
        text = path.read_text(encoding="utf-8")
    except Exception:
        return False
    for line in text.splitlines():
        stripped = line.lstrip()
        if stripped.startswith(";"):
            continue
        if "(deftest " in stripped or "(deftest\t" in stripped:
            return True
    return False


def _ns_sym_from_file(path: Path) -> Symbol:
    """Read the file's first form; if it's `(ns X …)` use X, else synthesize
    from the stem."""
    try:
        first = read_first_form_from_file(str(path))
        ok, name = _is_ns_form(first)
        if ok and name:
            return symbol(name)
    except Exception:
        pass
    return symbol(path.stem)


# ---- pytest collector / item -------------------------------------------------


class ClojureTestFailed(Exception):
    """Carries a list of clojure.test `:fail` / `:error` event maps for a
    single deftest."""

    def __init__(self, reports):
        super().__init__(f"clojure.test: {len(reports)} failure(s)")
        self.reports = reports


class ClojureTestItem(pytest.Item):
    def __init__(self, *, name, parent, var, ns_obj, line):
        super().__init__(name, parent)
        self._var = var
        self._ns = ns_obj
        self._line = line

    def runtest(self):
        _install_bridge()
        import sys
        bridge = sys.modules["clojure.test.pytest-bridge"]
        # Install the var-under-test on a side slot the bridge can reach.
        bridge.__dict__["pytest-current-var"].bind_root(self._var)
        _CURRENT_EVENTS.buf = []
        try:
            # Apply :each fixtures and invoke the test var.
            eval_string("""
                (let [v#    clojure.test.pytest-bridge/pytest-current-var
                      ns#   (:ns (clojure.core/meta v#))
                      ef#   (clojure.test/join-fixtures
                              (:clojure.test/each-fixtures (clojure.core/meta ns#)))
                      body# (fn [] (v#))]
                  (ef# body#))
            """)
        finally:
            buf = _CURRENT_EVENTS.buf
            _CURRENT_EVENTS.buf = None

        failures = [m for m in buf if _event_type(m) in ("fail", "error")]
        if failures:
            raise ClojureTestFailed(failures)

    def reportinfo(self):
        return (self.path, self._line, f"{self.path.stem}::{self.name}")

    def repr_failure(self, excinfo):
        if excinfo.errisinstance(ClojureTestFailed):
            exc: ClojureTestFailed = excinfo.value
            lines = []
            for r in exc.reports:
                lines.append(_format_report(r, self.path))
            return "\n".join(lines)
        return super().repr_failure(excinfo)


class ClojureTestFile(pytest.File):
    def _load_ns(self):
        sym = _ns_sym_from_file(self.path)
        initial = create_ns(sym)
        terminal = load_file_into_ns(str(self.path), initial)
        return terminal

    def collect(self):
        _install_bridge()
        ns = self._load_ns()
        self._ns = ns
        self._start_once_fixtures(ns)
        # Iterate interned vars; yield ClojureTestItem for those with :test meta.
        # Use ns-interns to stay consistent with clojure.test's own discovery.
        # Cheapest Python-side: walk __dict__ and check Var.meta.
        for name, val in list(ns.__dict__.items()):
            if not isinstance(val, Var):
                continue
            meta = val.meta
            if meta is None:
                continue
            if not _meta_has_test(meta):
                continue
            line = _meta_get(meta, "line") or 0
            yield ClojureTestItem.from_parent(
                parent=self, name=name, var=val, ns_obj=ns, line=line
            )

    def _start_once_fixtures(self, ns):
        """Split clojure.test's :once-fixtures (wrap-style, single callable)
        into setup+teardown phases using a worker thread. Setup runs eagerly
        before any item; teardown is registered via addfinalizer."""
        import threading
        setup_done = threading.Event()
        teardown_start = threading.Event()
        teardown_done = threading.Event()
        error_holder: list[BaseException] = []

        def worker():
            try:
                import sys
                bridge = sys.modules["clojure.test.pytest-bridge"]
                bridge.__dict__["pytest-current-ns"].bind_root(ns)
                def body():
                    setup_done.set()
                    teardown_start.wait()
                    return None
                bridge.__dict__["pytest-once-body"].bind_root(body)
                eval_string("""
                    (let [ns#   clojure.test.pytest-bridge/pytest-current-ns
                          of#   (clojure.test/join-fixtures
                                  (:clojure.test/once-fixtures
                                   (clojure.core/meta ns#)))
                          body# clojure.test.pytest-bridge/pytest-once-body]
                      (of# body#))
                """)
            except BaseException as e:  # noqa: BLE001
                error_holder.append(e)
                setup_done.set()
            finally:
                teardown_done.set()

        t = threading.Thread(target=worker, daemon=True)
        t.start()
        setup_done.wait()
        if error_holder:
            raise error_holder[0]

        def finalize():
            teardown_start.set()
            teardown_done.wait()
            if error_holder:
                raise error_holder[0]

        _ONCE_TEARDOWNS[str(self.path)] = finalize


# ---- helpers: Clojure data introspection from Python -------------------------


def _meta_has_test(meta) -> bool:
    # Fastest: eval a lookup; cached if called a lot, but usually few vars.
    try:
        kw = eval_string("(keyword \"test\")")
        return meta.__contains__(kw) if hasattr(meta, "__contains__") else False
    except Exception:
        return False


def _meta_get(meta, key_name: str):
    try:
        kw = eval_string(f"(keyword \"{key_name}\")")
        return meta[kw] if hasattr(meta, "__getitem__") else None
    except Exception:
        return None


def _event_type(m) -> str:
    try:
        t_kw = eval_string("(keyword \"type\")")
        t = m[t_kw] if hasattr(m, "__getitem__") else None
        if t is None:
            return ""
        return t.name if hasattr(t, "name") else str(t)
    except Exception:
        return ""


def _format_report(m, path: Path) -> str:
    t = _event_type(m)
    expected = _pretty(m, "expected")
    actual = _pretty(m, "actual")
    message = _pretty(m, "message")
    header = f"{t.upper()} in ({path.name})"
    body = [header]
    if message and message != "nil":
        body.append(f"  {message}")
    if expected is not None:
        body.append(f"  expected: {expected}")
    if actual is not None:
        body.append(f"    actual: {actual}")
    return "\n".join(body)


def _pretty(m, key_name: str):
    try:
        kw = eval_string(f"(keyword \"{key_name}\")")
        val = m[kw] if hasattr(m, "__getitem__") else None
        if val is None:
            return None
        return eval_string("(fn [x] (clojure.core/pr-str x))")(val)
    except Exception:
        return None


# ---- pytest hooks ------------------------------------------------------------


def pytest_collect_file(file_path: Path, parent):
    if _looks_like_test_file(file_path):
        return ClojureTestFile.from_parent(parent, path=file_path)
    return None


def pytest_configure(config):
    # Ensure clojure._core is importable & the meta-path finder is installed.
    import clojure  # noqa: F401


def pytest_sessionfinish(session, exitstatus):
    """Drain any pending :once-fixture teardowns. We use a per-file dict
    rather than pytest's addfinalizer because addfinalizer can't be called
    from a Collector's collect() method."""
    errors = []
    for teardown in list(_ONCE_TEARDOWNS.values()):
        try:
            teardown()
        except BaseException as e:  # noqa: BLE001
            errors.append(e)
    _ONCE_TEARDOWNS.clear()
    if errors:
        raise errors[0]
