"""Tests for core.clj batch 33: ns macro + require / use / load
machinery (selected from JVM 5810-6205).

Forms (12):
  in-ns,
  ns (macro),
  gen-class (no-op stub),
  throw-if (private),
  load-one / load-all / load-lib / load-libs / check-cyclic-dependency
    (private),
  require, use, load.

Backend additions:
  RT.in_ns(sym)
    Switches *ns* to the named namespace, creating it if absent.
    Uses thread-set when *ns* is thread-bound, else bind_root —
    matches JVM's REPL-vs-load behavior.

  RT.load(path)
    File-system search for `.clj` / `.cljc` resources matching a
    forward-slash-separated path on `sys.path`. Calls
    Compiler.load_file on the first match.

  Compiler.load_file now pushes a thread-binding for *ns* around
    the file's evaluation. Without this, a (require 'inner) inside
    an outer file would leak `inner`'s ns over the outer file's
    remaining forms.

Adaptations from JVM source:
  ns macro:
    - with-loading-context elided (JVM ClassLoader machinery has no
      Python analog).
    - .resetMeta → .reset_meta (snake_case).
    - (.equals 'name 'clojure.core) → (= 'name 'clojure.core).
  gen-class is a no-op stub — JVM uses it for AOT class generation;
    Python's import machinery doesn't need it.
  throw-if simplified — JVM builds a CompilerException with rewritten
    stack trace; we just throw plain Exception with formatted message.
"""

import os
import sys
import tempfile
import shutil

import pytest

import clojure.core  # bootstrap

from clojure.lang import (
    Compiler,
    read_string,
    Var, Symbol, Keyword,
    Namespace,
    PersistentArrayMap,
    RT,
)


def E(src):
    return Compiler.eval(read_string(src))


# Helpers for building a temp lib directory.

@pytest.fixture
def tmp_libs():
    """Create a temp directory, add it to sys.path, yield the path,
    clean up after."""
    d = tempfile.mkdtemp()
    sys.path.insert(0, d)
    try:
        yield d
    finally:
        sys.path.remove(d)
        shutil.rmtree(d, ignore_errors=True)


def _write_lib(root, lib_name, body):
    """Write a .clj file at <root>/<lib-path>.clj. Lib name uses dots
    (e.g. 'foo.bar')."""
    parts = lib_name.split(".")
    dirpath = os.path.join(root, *parts[:-1])
    os.makedirs(dirpath, exist_ok=True)
    path = os.path.join(dirpath, parts[-1] + ".clj")
    with open(path, "w") as f:
        f.write(body)
    return path


def _make_ns_with_core_referred(name):
    """Create a fresh namespace and refer all public clojure.core Vars
    into it, mirroring what the ns macro does. Without this, loading a
    file from this namespace would fail to resolve `ns`, `def`, etc."""
    ns = Namespace.find_or_create(Symbol.intern(name))
    core_ns = Namespace.find(Symbol.intern("clojure.core"))
    for entry in core_ns.get_mappings():
        sym = entry.key()
        v = entry.val()
        if isinstance(v, Var) and v.ns is core_ns and v.is_public():
            ns.refer(sym, v)
    return ns


# --- in-ns --------------------------------------------------------

def test_in_ns_creates_and_switches():
    """in-ns creates the namespace if absent and switches *ns* to it."""
    # Run inside a thread binding so we can restore *ns* after.
    ns_var = (Namespace.find(Symbol.intern("clojure.core"))
                       .find_interned_var(Symbol.intern("*ns*")))
    user_ns = Namespace.find(Symbol.intern("user"))
    Var.push_thread_bindings(PersistentArrayMap.create(ns_var, user_ns))
    try:
        out = E("(in-ns 'tcb33-via-in-ns)")
        assert isinstance(out, Namespace)
        assert str(out) == "tcb33-via-in-ns"
        # *ns* points at the new ns (deref the Var directly).
        assert ns_var.deref() is out
    finally:
        Var.pop_thread_bindings()


# --- ns macro -----------------------------------------------------

def test_ns_macro_creates_ns_with_clojure_core_referred(tmp_libs):
    """A simple (ns foo.simple) creates the ns and refers clojure.core."""
    _write_lib(tmp_libs, "foo.simple", "(ns foo.simple)\n(def x 1)\n")
    E("(require 'foo.simple)")
    fs = Namespace.find(Symbol.intern("foo.simple"))
    assert fs is not None
    # clojure.core/+ should be referred.
    assert fs.get_mapping(Symbol.intern("+")) is not None
    # Local var x exists.
    assert fs.get_mapping(Symbol.intern("x")) is not None
    assert fs.find_interned_var(Symbol.intern("x")).deref() == 1

def test_ns_macro_with_require(tmp_libs):
    _write_lib(tmp_libs, "ns2.dep", "(ns ns2.dep)\n(def answer 42)\n")
    _write_lib(tmp_libs, "ns2.user",
               "(ns ns2.user (:require [ns2.dep :as d]))\n"
               "(def via (str d/answer))\n")
    E("(require 'ns2.user)")
    assert E("ns2.user/via") == "42"

def test_ns_macro_with_use(tmp_libs):
    """:use brings in a lib and refers its publics into the current ns."""
    _write_lib(tmp_libs, "ns3.lib",
               "(ns ns3.lib)\n(defn shout [s] (str s \"!\"))\n")
    _write_lib(tmp_libs, "ns3.consumer",
               "(ns ns3.consumer (:use ns3.lib))\n"
               "(def yelled (shout \"hi\"))\n")
    E("(require 'ns3.consumer)")
    assert E("ns3.consumer/yelled") == "hi!"

def test_ns_macro_with_gen_class_is_no_op(tmp_libs):
    """gen-class is a no-op stub; ns forms with :gen-class still work."""
    _write_lib(tmp_libs, "gc.test",
               "(ns gc.test (:gen-class))\n(def ok :yes)\n")
    E("(require 'gc.test)")
    assert E("gc.test/ok") == Keyword.intern(None, "yes")

def test_ns_macro_excludes_refer_clojure_when_directive_present(tmp_libs):
    """(:refer-clojure :exclude [printf]) should still work with the
    automatic refer being skipped."""
    _write_lib(tmp_libs, "rc.test",
               "(ns rc.test (:refer-clojure :exclude [printf]))\n"
               "(defn printf [& _] :overridden)\n")
    E("(require 'rc.test)")
    # Call the user's printf — the local def, not clojure.core's.
    assert E("(rc.test/printf)") == Keyword.intern(None, "overridden")
    # And from the ns's mapping, `printf` resolves to the local def
    # (the :exclude prevented clojure.core/printf from being referred).
    rc = Namespace.find(Symbol.intern("rc.test"))
    pf = rc.get_mapping(Symbol.intern("printf"))
    assert pf is not None
    assert pf.ns is rc  # owned by rc.test, not referred from clojure.core


# --- require ------------------------------------------------------

def test_require_loads_lib_once(tmp_libs):
    """A second require of the same lib is a no-op."""
    counter = [0]
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb33-poke!"),
               lambda: counter.append(1))
    _write_lib(tmp_libs, "rq1.lib", "(ns rq1.lib)\n(user/tcb33-poke!)\n")
    E("(require 'rq1.lib)")
    E("(require 'rq1.lib)")
    assert sum(counter) == 1

def test_require_with_reload_reruns_load(tmp_libs):
    counter = [0]
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb33-rl-poke!"),
               lambda: counter.append(1))
    _write_lib(tmp_libs, "rq2.lib",
               "(ns rq2.lib)\n(user/tcb33-rl-poke!)\n")
    E("(require 'rq2.lib)")
    E("(require 'rq2.lib :reload)")
    assert sum(counter) == 2

def test_require_with_as_aliases_ns(tmp_libs):
    _write_lib(tmp_libs, "rq3.lib", "(ns rq3.lib)\n(def v 99)\n")
    E("(require '[rq3.lib :as r3])")
    assert E("r3/v") == 99

def test_require_with_refer_brings_specific_vars(tmp_libs):
    _write_lib(tmp_libs, "rq4.lib",
               "(ns rq4.lib)\n(def a 1)\n(def b 2)\n(def c 3)\n")
    ns_var = (Namespace.find(Symbol.intern("clojure.core"))
                       .find_interned_var(Symbol.intern("*ns*")))
    target = _make_ns_with_core_referred("rq4.consumer")
    Var.push_thread_bindings(PersistentArrayMap.create(ns_var, target))
    try:
        E("(clojure.core/require '[rq4.lib :refer [a c]])")
        # `a` and `c` referred, `b` not.
        assert target.get_mapping(Symbol.intern("a")) is not None
        assert target.get_mapping(Symbol.intern("c")) is not None
        assert target.get_mapping(Symbol.intern("b")) is None
    finally:
        Var.pop_thread_bindings()

def test_require_missing_lib_throws():
    with pytest.raises(Exception):
        E("(require 'totally.nonexistent.lib)")

def test_require_records_in_loaded_libs(tmp_libs):
    _write_lib(tmp_libs, "rq5.lib", "(ns rq5.lib)\n(def x 1)\n")
    E("(require 'rq5.lib)")
    libs = E("(loaded-libs)")
    assert any(str(s) == "rq5.lib" for s in libs)


# --- use ----------------------------------------------------------

def test_use_refers_publics_from_lib(tmp_libs):
    _write_lib(tmp_libs, "us1.lib",
               "(ns us1.lib)\n(defn yelp [s] (str s \"!!\"))\n")
    target = _make_ns_with_core_referred("us1.consumer")
    ns_var = (Namespace.find(Symbol.intern("clojure.core"))
                       .find_interned_var(Symbol.intern("*ns*")))
    Var.push_thread_bindings(PersistentArrayMap.create(ns_var, target))
    try:
        E("(clojure.core/use 'us1.lib)")
        # `yelp` got referred into us1.consumer.
        v = target.get_mapping(Symbol.intern("yelp"))
        assert v is not None
        assert v.deref()("hi") == "hi!!"
    finally:
        Var.pop_thread_bindings()


# --- load ---------------------------------------------------------

def test_load_with_absolute_path(tmp_libs):
    """A leading-slash path is classpath-relative — no current-ns prefix."""
    _write_lib(tmp_libs, "ld1.lib",
               "(ns ld1.lib)\n(def loaded :yes)\n")
    E('(load "/ld1/lib")')
    assert E("ld1.lib/loaded") == Keyword.intern(None, "yes")


# --- cyclic dependency detection ---------------------------------

def test_cyclic_dependency_detected(tmp_libs):
    _write_lib(tmp_libs, "cyc.a",
               "(ns cyc.a (:require cyc.b))\n")
    _write_lib(tmp_libs, "cyc.b",
               "(ns cyc.b (:require cyc.a))\n")
    with pytest.raises(Exception, match="Cyclic load dependency"):
        E("(require 'cyc.a)")
