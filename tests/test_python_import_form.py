"""Tests for the `(import ...)` Python-module/class import form
and `__clj_imports__` symbol resolution.
"""

import pytest

from clojure._core import (
    create_ns,
    eval_string,
    import_cls,
    symbol,
)


@pytest.fixture(autouse=True)
def _restore_ns():
    """Save *ns* and restore it after each test, so that tests which
    switch namespaces don't leak into other test files."""
    saved = eval_string("(str (ns-name *ns*))")
    yield
    eval_string(f"(in-ns '{saved})")


def _fresh_ns(name: str):
    """Make a fresh ns, switch *ns* to it, refer clojure.core, and
    return the ns object."""
    ns = create_ns(symbol(name))
    eval_string(f"(in-ns '{name})")
    eval_string("(clojure.core/refer 'clojure.core)")
    return ns


def test_imports_dict_lookup_resolves_unqualified():
    """If `__clj_imports__` contains {Tk -> tkinter.Tk}, the symbol `Tk`
    resolves to the class object (as a const)."""
    import tkinter
    ns = _fresh_ns("import.test.lookup1")
    import_cls(ns, symbol("Tk"), tkinter.Tk)
    # Symbol Tk should now resolve to the Tk class.
    result = eval_string("Tk")
    assert result is tkinter.Tk


def test_import_dotted_class():
    _fresh_ns("import.test.dotted1")
    eval_string("(import 'tkinter.Tk)")
    import tkinter
    assert eval_string("Tk") is tkinter.Tk


def test_import_bare_module():
    _fresh_ns("import.test.bare1")
    eval_string("(import 'tkinter)")
    import tkinter
    assert eval_string("tkinter") is tkinter


def test_import_vector_form():
    _fresh_ns("import.test.vec1")
    eval_string("(import '[tkinter Tk Canvas])")
    import tkinter
    assert eval_string("Tk") is tkinter.Tk
    assert eval_string("Canvas") is tkinter.Canvas


def test_import_list_form():
    _fresh_ns("import.test.list1")
    eval_string("(import '(tkinter Tk Canvas))")
    import tkinter
    assert eval_string("Tk") is tkinter.Tk
    assert eval_string("Canvas") is tkinter.Canvas


def test_import_multi_group():
    _fresh_ns("import.test.multi1")
    eval_string("(import '[tkinter Tk] '[tkinter.font Font])")
    import tkinter
    import tkinter.font
    assert eval_string("Tk") is tkinter.Tk
    assert eval_string("Font") is tkinter.font.Font


def test_import_unknown_module_raises():
    _fresh_ns("import.test.err1")
    with pytest.raises(Exception) as ei:
        eval_string("(import 'no_such_module_xyz)")
    msg = str(ei.value)
    assert "no_such_module_xyz" in msg


def test_import_unknown_attr_raises():
    _fresh_ns("import.test.err2")
    with pytest.raises(Exception) as ei:
        eval_string("(import 'tkinter.NoSuchAttr)")
    msg = str(ei.value)
    assert "NoSuchAttr" in msg and "tkinter" in msg


def test_ns_form_import_directive():
    """`(:import [tkinter Tk Canvas])` inside `(ns ...)` works the same
    as a top-level `(import '[tkinter Tk Canvas])`."""
    import tkinter
    import tkinter.font
    eval_string("""
    (ns import.test.ns-form-1
      (:import [tkinter Tk Canvas]
               [tkinter.font Font]))
    """)
    result = eval_string("[Tk Canvas Font]")
    items = list(result)
    assert items == [tkinter.Tk, tkinter.Canvas, tkinter.font.Font]
