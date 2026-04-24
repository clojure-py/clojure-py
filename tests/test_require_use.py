"""Tests for `require`, `use`, `load-file`, `(ns …)`, and the underlying
loader plumbing (`in-ns`, `*loaded-libs*`).
"""

import os
import sys
import tempfile
import textwrap
import pytest
from clojure._core import eval_string


def _ev(s):
    return eval_string(s)


@pytest.fixture
def libdir():
    """A fresh temp dir on sys.path + clean *loaded-libs*, sys.modules,
    and stale clojure_test_libs_* sys.path entries. Tests share global
    state otherwise."""
    # Strip any leftover libdirs from earlier tests so find-source-file
    # only finds the file we're about to write.
    for entry in list(sys.path):
        if isinstance(entry, str) and "clojure_test_libs_" in entry:
            sys.path.remove(entry)
    d = tempfile.mkdtemp(prefix="clojure_test_libs_")
    sys.path.insert(0, d)
    _ev("(reset! *loaded-libs* #{})")
    stale = [k for k in sys.modules if k.startswith("rqlib.")]
    for k in stale:
        del sys.modules[k]
    try:
        yield d
    finally:
        if d in sys.path:
            sys.path.remove(d)


def _write(libdir, ns_path, source):
    """Write `source` to `<libdir>/<ns/path>.clj`. ns_path uses '/' separators."""
    full = os.path.join(libdir, *ns_path.split("/"))
    full += ".clj"
    os.makedirs(os.path.dirname(full), exist_ok=True)
    with open(full, "w") as f:
        f.write(textwrap.dedent(source).lstrip())
    return full


# --- load-file ---


def test_load_file_basic(libdir):
    path = _write(libdir, "scratch", '(def --lf-x 42)\n--lf-x\n')
    _ev('(load-file "%s")' % path.replace("\\", "/"))
    assert _ev("--lf-x") == 42


# --- require: basic ---


def test_require_loads_file(libdir):
    _write(libdir, "rqlib/a", """
        (ns rqlib.a)
        (defn alpha [] :alpha)
    """)
    _ev("(require 'rqlib.a)")
    assert _ev("(rqlib.a/alpha)") == _ev(":alpha")


def test_require_idempotent(libdir):
    _write(libdir, "rqlib/b", """
        (ns rqlib.b)
        (def --rqb-counter (atom 0))
        (swap! --rqb-counter inc)
    """)
    _ev("(require 'rqlib.b)")
    _ev("(require 'rqlib.b)")
    _ev("(require 'rqlib.b)")
    # Body ran exactly once.
    assert _ev("@rqlib.b/--rqb-counter") == 1


def test_require_tracks_loaded_libs(libdir):
    _write(libdir, "rqlib/c", "(ns rqlib.c)")
    _ev("(require 'rqlib.c)")
    libs = {str(s) for s in _ev("@*loaded-libs*")}
    assert "rqlib.c" in libs


# --- require :as ---


def test_require_as_alias(libdir):
    _write(libdir, "rqlib/d", """
        (ns rqlib.d)
        (defn greet [n] (str "hello " n))
    """)
    _ev("(require '[rqlib.d :as d])")
    assert _ev('(d/greet "world")') == "hello world"


def test_require_as_in_loaded_file(libdir):
    _write(libdir, "rqlib/e1", """
        (ns rqlib.e1)
        (defn shout [s] (str s "!"))
    """)
    _write(libdir, "rqlib/e2", """
        (ns rqlib.e2 (:require [rqlib.e1 :as e]))
        (defn loud [s] (e/shout s))
    """)
    _ev("(require 'rqlib.e2)")
    assert _ev('(rqlib.e2/loud "hi")') == "hi!"


# --- require :refer ---


def test_require_refer_specific(libdir):
    _write(libdir, "rqlib/f", """
        (ns rqlib.f)
        (defn one [] 1)
        (defn two [] 2)
        (defn three [] 3)
    """)
    _ev("(require '[rqlib.f :refer [one three]])")
    assert _ev("(one)") == 1
    assert _ev("(three)") == 3
    # `two` was NOT referred → fully-qualified still works.
    assert _ev("(rqlib.f/two)") == 2


# --- use ---


def test_use_refers_all_publics(libdir):
    _write(libdir, "rqlib/g", """
        (ns rqlib.g)
        (defn aa [] :aa)
        (defn bb [] :bb)
    """)
    _ev("(use 'rqlib.g)")
    assert _ev("(aa)") == _ev(":aa")
    assert _ev("(bb)") == _ev(":bb")


def test_use_with_only_filter(libdir):
    _write(libdir, "rqlib/h", """
        (ns rqlib.h)
        (defn xx [] :xx)
        (defn yy [] :yy)
    """)
    _ev("(use '[rqlib.h :only [xx]])")
    assert _ev("(xx)") == _ev(":xx")


# --- (ns …) macro creates the namespace + applies directives ---


def test_ns_macro_creates_namespace_with_require(libdir):
    _write(libdir, "nsmacro/i", "(ns nsmacro.i) (defn p [] :p)")
    _write(libdir, "nsmacro/j", """
        (ns nsmacro.j (:require [nsmacro.i :as i]))
        (defn q [] (i/p))
    """)
    _ev("(require 'nsmacro.j)")
    assert _ev("(nsmacro.j/q)") == _ev(":p")


def test_ns_macro_with_use_refers(libdir):
    _write(libdir, "rqlib/k", "(ns rqlib.k) (defn k-fn [] :k-fn)")
    _write(libdir, "rqlib/l", """
        (ns rqlib.l (:use rqlib.k))
        (defn use-it [] (k-fn))
    """)
    _ev("(require 'rqlib.l)")
    assert _ev("(rqlib.l/use-it)") == _ev(":k-fn")


def test_ns_loaded_files_can_call_clojure_core(libdir):
    # The (ns …) macro auto-refers clojure.core.
    _write(libdir, "rqlib/m", """
        (ns rqlib.m)
        (defn make-vec [] (vec (range 3)))
    """)
    _ev("(require 'rqlib.m)")
    assert list(_ev("(rqlib.m/make-vec)")) == [0, 1, 2]


# --- in-ns ---


def test_in_ns_switches_target_for_subsequent_forms(libdir):
    _write(libdir, "rqlib/n", """
        (in-ns 'rqlib.n)
        (def --in-ns-x 100)
    """)
    _ev('(load-file "%s")' % os.path.join(libdir, "rqlib", "n.clj").replace("\\", "/"))
    assert _ev("rqlib.n/--in-ns-x") == 100
