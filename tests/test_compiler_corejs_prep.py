"""Compiler tests — collection literals, class FQN resolution, def
metadata transfer. Prep work needed before translating core.clj."""

import pytest

from clojure.lang import (
    Compiler,
    read_string,
    Var, Symbol, Keyword, RT,
    PersistentVector, PersistentArrayMap, PersistentHashSet, PersistentList,
    IPersistentMap,
)


def _eval(src):
    return Compiler.eval(read_string(src))


def _intern(name, val):
    Var.intern(Compiler.current_ns(), Symbol.intern(name), val)


# --- vector literals --------------------------------------------------

def test_vector_literal_empty():
    v = _eval("[]")
    assert isinstance(v, PersistentVector)
    assert v.count() == 0

def test_vector_literal_with_constants():
    v = _eval("[1 2 3]")
    assert v == PersistentVector.create(1, 2, 3)

def test_vector_literal_evaluates_elements():
    _intern("ccp-incfn", lambda x: x + 1)
    v = _eval("[(ccp-incfn 1) (ccp-incfn 2) (ccp-incfn 3)]")
    assert v == PersistentVector.create(2, 3, 4)


# --- map literals -----------------------------------------------------

def test_map_literal_empty():
    m = _eval("{}")
    assert isinstance(m, IPersistentMap)
    assert m.count() == 0

def test_map_literal_keys_and_vals_evaluated():
    _intern("ccp-mkk", lambda: Keyword.intern(None, "x"))
    _intern("ccp-mkv", lambda: 99)
    m = _eval("{(ccp-mkk) (ccp-mkv)}")
    assert m.val_at(Keyword.intern(None, "x")) == 99


# --- set literals -----------------------------------------------------

def test_set_literal_empty():
    s = _eval("#{}")
    assert isinstance(s, PersistentHashSet)
    assert s.count() == 0

def test_set_literal_with_constants():
    s = _eval("#{1 2 3}")
    assert s.count() == 3
    for x in (1, 2, 3):
        assert s.contains(x)


# --- class FQN resolution --------------------------------------------

def test_unqualified_dotted_resolves_to_class():
    c = _eval("clojure.lang.RT")
    assert c is RT

def test_unqualified_dotted_persistent_vector():
    c = _eval("clojure.lang.PersistentVector")
    assert c is PersistentVector

def test_qualified_static_attribute():
    """`clojure.lang.RT/cons` resolves to the bound method."""
    f = _eval("clojure.lang.RT/cons")
    assert callable(f)
    result = f(99, None)
    assert result.first() == 99

def test_qualified_static_call():
    result = _eval("(clojure.lang.RT/cons :a nil)")
    assert result.first() == Keyword.intern(None, "a")
    assert result.next() is None

def test_dot_form_with_class_target():
    """(. Class (method args)) → Class.method(args)."""
    result = _eval("(. clojure.lang.RT (cons :x nil))")
    assert result.first() == Keyword.intern(None, "x")


# --- def metadata transfer -------------------------------------------

def test_def_doc_metadata():
    v = _eval('(def ^{:doc "the doc"} ccp-x 1)')
    assert v.meta().val_at(Keyword.intern(None, "doc")) == "the doc"

def test_def_added_metadata():
    v = _eval('(def ^{:added "1.0"} ccp-y 2)')
    assert v.meta().val_at(Keyword.intern(None, "added")) == "1.0"

def test_def_arglists_metadata():
    """:arglists in source is typically '([...]). Metadata isn't
    evaluated — the literal `(quote ([x] [x y]))` lands in the meta as-is,
    matching JVM Clojure's behavior. Tools like `doc` unwrap the quote
    when they read it."""
    v = _eval("(def ^{:arglists '([x] [x y])} ccp-al (fn* [x] x))")
    al = v.meta().val_at(Keyword.intern(None, "arglists"))
    assert al == read_string("(quote ([x] [x y]))")

def test_def_macro_metadata_makes_var_a_macro():
    v = _eval("(def ^{:macro true} ccp-mm (fn* [&form &env x] x))")
    assert v.is_macro()

def test_def_macro_shorthand():
    """`^:macro` is shorthand for `^{:macro true}`."""
    v = _eval("(def ^:macro ccp-mm2 (fn* [&form &env x] x))")
    assert v.is_macro()

def test_def_macro_then_use():
    """Defining a macro and using it in a subsequent form."""
    _eval("(def ^:macro ccp-id (fn* [&form &env x] x))")
    assert _eval("(ccp-id 42)") == 42

def test_def_meta_survives_redef():
    """When a Var is redefined, the new metadata replaces the old."""
    _eval("(def ^{:doc \"first\"} ccp-redef 1)")
    _eval("(def ^{:doc \"second\"} ccp-redef 2)")
    v = Compiler.eval(read_string("(var ccp-redef)"))
    assert v.meta().val_at(Keyword.intern(None, "doc")) == "second"
