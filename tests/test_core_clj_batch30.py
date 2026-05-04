"""Tests for core.clj batch 30 (selected from JVM 5400-5618):
class introspection + Var helpers + hierarchy stubs.

Forms (9):
  class?,
  alter-var-root, bound?, thread-bound?,
  make-hierarchy, global-hierarchy (private def),
  not-empty,
  bases, supers.

Skipped — saved for follow-up batches:
  booleans / bytes / chars / shorts / floats / ints / doubles /
  longs (8 definlines)        — Object[] → typed-array casts; need
                                 Numbers.X cast methods + the
                                 definline expansion-time eval.
  bytes?                       — JVM checks (= Byte/TYPE
                                 (.getComponentType (class x))).
                                 Python has no primitive Byte type;
                                 add when there's a real use.
  seque                        — needs java.util.concurrent
                                 BlockingQueue + LinkedBlockingQueue
                                 shims and careful agent + send-off
                                 coordination.
  is-annotation? /
  is-runtime-annotation? /
  descriptor /
  add-annotation /
  process-annotation /
  add-annotations              — JVM-only (clojure.asm + java.lang
                                 .annotation).

Adaptations from JVM source (all snake_case interop method renames):
  alter-var-root  uses .alter_root         (JVM: .alterRoot).
  bound?          uses .is_bound           (JVM: .isBound).
  thread-bound?   uses .get_thread_binding (JVM: .getThreadBinding).
  bases           uses Python's class.__bases__ instead of JVM's
                  .getInterfaces / .getSuperclass — Python's MRO
                  unifies superclasses and interfaces. We drop
                  `object` from the result to match JVM's "no
                  superclass for Object" elision.
"""

import pytest

import clojure.core  # bootstrap

from clojure.lang import (
    Compiler,
    read_string,
    Var, Symbol, Keyword,
    PersistentArrayMap,
)


def E(src):
    return Compiler.eval(read_string(src))


def K(name):
    return Keyword.intern(None, name)


# --- class? ------------------------------------------------------

def test_class_pred_true_for_python_classes():
    assert E("(class? Integer)") is True
    assert E("(class? String)") is True
    assert E("(class? Object)") is True

def test_class_pred_false_for_instances():
    assert E("(class? 5)") is False
    assert E("(class? :keyword)") is False
    assert E("(class? [1 2 3])") is False
    assert E("(class? nil)") is False

def test_class_pred_true_for_user_class():
    """User-defined Python classes count."""
    class Foo:
        pass
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb30-Foo"), Foo)
    assert E("(class? user/tcb30-Foo)") is True


# --- alter-var-root ----------------------------------------------

def test_alter_var_root_one_fn():
    E("(def tcb30-avr-x 10)")
    E("(alter-var-root (var tcb30-avr-x) (fn [n] (* n 5)))")
    assert E("tcb30-avr-x") == 50

def test_alter_var_root_with_args():
    E("(def tcb30-avr-y 0)")
    E("(alter-var-root (var tcb30-avr-y) + 100 7)")
    assert E("tcb30-avr-y") == 107

def test_alter_var_root_returns_new_value():
    """JVM: alter-var-root returns the new root value."""
    E("(def tcb30-avr-r 1)")
    out = E("(alter-var-root (var tcb30-avr-r) inc)")
    assert out == 2


# --- bound? ------------------------------------------------------

def test_bound_pred_true_for_bound_var():
    E("(def tcb30-bound 42)")
    assert E("(bound? (var tcb30-bound))") is True

def test_bound_pred_false_for_unbound_var():
    """def without a value creates an unbound var."""
    E("(def tcb30-unbound)")
    assert E("(bound? (var tcb30-unbound))") is False

def test_bound_pred_true_with_no_args():
    """Vacuously true."""
    assert E("(bound?)") is True

def test_bound_pred_false_if_any_arg_unbound():
    E("(def tcb30-mix-bound 1)")
    E("(def tcb30-mix-unbound)")
    out = E("(bound? (var tcb30-mix-bound) (var tcb30-mix-unbound))")
    assert out is False


# --- thread-bound? -----------------------------------------------

def test_thread_bound_false_outside_binding():
    E("(def ^:dynamic tcb30-tb-x 0)")
    assert E("(thread-bound? (var tcb30-tb-x))") is False

def test_thread_bound_true_inside_binding():
    E("(def ^:dynamic tcb30-tb-y 0)")
    out = E("(binding [tcb30-tb-y 99] (thread-bound? (var tcb30-tb-y)))")
    assert out is True

def test_thread_bound_true_with_no_args():
    assert E("(thread-bound?)") is True


# --- make-hierarchy ----------------------------------------------

def test_make_hierarchy_shape():
    h = E("(make-hierarchy)")
    assert dict(h) == {
        K("parents"): PersistentArrayMap.create(),
        K("descendants"): PersistentArrayMap.create(),
        K("ancestors"): PersistentArrayMap.create(),
    }

def test_make_hierarchy_returns_fresh_map():
    """Each call returns a new (but equal) map — basic sanity."""
    h1 = E("(make-hierarchy)")
    h2 = E("(make-hierarchy)")
    assert dict(h1) == dict(h2)


# --- global-hierarchy --------------------------------------------

def test_global_hierarchy_initialized():
    """global-hierarchy is a private Var initialized to make-hierarchy's
    output. Reach for it via the Var since it's :private."""
    from clojure.lang import Namespace
    v = Namespace.find(Symbol.intern("clojure.core")).find_interned_var(
        Symbol.intern("global-hierarchy"))
    out = v.deref()
    assert dict(out) == {
        K("parents"): PersistentArrayMap.create(),
        K("descendants"): PersistentArrayMap.create(),
        K("ancestors"): PersistentArrayMap.create(),
    }


# --- not-empty ---------------------------------------------------

def test_not_empty_nonempty_passthrough():
    assert E("(not-empty [1 2 3])") == E("[1 2 3]")
    assert E("(not-empty {:a 1})") == E("{:a 1}")
    assert E("(not-empty (list 1))") == E("(list 1)")

def test_not_empty_empty_returns_nil():
    assert E("(not-empty [])") is None
    assert E("(not-empty {})") is None
    assert E("(not-empty (list))") is None
    assert E("(not-empty #{})") is None

def test_not_empty_nil_returns_nil():
    assert E("(not-empty nil)") is None

def test_not_empty_string():
    """Strings work: empty → nil, non-empty → the string."""
    assert E('(not-empty "")') is None
    assert E('(not-empty "hello")') == "hello"


# --- bases -------------------------------------------------------

def test_bases_for_simple_class():
    """User Python class with one parent."""
    class Animal:
        pass
    class Dog(Animal):
        pass
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb30-Animal"), Animal)
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb30-Dog"), Dog)
    out = list(E("(bases user/tcb30-Dog)"))
    assert out == [Animal]

def test_bases_for_multi_inheritance():
    class A: pass
    class B: pass
    class AB(A, B): pass
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb30-A"), A)
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb30-B"), B)
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb30-AB"), AB)
    out = list(E("(bases user/tcb30-AB)"))
    assert set(out) == {A, B}

def test_bases_drops_object_to_match_jvm():
    """Python's __bases__ for direct child of object includes object;
    we elide it to match JVM's 'no superclass for Object' behavior."""
    class Direct:
        pass
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb30-Direct"), Direct)
    # Direct's only base is `object` — should be filtered out → nil.
    out = E("(bases user/tcb30-Direct)")
    assert out is None  # seq returns nil for empty

def test_bases_nil_class_returns_nil():
    assert E("(bases nil)") is None


# --- supers ------------------------------------------------------

def test_supers_walks_the_chain():
    class A: pass
    class B(A): pass
    class C(B): pass
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb30-sup-A"), A)
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb30-sup-B"), B)
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb30-sup-C"), C)
    out = E("(supers user/tcb30-sup-C)")
    assert set(out) == {A, B}

def test_supers_diamond_inheritance():
    class A: pass
    class B(A): pass
    class C(A): pass
    class D(B, C): pass
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb30-A"), A)
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb30-B"), B)
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb30-C"), C)
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb30-D"), D)
    out = E("(supers user/tcb30-D)")
    assert set(out) == {A, B, C}

def test_supers_root_class_returns_nil():
    """A class whose only base is object has no non-object supers."""
    class Direct:
        pass
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb30-Direct"), Direct)
    out = E("(supers user/tcb30-Direct)")
    assert out is None
