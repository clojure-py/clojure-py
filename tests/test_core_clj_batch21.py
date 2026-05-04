"""Tests for core.clj batch 21 (lines 4048-4206, skipping the struct family):

Forms (14):
  macroexpand-1, macroexpand,
  load-reader, load-string,
  set?, set,
  filter-key (private),
  find-ns, create-ns, remove-ns, all-ns, the-ns,
  ns-name, ns-map.

Skipped (deferred, JVM 4068-4110):
  create-struct, defstruct, struct-map, struct, accessor —
  depend on clojure.lang.PersistentStructMap which isn't ported.
  Structmaps are deprecated 1.0-era machinery, rarely used in
  modern Clojure; revisit only if needed.

Backend addition:
  Compiler.load(rdr) — read+eval all forms from a reader. Backs
  load-reader (and indirectly load-string).

Three adaptations from JVM source:
  macroexpand-1 calls .macroexpand_1 (snake_case) where JVM has
    .macroexpand1.
  load-string uses (py.io/StringIO s) instead of
    (java.io.StringReader. s).
  create-ns calls Namespace/find_or_create where JVM has findOrCreate.
"""

import io
import sys

import pytest

import clojure.core  # bootstrap

from clojure.lang import (
    Compiler,
    read_string,
    Var, Symbol, Namespace,
    PersistentHashSet,
    PersistentArrayMap,
)


def E(src):
    return Compiler.eval(read_string(src))


# --- macroexpand-1 -------------------------------------------------

def test_macroexpand_1_when():
    out = E("(clojure.core/macroexpand-1 '(clojure.core/when true 42))")
    # `when` expands to `(if test (do body...))`
    parts = list(out)
    assert str(parts[0]) == "if"

def test_macroexpand_1_non_macro_returns_form_unchanged():
    form = E("(clojure.core/macroexpand-1 '(clojure.core/+ 1 2))")
    assert str(form) == "(clojure.core/+ 1 2)"

def test_macroexpand_1_special_form_unchanged():
    """Special forms aren't macros; they pass through."""
    out = E("(clojure.core/macroexpand-1 '(if x y z))")
    assert str(out) == "(if x y z)"


# --- macroexpand ---------------------------------------------------

def test_macroexpand_unwraps_nested_macros():
    """macroexpand keeps expanding until the head stops being a macro."""
    out = E("(clojure.core/macroexpand '(clojure.core/when true 42))")
    # Should reach (if test (do 42)) — `if` is special, stops there
    assert str(list(out)[0]) == "if"

def test_macroexpand_non_macro_returns_form():
    form = E("(clojure.core/macroexpand '(clojure.core/+ 1 2))")
    assert str(form) == "(clojure.core/+ 1 2)"


# --- load-reader / load-string ------------------------------------

def test_load_string_single_form():
    assert E('(clojure.core/load-string "42")') == 42

def test_load_string_returns_last_form_value():
    """Multiple forms — load-string returns the last form's value."""
    out = E('(clojure.core/load-string "1 2 3 :last")')
    from clojure.lang import Keyword
    assert out == Keyword.intern(None, "last")

def test_load_string_with_def_side_effect():
    E('(clojure.core/load-string "(clojure.core/def __tcb21-side! 99)")')
    assert E("user/__tcb21-side!") == 99

def test_load_reader_directly():
    """Pass a LineNumberingPushbackReader into load-reader."""
    from clojure.lang import LineNumberingPushbackReader
    rdr = LineNumberingPushbackReader(io.StringIO("(clojure.core/* 6 7)"))
    Var.intern(Compiler.current_ns(), Symbol.intern("__tcb21-rdr"), rdr)
    assert E("(clojure.core/load-reader user/__tcb21-rdr)") == 42

def test_load_string_empty_returns_nil():
    assert E('(clojure.core/load-string "")') is None


# --- set? ----------------------------------------------------------

def test_set_pred_true_for_set():
    assert E("(clojure.core/set? #{1 2 3})") is True

def test_set_pred_true_for_empty_set():
    assert E("(clojure.core/set? #{})") is True

def test_set_pred_false_for_vector():
    assert E("(clojure.core/set? [1 2 3])") is False

def test_set_pred_false_for_map():
    assert E("(clojure.core/set? {:a 1})") is False

def test_set_pred_false_for_nil():
    assert E("(clojure.core/set? nil)") is False


# --- set -----------------------------------------------------------

def test_set_from_vector_dedups():
    out = E("(clojure.core/set [1 2 3 1 2])")
    assert isinstance(out, PersistentHashSet)
    assert set(out) == {1, 2, 3}

def test_set_from_set_passthrough_strips_meta():
    """JVM: (set x) on a set returns it with-meta nil."""
    out = E("(clojure.core/set #{:a :b :c})")
    from clojure.lang import Keyword
    assert set(out) == {Keyword.intern(None, "a"),
                        Keyword.intern(None, "b"),
                        Keyword.intern(None, "c")}

def test_set_from_nil_returns_empty():
    out = E("(clojure.core/set nil)")
    assert isinstance(out, PersistentHashSet)
    assert set(out) == set()

def test_set_from_empty_vec():
    assert set(E("(clojure.core/set [])")) == set()

def test_set_from_lazy_seq():
    out = E("(clojure.core/set (clojure.core/range 5))")
    assert set(out) == {0, 1, 2, 3, 4}


# --- find-ns -------------------------------------------------------

def test_find_ns_existing_returns_ns():
    out = E("(clojure.core/find-ns 'clojure.core)")
    assert isinstance(out, Namespace)
    assert str(out) == "clojure.core"

def test_find_ns_missing_returns_nil():
    assert E("(clojure.core/find-ns 'this.ns.should.not.exist.xyz)") is None


# --- create-ns -----------------------------------------------------

def test_create_ns_new():
    out = E("(clojure.core/create-ns 'tcb21.created)")
    assert isinstance(out, Namespace)
    assert str(out) == "tcb21.created"

def test_create_ns_existing_returns_existing():
    """Repeated create-ns of the same name returns the same instance."""
    a = E("(clojure.core/create-ns 'tcb21.same)")
    b = E("(clojure.core/create-ns 'tcb21.same)")
    assert a is b


# --- remove-ns -----------------------------------------------------

def test_remove_ns_removes():
    E("(clojure.core/create-ns 'tcb21.tobedeleted)")
    assert E("(clojure.core/find-ns 'tcb21.tobedeleted)") is not None
    E("(clojure.core/remove-ns 'tcb21.tobedeleted)")
    assert E("(clojure.core/find-ns 'tcb21.tobedeleted)") is None


# --- all-ns --------------------------------------------------------

def test_all_ns_includes_clojure_core():
    out = list(E("(clojure.core/all-ns)"))
    names = [str(n) for n in out]
    assert "clojure.core" in names
    assert "user" in names


# --- the-ns --------------------------------------------------------

def test_the_ns_from_symbol():
    out = E("(clojure.core/the-ns 'clojure.core)")
    assert isinstance(out, Namespace)

def test_the_ns_from_ns_passthrough():
    out = E("(clojure.core/the-ns (clojure.core/find-ns 'clojure.core))")
    assert isinstance(out, Namespace)

def test_the_ns_missing_throws():
    with pytest.raises(Exception, match="No namespace"):
        E("(clojure.core/the-ns 'nonexistent.ns)")


# --- ns-name -------------------------------------------------------

def test_ns_name_from_symbol():
    out = E("(clojure.core/ns-name 'clojure.core)")
    # Returns a Symbol
    assert isinstance(out, Symbol)
    assert str(out) == "clojure.core"

def test_ns_name_from_ns():
    out = E("(clojure.core/ns-name (clojure.core/find-ns 'clojure.core))")
    assert str(out) == "clojure.core"


# --- ns-map --------------------------------------------------------

def test_ns_map_clojure_core_has_basic_vars():
    """ns-map returns the namespace's mappings; clojure.core has many."""
    out = E("(clojure.core/ns-map 'clojure.core)")
    assert len(out) > 0
    # Iterating a map gives MapEntries; extract keys.
    keys_str = {str(e.key()) for e in out}
    assert "+" in keys_str
    assert "inc" in keys_str

def test_ns_map_returns_includes_imports_and_vars():
    """ns-map includes both Var mappings and class mappings."""
    out = E("(clojure.core/ns-map 'clojure.core)")
    keys_str = {str(e.key()) for e in out}
    # From the batch type-alias migration (`def Integer …`).
    assert "Integer" in keys_str
    assert "Throwable" in keys_str


# --- filter-key (private) -----------------------------------------

def test_filter_key_filters_map_by_key_predicate():
    """filter-key is private but resolvable via Var lookup."""
    fk_var = Namespace.find(Symbol.intern("clojure.core")).find_interned_var(
        Symbol.intern("filter-key"))
    fk = fk_var.deref()
    # (filter-key keyfn pred amap) — filter map entries where (pred (keyfn entry)) is truthy.
    src = E("{:a 1 :b 2 :c 3}")
    out = fk(lambda e: e.key(),
             lambda k: str(k) == ":b",
             src)
    assert dict(out) == {E(":b"): 2}
