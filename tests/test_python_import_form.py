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
    """Restore *ns* to clojure.user after each test so global state
    doesn't leak into subsequent tests in other modules."""
    yield
    eval_string("(in-ns 'clojure.user)")


def _fresh_ns(name: str):
    """Make a fresh ns and switch *ns* to it, returning the ns object."""
    ns = create_ns(symbol(name))
    eval_string(f"(in-ns '{name})")
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
