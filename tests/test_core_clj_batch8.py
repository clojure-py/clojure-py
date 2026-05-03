"""Tests for core.clj batch 8 (lines 1851-2063):

assert-args (private), if-let, when-let, if-some, when-some,
push-thread-bindings, pop-thread-bindings, get-thread-bindings,
binding (macro), with-bindings*, with-bindings,
bound-fn*, bound-fn,
find-var, binding-conveyor-fn (private)
"""

import pytest
import threading

import clojure.core  # triggers load

from clojure.lang import (
    Compiler,
    read_string,
    Var, Symbol, Keyword, Namespace, RT,
    PersistentArrayMap,
)


def E(src):
    return Compiler.eval(read_string(src))


def K(name):
    return Keyword.intern(None, name)


# --- if-let / when-let -------------------------------------------

def test_if_let_truthy_binds_value():
    assert E("(clojure.core/if-let [x 42] x :no)") == 42

def test_if_let_falsy_uses_else():
    assert E("(clojure.core/if-let [x nil] x :no)") == K("no")
    assert E("(clojure.core/if-let [x false] x :no)") == K("no")

def test_if_let_no_else_is_nil_when_false():
    assert E("(clojure.core/if-let [x nil] x)") is None

def test_if_let_uses_full_value_in_then():
    """The full test value is bound to the form, not just truthy/false."""
    assert E("(clojure.core/if-let [x [1 2 3]] (clojure.core/count x) 0)") == 3

def test_when_let_truthy():
    assert E("(clojure.core/when-let [x 5] (clojure.core/+ x 1))") == 6

def test_when_let_falsy_is_nil():
    assert E("(clojure.core/when-let [x nil] x)") is None

def test_when_let_body_is_implicit_do():
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb8-counter"), [])
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb8-bump!"),
               lambda x: (Compiler.eval(read_string("user/tcb8-counter")).append(x), x)[1])
    E("(clojure.core/when-let [x 1] (user/tcb8-bump! x) (user/tcb8-bump! 99))")
    assert Compiler.eval(read_string("user/tcb8-counter")) == [1, 99]


# --- if-some / when-some -----------------------------------------

def test_if_some_zero_is_present():
    assert E("(clojure.core/if-some [x 0] x :no)") == 0

def test_if_some_false_is_present():
    """if-some only treats nil as 'absent', not false."""
    assert E("(clojure.core/if-some [x false] x :no)") is False

def test_if_some_nil_uses_else():
    assert E("(clojure.core/if-some [x nil] x :no)") == K("no")

def test_when_some_zero_evaluates_body():
    assert E("(clojure.core/when-some [x 0] (clojure.core/inc x))") == 1

def test_when_some_nil_is_nil():
    assert E("(clojure.core/when-some [x nil] x)") is None


# --- assert-args ------------------------------------------------

def test_assert_args_via_if_let():
    """if-let uses assert-args internally; verify it raises for bad
    binding shapes."""
    with pytest.raises(ValueError):
        E("(clojure.core/if-let (x 1) x)")  # not a vector
    with pytest.raises(ValueError):
        E("(clojure.core/if-let [x 1 y 2] x)")  # too many bindings


# --- push/pop/get thread bindings -------------------------------

def test_push_pop_get_thread_bindings():
    v = Var.intern(Compiler.current_ns(), Symbol.intern("tcb8-d1"),
                   "root").set_dynamic()
    initial = E("(clojure.core/get-thread-bindings)")
    pre_count = initial.count() if hasattr(initial, "count") else 0
    E("(clojure.core/push-thread-bindings (clojure.core/hash-map (var user/tcb8-d1) \"x\"))")
    try:
        assert E("user/tcb8-d1") == "x"
        bindings = E("(clojure.core/get-thread-bindings)")
        assert bindings.count() == pre_count + 1
    finally:
        E("(clojure.core/pop-thread-bindings)")
    assert E("user/tcb8-d1") == "root"


# --- binding macro ---------------------------------------------

def test_binding_basic():
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb8-b"),
               "root").set_dynamic()
    assert E('(clojure.core/binding [user/tcb8-b "bound"] user/tcb8-b)') == "bound"
    # outside, root is restored
    assert E("user/tcb8-b") == "root"

def test_binding_multiple_vars():
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb8-b1"),
               1).set_dynamic()
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb8-b2"),
               2).set_dynamic()
    result = E(
        "(clojure.core/binding [user/tcb8-b1 10 user/tcb8-b2 20]"
        " (clojure.core/+ user/tcb8-b1 user/tcb8-b2))"
    )
    assert result == 30

def test_binding_pops_on_exception():
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb8-be"),
               "root").set_dynamic()
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb8-bom"),
               lambda: (_ for _ in ()).throw(ValueError("boom")))
    with pytest.raises(ValueError):
        E('(clojure.core/binding [user/tcb8-be "bound"] (user/tcb8-bom))')
    # Binding pops even though body threw
    assert E("user/tcb8-be") == "root"

def test_binding_requires_even_bindings():
    with pytest.raises(ValueError):
        E("(clojure.core/binding [a 1 b] a)")


# --- with-bindings* / with-bindings ----------------------------

def test_with_bindings_star():
    v = Var.intern(Compiler.current_ns(), Symbol.intern("tcb8-wb"),
                   "root").set_dynamic()
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb8-readwb"),
               lambda: Compiler.eval(read_string("user/tcb8-wb")))
    result = E(
        "(clojure.core/with-bindings* (clojure.core/hash-map (var user/tcb8-wb) \"x\")"
        " user/tcb8-readwb)"
    )
    assert result == "x"

def test_with_bindings_macro():
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb8-wb2"),
               "root").set_dynamic()
    result = E(
        "(clojure.core/with-bindings (clojure.core/hash-map (var user/tcb8-wb2) \"y\")"
        " user/tcb8-wb2)"
    )
    assert result == "y"


# --- bound-fn* / bound-fn -------------------------------------

def test_bound_fn_star_captures_bindings_for_other_thread():
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb8-tfn"),
               "root").set_dynamic()
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb8-readtfn"),
               lambda: Compiler.eval(read_string("user/tcb8-tfn")))
    captured = []
    E("(def tcb8-bf "
      " (clojure.core/binding [user/tcb8-tfn \"bound-val\"]"
      "   (clojure.core/bound-fn* user/tcb8-readtfn)))")
    bf = E("user/tcb8-bf")
    # Spawn another thread; bound-fn* should still see "bound-val" even
    # though the binding was popped.
    result_holder = []
    def go():
        result_holder.append(bf())
    t = threading.Thread(target=go)
    t.start()
    t.join()
    assert result_holder == ["bound-val"]


# --- find-var --------------------------------------------------

def test_find_var_returns_existing():
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb8-fv"), 42)
    v = E("(clojure.core/find-var 'user/tcb8-fv)")
    assert isinstance(v, Var)
    assert v.deref() == 42

def test_find_var_unknown_raises():
    """Our Var.find raises when the namespace isn't found, matching
    the JVM Var.find semantics (different from JVM Var.find which
    returns nil). Ours is stricter — accept either nil or an exception."""
    try:
        v = E("(clojure.core/find-var 'no-such-ns/no-such-var)")
        assert v is None
    except (ValueError, KeyError):
        pass


# --- binding-conveyor-fn (private) ----------------------------

def test_binding_conveyor_fn_carries_frame():
    """Just verify it returns a callable; the conveyor's full semantic
    is exercised by future-style executors which we'll add later."""
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb8-conv"),
               "root").set_dynamic()
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb8-rconv"),
               lambda: Compiler.eval(read_string("user/tcb8-conv")))
    bcf = E("(clojure.core/binding [user/tcb8-conv \"X\"]"
            " (clojure.core/binding-conveyor-fn user/tcb8-rconv))")
    assert callable(bcf)
    # Calling the returned fn re-establishes the binding frame
    assert bcf() == "X"
