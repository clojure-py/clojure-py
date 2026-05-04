"""Tests for the `py.X/Y` and `py.X.Y` resolver path on RT.class_for_name.

The `py.` prefix gives clojure code an explicit, namespaced way to reach
into Python's runtime — builtins, modules, submodules — without
polluting clojure.core. Mirrors how JVM Clojure scopes Java types under
`java.X`. Used by core.clj to alias Python builtins (Integer, String,
…) and to bind dynamic var defaults like `*out*` to `py.sys/stdout`.
"""

import builtins
import importlib
import os
import sys

import pytest

import clojure.core  # bootstrap

from clojure.lang import (
    Compiler,
    read_string,
    RT,
)


def E(src):
    return Compiler.eval(read_string(src))


# --- RT.class_for_name py-resolver --------------------------------

def test_py_builtins_module():
    assert RT.class_for_name("py.__builtins__") is builtins

def test_py_builtins_int():
    """Slash form 'py.__builtins__/int' is parsed by the compiler into
    ns='py.__builtins__' + name='int'; class_for_name resolves the ns."""
    assert RT.class_for_name("py.__builtins__") is builtins
    # class_for_name sees the full dotted form when called with no slash:
    assert RT.class_for_name("py.__builtins__.int") is int

def test_py_module_top_level():
    assert RT.class_for_name("py.sys") is sys
    assert RT.class_for_name("py.os") is os

def test_py_submodule_dotted():
    """py.os.path → os.path module, even though it's a submodule that
    requires explicit import on some platforms."""
    assert RT.class_for_name("py.os.path") is os.path

def test_py_module_attribute():
    """py.sys.stdout → sys.stdout via getattr."""
    assert RT.class_for_name("py.sys.stdout") is sys.stdout

def test_py_deeply_nested():
    """py.os.path.join → callable os.path.join."""
    assert RT.class_for_name("py.os.path.join") is os.path.join

def test_py_bare_returns_builtins():
    """Bare `py` is treated as builtins for sensible defaults."""
    assert RT.class_for_name("py") is builtins

def test_py_unknown_module_raises():
    with pytest.raises(ImportError):
        RT.class_for_name("py.this_module_should_not_exist_xyz")

def test_py_unknown_attribute_raises():
    with pytest.raises(AttributeError):
        RT.class_for_name("py.sys.this_attr_should_not_exist_xyz")


# --- Slash-form resolution from clojure ---------------------------

def test_clj_py_builtins_slash():
    assert E("py.__builtins__/int") is int
    assert E("py.__builtins__/str") is str
    assert E("py.__builtins__/list") is list

def test_clj_py_builtins_callable():
    assert E("(py.__builtins__/int 3.7)") == 3
    assert E('(py.__builtins__/str 42)') == "42"

def test_clj_py_sys_stdout():
    assert E("py.sys/stdout") is sys.stdout

def test_clj_py_os_sep():
    assert E("py.os/sep") == os.sep

def test_clj_py_os_path_join():
    """Slash on a module attribute that's a callable — invoke it."""
    out = E('(py.os.path/join "a" "b" "c")')
    assert out == os.path.join("a", "b", "c")


# --- Dotted form (no slash) from clojure --------------------------

def test_clj_py_dotted_to_attr():
    """Dotted symbol `py.__builtins__.int` resolves the full path."""
    assert E("py.__builtins__.int") is int

def test_clj_py_dotted_module():
    assert E("py.sys") is sys

def test_clj_py_dotted_submodule():
    assert E("py.os.path") is os.path
