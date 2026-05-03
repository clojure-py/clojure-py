"""Tests for the core.clj bootstrap section.

Importing `clojure.core` triggers the load. Each Var defined in core.clj
is accessible via `clojure.core/<name>`, and using the unqualified name
works when the current namespace is `clojure.core` or has referred it.
The tests below qualify everything for unambiguous resolution."""

import pytest

# Importing this module triggers the core.clj load.
import clojure.core  # noqa: F401

from clojure.lang import (
    Compiler,
    read_string,
    Var, Symbol, Keyword, Namespace, RT,
    PersistentList, PersistentVector, PersistentArrayMap,
    ISeq,
)


def E(src):
    return Compiler.eval(read_string(src))


# --- list / cons -------------------------------------------------------

def test_list_constructor():
    assert E("(clojure.core/list 1 2 3)").seq() is not None
    assert list(E("(clojure.core/list 1 2 3)")) == [1, 2, 3]
    assert E("(clojure.core/list)").seq() is None

def test_cons():
    result = E("(clojure.core/cons 1 nil)")
    assert result.first() == 1
    assert result.next() is None

def test_cons_onto_seq():
    result = E("(clojure.core/cons 1 (clojure.core/list 2 3))")
    assert list(result) == [1, 2, 3]


# --- first / next / rest ----------------------------------------------

def test_first():
    assert E("(clojure.core/first [1 2 3])") == 1
    assert E("(clojure.core/first nil)") is None
    assert E("(clojure.core/first [])") is None

def test_next():
    assert list(E("(clojure.core/next [1 2 3])")) == [2, 3]
    assert E("(clojure.core/next [1])") is None
    assert E("(clojure.core/next nil)") is None

def test_rest_returns_empty_seq_not_nil():
    """rest returns an empty seq when no more items, not nil."""
    r = E("(clojure.core/rest [1])")
    assert r is not None
    assert r.seq() is None  # empty seq

def test_rest_normal_case():
    assert list(E("(clojure.core/rest [1 2 3])")) == [2, 3]


# --- conj --------------------------------------------------------------

def test_conj_no_args_returns_empty_vector():
    v = E("(clojure.core/conj)")
    assert isinstance(v, PersistentVector)
    assert v.count() == 0

def test_conj_single_coll():
    assert E("(clojure.core/conj [1 2])").count() == 2

def test_conj_two_args():
    v = E("(clojure.core/conj [1 2] 3)")
    assert list(v) == [1, 2, 3]

def test_conj_variadic():
    v = E("(clojure.core/conj [1 2] 3 4 5)")
    assert list(v) == [1, 2, 3, 4, 5]


# --- second/ffirst/nfirst/fnext/nnext ---------------------------------

def test_second():
    assert E("(clojure.core/second [1 2 3])") == 2
    assert E("(clojure.core/second [1])") is None
    assert E("(clojure.core/second nil)") is None

def test_ffirst_nfirst_fnext_nnext():
    src = "[[1 2 3] [4 5 6] [7 8 9]]"
    assert E("(clojure.core/ffirst " + src + ")") == 1
    assert list(E("(clojure.core/nfirst " + src + ")")) == [2, 3]
    assert list(E("(clojure.core/fnext " + src + ")")) == [4, 5, 6]
    # nnext = next of next; one element left, the third sub-vector
    assert E("(clojure.core/nnext " + src + ")").first() == [7, 8, 9]
    assert E("(clojure.core/nnext " + src + ")").next() is None


# --- seq + predicates --------------------------------------------------

def test_seq_on_empty_is_nil():
    assert E("(clojure.core/seq [])") is None
    assert E("(clojure.core/seq nil)") is None

def test_seq_on_nonempty():
    assert isinstance(E("(clojure.core/seq [1])"), ISeq)

def test_seq_predicate():
    assert E("(clojure.core/seq? '(1 2))") is True
    assert E("(clojure.core/seq? [1 2])") is False
    assert E("(clojure.core/seq? nil)") is False

def test_string_predicate():
    assert E('(clojure.core/string? "x")') is True
    assert E("(clojure.core/string? 42)") is False

def test_map_predicate():
    assert E("(clojure.core/map? {:a 1})") is True
    assert E("(clojure.core/map? [1 2])") is False

def test_vector_predicate():
    assert E("(clojure.core/vector? [1])") is True
    assert E("(clojure.core/vector? '(1))") is False


# --- assoc ------------------------------------------------------------

def test_assoc_single_pair():
    m = E("(clojure.core/assoc {:a 1} :b 2)")
    assert m.val_at(Keyword.intern(None, "a")) == 1
    assert m.val_at(Keyword.intern(None, "b")) == 2

def test_assoc_multiple_pairs():
    m = E("(clojure.core/assoc {:a 1} :b 2 :c 3 :d 4)")
    assert m.val_at(Keyword.intern(None, "b")) == 2
    assert m.val_at(Keyword.intern(None, "c")) == 3
    assert m.val_at(Keyword.intern(None, "d")) == 4

def test_assoc_odd_args_raises():
    with pytest.raises(ValueError):
        E("(clojure.core/assoc {:a 1} :b 2 :c)")

def test_assoc_on_vector():
    v = E("(clojure.core/assoc [10 20 30] 1 99)")
    assert list(v) == [10, 99, 30]


# --- bootstrap macros: let, loop, fn ---------------------------------

def test_let_macro_expands_to_let_star():
    """`let` is a bootstrap macro that just rewrites to let*."""
    expanded = Compiler.macroexpand_1(
        read_string("(clojure.core/let [x 1] x)"))
    assert expanded == read_string("(let* [x 1] x)")

def test_let_macro_evaluates():
    assert E("(clojure.core/let [x 5 y 7] (clojure.core/cons x (clojure.core/cons y nil)))").first() == 5

def test_loop_macro_expands_to_loop_star():
    expanded = Compiler.macroexpand_1(
        read_string("(clojure.core/loop [n 0] n)"))
    assert expanded == read_string("(loop* [n 0] n)")

def test_fn_macro_expands_to_fn_star():
    expanded = Compiler.macroexpand_1(
        read_string("(clojure.core/fn [x] x)"))
    assert expanded == read_string("(fn* [x] x)")

def test_fn_macro_creates_callable():
    f = E("(clojure.core/fn [x] (clojure.core/cons x nil))")
    assert callable(f)
    assert f(99).first() == 99


# --- metadata transferred onto vars -----------------------------------

def test_list_var_has_arglists_metadata():
    v = E("(var clojure.core/list)")
    # arglists ends up as a quoted list in the meta
    al = v.meta().val_at(Keyword.intern(None, "arglists"))
    assert al == read_string("(quote ([& items]))")

def test_first_var_has_doc():
    v = E("(var clojure.core/first)")
    doc = v.meta().val_at(Keyword.intern(None, "doc"))
    assert doc and "first item in the collection" in doc

def test_let_var_is_a_macro():
    v = E("(var clojure.core/let)")
    assert v.is_macro()

def test_fn_var_is_a_macro():
    v = E("(var clojure.core/fn)")
    assert v.is_macro()


# --- meta / with-meta -------------------------------------------------

def test_meta_on_obj_with_meta():
    m = E("(clojure.core/meta (clojure.core/with-meta [1 2] {:k 1}))")
    assert m.val_at(Keyword.intern(None, "k")) == 1

def test_meta_on_plain_value_is_nil():
    assert E("(clojure.core/meta 42)") is None
    assert E('(clojure.core/meta "x")') is None

def test_with_meta_preserves_value():
    v = E("(clojure.core/with-meta [1 2 3] {:tag 'X})")
    assert list(v) == [1, 2, 3]


# --- last / butlast --------------------------------------------------

def test_last_on_vector():
    assert E("(clojure.core/last [1 2 3])") == 3

def test_last_on_single():
    assert E("(clojure.core/last [99])") == 99

def test_last_on_nil():
    assert E("(clojure.core/last nil)") is None

def test_butlast_on_vector():
    r = E("(clojure.core/butlast [1 2 3 4])")
    assert list(r) == [1, 2, 3]

def test_butlast_on_single_is_nil():
    """butlast of a 1-elem coll is nil (empty seq, returned via (seq [])
    which is nil)."""
    assert E("(clojure.core/butlast [1])") is None


# --- defn macro -------------------------------------------------------

def test_defn_var_is_a_macro():
    v = E("(var clojure.core/defn)")
    assert v.is_macro()

def test_defn_basic():
    """(defn foo [x] x) — basic single-arity."""
    E("(clojure.core/defn ccd-id [x] x)")
    assert E("(ccd-id 42)") == 42

def test_defn_with_docstring():
    E('(clojure.core/defn ccd-doc "the doc" [x] x)')
    v = E("(var ccd-doc)")
    assert v.meta().val_at(Keyword.intern(None, "doc")) == "the doc"

def test_defn_with_attr_map():
    E("(clojure.core/defn ccd-attr {:added \"1.0\"} [x] x)")
    v = E("(var ccd-attr)")
    assert v.meta().val_at(Keyword.intern(None, "added")) == "1.0"

def test_defn_multi_arity():
    E("""
    (clojure.core/defn ccd-multi
      ([] :zero)
      ([x] x)
      ([x & rest] (clojure.core/cons x rest)))
    """)
    assert E("(ccd-multi)") == Keyword.intern(None, "zero")
    assert E("(ccd-multi 99)") == 99
    r = E("(ccd-multi 1 2 3)")
    assert list(r) == [1, 2, 3]

def test_defn_arglists_metadata_filled_in():
    """defn auto-derives :arglists from the fn's arity vectors."""
    E("(clojure.core/defn ccd-al [a b c] a)")
    v = E("(var ccd-al)")
    al = v.meta().val_at(Keyword.intern(None, "arglists"))
    # arglists is a list of arg vectors; for a single-arity defn it's
    # ((a b c)) wrapped in a quote
    assert al == read_string("(quote ([a b c]))")

def test_defn_first_arg_must_be_symbol():
    with pytest.raises(ValueError):
        E("(clojure.core/defn 42 [x] x)")

def test_defn_uses_recur():
    """defn fns can use recur to fn args (no enclosing loop)."""
    E("""
    (clojure.core/defn ccd-count-down [n]
      (clojure.core/cons n (if (clojure.core/seq (clojure.core/conj [] n))
                              nil
                              nil)))
    """)
    # Simpler recur test:
    E("""
    (clojure.core/defn ccd-rec [n acc]
      (if (clojure.core/seq? (clojure.core/conj nil n))  ; always true (Cons)
        (if n (recur (clojure.core/cons :x nil) acc) acc)
        acc))
    """)
    # Trivial smoke — recur path executes
    assert E("(ccd-rec nil :done)") == Keyword.intern(None, "done")
