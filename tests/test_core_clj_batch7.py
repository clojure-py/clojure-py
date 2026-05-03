"""Tests for core.clj batch 7 (lines 1573-1850):

keys, vals, key, val, rseq,
name, namespace, boolean,
ident? family (simple/qualified),
locking, .., -> and ->> (threading macros),
deref (forward def), check-valid-options,
defmulti, defmethod, remove-all-methods, remove-method,
prefer-method, methods, get-method, prefers
"""

import pytest

import clojure.core  # triggers load

from clojure.lang import (
    Compiler,
    read_string,
    Var, Symbol, Keyword, Namespace, RT,
    PersistentVector, PersistentArrayMap,
    MapEntry, MultiFn,
)


def E(src):
    return Compiler.eval(read_string(src))


def K(name):
    return Keyword.intern(None, name)


# --- keys / vals --------------------------------------------------

def test_keys_returns_seq():
    ks = list(E("(clojure.core/keys {:a 1 :b 2})"))
    assert set(ks) == {K("a"), K("b")}

def test_vals_returns_seq():
    vs = list(E("(clojure.core/vals {:a 1 :b 2})"))
    assert set(vs) == {1, 2}

def test_keys_empty_is_nil():
    assert E("(clojure.core/keys {})") is None

def test_vals_empty_is_nil():
    assert E("(clojure.core/vals {})") is None


# --- key / val (on map entries) -----------------------------------

def test_key_of_entry():
    assert E("(clojure.core/key (clojure.core/find {:a 1} :a))") == K("a")

def test_val_of_entry():
    assert E("(clojure.core/val (clojure.core/find {:a 99} :a))") == 99


# --- rseq ---------------------------------------------------------

def test_rseq_vector():
    assert list(E("(clojure.core/rseq [1 2 3 4 5])")) == [5, 4, 3, 2, 1]

def test_rseq_empty_returns_nil():
    assert E("(clojure.core/rseq [])") is None

def test_rseq_sorted_map():
    rs = list(E("(clojure.core/rseq (clojure.core/sorted-map :a 1 :b 2 :c 3))"))
    keys = [e.key().get_name() for e in rs]
    assert keys == ["c", "b", "a"]


# --- name / namespace --------------------------------------------

def test_name_string_passthrough():
    assert E('(clojure.core/name "hello")') == "hello"

def test_name_symbol():
    assert E("(clojure.core/name 'foo)") == "foo"

def test_name_keyword():
    assert E("(clojure.core/name :foo)") == "foo"

def test_name_qualified_keyword_strips_ns():
    assert E("(clojure.core/name :a/b)") == "b"

def test_namespace_unqualified_is_nil():
    assert E("(clojure.core/namespace 'foo)") is None
    assert E("(clojure.core/namespace :foo)") is None

def test_namespace_qualified():
    assert E("(clojure.core/namespace :a/b)") == "a"
    assert E("(clojure.core/namespace 'x/y)") == "x"


# --- boolean coercion ---------------------------------------------

def test_boolean_nil_is_false():
    assert E("(clojure.core/boolean nil)") is False

def test_boolean_false_is_false():
    assert E("(clojure.core/boolean false)") is False

def test_boolean_zero_is_true():
    """Clojure's only falsy values are nil and false."""
    assert E("(clojure.core/boolean 0)") is True

def test_boolean_empty_string_is_true():
    assert E('(clojure.core/boolean "")') is True

def test_boolean_truthy_value():
    assert E("(clojure.core/boolean :anything)") is True


# --- ident? family ------------------------------------------------

def test_ident_p():
    assert E("(clojure.core/ident? 'foo)") is True
    assert E("(clojure.core/ident? :foo)") is True
    assert E("(clojure.core/ident? 42)") is False
    assert E('(clojure.core/ident? "x")') is False

def test_simple_ident_p():
    assert E("(clojure.core/simple-ident? 'foo)") is True
    assert E("(clojure.core/simple-ident? :foo)") is True
    assert E("(clojure.core/simple-ident? :a/b)") is False
    assert E("(clojure.core/simple-ident? 'a/b)") is False

def test_qualified_ident_p():
    assert E("(clojure.core/qualified-ident? :a/b)") is True
    assert E("(clojure.core/qualified-ident? 'a/b)") is True
    assert E("(clojure.core/qualified-ident? :foo)") is False

def test_simple_qualified_keyword_and_symbol():
    assert E("(clojure.core/simple-keyword? :foo)") is True
    assert E("(clojure.core/qualified-keyword? :a/b)") is True
    assert E("(clojure.core/simple-symbol? 'foo)") is True
    assert E("(clojure.core/qualified-symbol? 'a/b)") is True
    assert E("(clojure.core/simple-keyword? :a/b)") is False
    assert E("(clojure.core/qualified-symbol? 'foo)") is False


# --- threading macros -> and ->> ---------------------------------

def test_thread_first_simple():
    """(-> 5 inc inc) → (inc (inc 5))."""
    assert E("(clojure.core/-> 5 clojure.core/inc clojure.core/inc)") == 7

def test_thread_first_with_arg_form():
    """(-> 5 (+ 10)) → (+ 5 10)."""
    assert E("(clojure.core/-> 5 (clojure.core/+ 10))") == 15

def test_thread_first_chained():
    """(-> 1 (+ 2) (* 3) (- 4)) → ((1 + 2) * 3) - 4 = 5."""
    assert E("(clojure.core/-> 1 (clojure.core/+ 2) (clojure.core/* 3) (clojure.core/- 4))") == 5

def test_thread_last_simple():
    """(->> [1 2 3] (apply +)) — coll goes last."""
    assert E("(clojure.core/->> [1 2 3] (clojure.core/apply clojure.core/+))") == 6

def test_thread_last_chained():
    """(->> [1 2 3] (clojure.core/apply +) (* 10)) → 60."""
    assert E(
        "(clojure.core/->> [1 2 3] (clojure.core/apply clojure.core/+) (clojure.core/* 10))"
    ) == 60


# --- .. dot-dot --------------------------------------------------

def test_dot_dot_chains_member_access():
    """(.. \"hello\" upper) → (.upper \"hello\") → \"HELLO\"."""
    assert E('(clojure.core/.. "hello" upper)') == "HELLO"

def test_dot_dot_multi_step():
    """(.. \"abc\" upper (replace \"A\" \"X\")) → \"XBC\"."""
    assert E('(clojure.core/.. "abc" upper (replace "A" "X"))') == "XBC"


# --- deref --------------------------------------------------------

def test_deref_var():
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb7-d"), 42)
    assert E("(clojure.core/deref (var tcb7-d))") == 42


# --- defmulti / defmethod ----------------------------------------

def test_defmulti_basic():
    core_ns = Namespace.find_or_create(Symbol.intern("clojure.core"))
    RT.CURRENT_NS.bind_root(core_ns)
    try:
        E("(defmulti tcb7d-mm clojure.core/identity)")
        E("(defmethod tcb7d-mm :a [_] :got-a)")
        E("(defmethod tcb7d-mm :b [_] :got-b)")
        E("(defmethod tcb7d-mm :default [_] :default-case)")
        assert E("(tcb7d-mm :a)") == K("got-a")
        assert E("(tcb7d-mm :b)") == K("got-b")
        assert E("(tcb7d-mm :unknown)") == K("default-case")
    finally:
        user_ns = Namespace.find_or_create(Symbol.intern("user"))
        RT.CURRENT_NS.bind_root(user_ns)

def test_defmulti_returns_multifn():
    core_ns = Namespace.find_or_create(Symbol.intern("clojure.core"))
    RT.CURRENT_NS.bind_root(core_ns)
    try:
        E("(defmulti tcb7d-mm2 clojure.core/identity)")
        v = E("(var tcb7d-mm2)")
        assert isinstance(v.deref(), MultiFn)
    finally:
        user_ns = Namespace.find_or_create(Symbol.intern("user"))
        RT.CURRENT_NS.bind_root(user_ns)

def test_methods_returns_dispatch_table():
    core_ns = Namespace.find_or_create(Symbol.intern("clojure.core"))
    RT.CURRENT_NS.bind_root(core_ns)
    try:
        E("(defmulti tcb7d-mt clojure.core/identity)")
        E("(defmethod tcb7d-mt :x [_] :x)")
        E("(defmethod tcb7d-mt :y [_] :y)")
        m = E("(clojure.core/methods tcb7d-mt)")
        keys = [k for k in m.seq()]
        # Returns a map dispatch-val → fn; we can pull keys via val_at on each entry
        actual_keys = set()
        s = m.seq()
        while s is not None:
            actual_keys.add(s.first().key())
            s = s.next()
        assert K("x") in actual_keys
        assert K("y") in actual_keys
    finally:
        user_ns = Namespace.find_or_create(Symbol.intern("user"))
        RT.CURRENT_NS.bind_root(user_ns)

def test_remove_method():
    core_ns = Namespace.find_or_create(Symbol.intern("clojure.core"))
    RT.CURRENT_NS.bind_root(core_ns)
    try:
        E("(defmulti tcb7d-rm clojure.core/identity)")
        E("(defmethod tcb7d-rm :a [_] :got-a)")
        E("(defmethod tcb7d-rm :default [_] :default-case)")
        assert E("(tcb7d-rm :a)") == K("got-a")
        E("(clojure.core/remove-method tcb7d-rm :a)")
        assert E("(tcb7d-rm :a)") == K("default-case")
    finally:
        user_ns = Namespace.find_or_create(Symbol.intern("user"))
        RT.CURRENT_NS.bind_root(user_ns)
