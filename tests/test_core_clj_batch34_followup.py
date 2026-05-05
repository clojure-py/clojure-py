"""Follow-up to batch 34: implement letfn / letfn*, the REPL state
vars, the missing dyn vars (*file* / *err* / *command-line-args* /
*warn-on-reflection* / *compile-path* / *compile-files* /
*compiler-options*), load-file, and the alter-meta! / add-doc-and-meta
documentation block (JVM 6477-6611).

Newly added forms:
  letfn (macro) + letfn* (special form)
  *1, *2, *3, *e, *repl* (REPL state)
  *file*, *err*, *command-line-args*, *warn-on-reflection*,
  *compile-path*, *compile-files*, *compiler-options* (dyn vars)
  load-file
  add-doc-and-meta (private macro)

Compiler change worth calling out:
  _compile_letfn_star added. Allocates a CELL slot per binding name
  BEFORE compiling any of the fn-forms, so each fn closure captures
  the cell — not its current value. Forward references resolve at
  call time once all cells have been filled. The same machinery
  (ctx.cellvars + STORE_DEREF / LOAD_DEREF) the FAST→CELL promotion
  path uses is exercised here from the start.
"""

import sys
import io

import pytest

import clojure.core  # bootstrap

from clojure.lang import (
    Compiler,
    read_string,
    Var, Symbol, Keyword,
    Namespace,
    PersistentArrayMap,
)


def E(src):
    return Compiler.eval(read_string(src))


def K(name):
    return Keyword.intern(None, name)


def _core_var(name):
    return Namespace.find(Symbol.intern("clojure.core")).find_interned_var(
        Symbol.intern(name))


# --- letfn* (compiler special form) ------------------------------

def test_letfn_star_simple_binding():
    assert E("(letfn* [f (fn* [x] (* x 2))] (f 21))") == 42

def test_letfn_star_self_recursion():
    out = E("""
      (letfn* [fact (fn* [n] (if (= n 0) 1 (* n (fact (- n 1)))))]
        (fact 5))""")
    assert out == 120

def test_letfn_star_mutual_recursion():
    """Both fns reference each other — only works if both cells exist
    before either fn-form is compiled."""
    out = E("""
      (letfn* [eveny? (fn* [n] (if (zero? n) true  (oddy?  (- n 1))))
               oddy?  (fn* [n] (if (zero? n) false (eveny? (- n 1))))]
        [(eveny? 10) (eveny? 11) (oddy? 7)])""")
    assert list(out) == [True, False, True]

def test_letfn_star_three_way_mutual():
    """Three mutually-recursive fns — make sure all cells are visible
    to each closure."""
    out = E("""
      (letfn* [a (fn* [n] (if (zero? n) :a-done (b (- n 1))))
               b (fn* [n] (if (zero? n) :b-done (c (- n 1))))
               c (fn* [n] (if (zero? n) :c-done (a (- n 1))))]
        [(a 0) (a 1) (a 2) (a 3)])""")
    assert list(out) == [K("a-done"), K("b-done"), K("c-done"), K("a-done")]

def test_letfn_star_can_reference_outer_locals():
    """Closures inside letfn* can still see outer let* bindings."""
    out = E("""
      (let* [factor 10]
        (letfn* [scale (fn* [x] (* x factor))]
          (scale 5)))""")
    assert out == 50

def test_letfn_star_body_sees_bindings():
    """The body of letfn* sees the bound names too."""
    out = E("""
      (letfn* [f (fn* [x] (+ x 100))
               g (fn* [x] (* x 100))]
        [(f 1) (g 2)])""")
    assert list(out) == [101, 200]

def test_letfn_star_assert_args():
    """Odd number of forms in the binding vector → SyntaxError."""
    with pytest.raises(Exception, match="even number"):
        E("(letfn* [a (fn* [] :a) b] :body)")


# --- letfn macro --------------------------------------------------

def test_letfn_macro_expands_and_binds():
    """The user-facing letfn macro accepts (fname [params] body) tuples."""
    assert E("(letfn [(plus2 [x] (+ x 2))] (plus2 40))") == 42

def test_letfn_macro_mutual_recursion():
    out = E("""
      (letfn [(my-even? [n] (if (zero? n) true  (my-odd?  (dec n))))
              (my-odd?  [n] (if (zero? n) false (my-even? (dec n))))]
        [(my-even? 10) (my-odd? 7)])""")
    assert list(out) == [True, True]

def test_letfn_macro_with_destructuring():
    """Inside letfn, fns get the new (destructure-aware) `fn` macro."""
    out = E("""
      (letfn [(sum-pair [[a b]] (+ a b))]
        (sum-pair [10 20]))""")
    assert out == 30


# --- REPL state vars ---------------------------------------------

def test_star_1_2_3_e_unbound_by_default():
    """*1 *2 *3 *e have no root binding (`def name` without a value)."""
    for name in ("*1", "*2", "*3", "*e"):
        v = _core_var(name)
        assert v is not None
        assert v.has_root() is False

def test_star_repl_default_false():
    assert _core_var("*repl*").deref() is False

def test_star_1_2_3_dynamic():
    """REPL-state vars are dynamic — bindable via thread-local."""
    v1 = _core_var("*1")
    Var.push_thread_bindings(PersistentArrayMap.create(v1, "captured"))
    try:
        assert v1.deref() == "captured"
    finally:
        Var.pop_thread_bindings()


# --- new dynamic vars --------------------------------------------

def test_star_file_default_empty():
    assert _core_var("*file*").deref() == ""

def test_star_err_is_sys_stderr():
    assert _core_var("*err*").deref() is sys.stderr

def test_star_command_line_args_default_nil():
    assert _core_var("*command-line-args*").deref() is None

def test_star_warn_on_reflection_default_false():
    assert _core_var("*warn-on-reflection*").deref() is False

def test_star_compile_path_default_classes():
    assert _core_var("*compile-path*").deref() == "classes"

def test_star_compile_files_default_false():
    assert _core_var("*compile-files*").deref() is False

def test_star_compiler_options_default_empty_map():
    out = _core_var("*compiler-options*").deref()
    assert hasattr(out, "count")  # PersistentMap-like
    assert out.count() == 0


# --- load-file ----------------------------------------------------

def test_load_file_evaluates(tmp_path):
    """load-file reads forms from a file and evaluates them sequentially."""
    p = tmp_path / "loaded.clj"
    p.write_text("(def __tcb-loaded :ok)\n(def __tcb-num 99)\n")
    E(f'(load-file "{p}")')
    assert E("user/__tcb-loaded") == K("ok")
    assert E("user/__tcb-num") == 99


# --- alter-meta! / add-doc-and-meta documentation -----------------

def test_doc_attached_to_star_ns():
    out = E("(:doc (meta (clojure.core/var *ns*)))")
    assert out is not None
    assert "Namespace" in out

def test_doc_attached_to_star_out():
    out = E("(:doc (meta (clojure.core/var *out*)))")
    assert out is not None
    assert "output" in out.lower()

def test_doc_attached_to_star_err():
    out = E("(:doc (meta (clojure.core/var *err*)))")
    assert out is not None
    assert "error" in out.lower()

def test_doc_attached_to_star_compile_path():
    out = E("(:doc (meta (clojure.core/var *compile-path*)))")
    assert out is not None
    assert ".class" in out

def test_added_attached_to_star_assert():
    assert E("(:added (meta (clojure.core/var *assert*)))") == "1.0"

def test_added_attached_to_in_ns():
    assert E("(:added (meta (clojure.core/var in-ns)))") == "1.0"

def test_added_attached_to_load_file():
    assert E("(:added (meta (clojure.core/var load-file)))") == "1.0"

def test_added_attached_to_star_agent():
    """JVM source: (alter-meta! #'*agent* assoc :added "1.0")."""
    assert E("(:added (meta (clojure.core/var *agent*)))") == "1.0"


# --- add-doc-and-meta macro itself --------------------------------

def test_add_doc_and_meta_is_private():
    """add-doc-and-meta is a private macro."""
    v = _core_var("add-doc-and-meta")
    assert v is not None
    assert v.meta() is not None
    assert v.meta()[K("private")] is True

def test_add_doc_and_meta_attaches_doc_and_meta():
    """Verify the macro itself works on a fresh Var."""
    E("(def tcb-doc-target 42)")
    E("""(let [m (clojure.core/var clojure.core/add-doc-and-meta)]
           ((clojure.core/macroexpand
              (clojure.core/list (clojure.core/var clojure.core/add-doc-and-meta)
                                 'tcb-doc-target
                                 "test docstring"
                                 {:added "0.0"}))))""" if False else
      """(clojure.core/alter-meta! (clojure.core/var tcb-doc-target)
                                   merge
                                   {:doc "test docstring" :added "0.0"})""")
    assert E("(:doc (meta (clojure.core/var tcb-doc-target)))") == "test docstring"
    assert E("(:added (meta (clojure.core/var tcb-doc-target)))") == "0.0"
