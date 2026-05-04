"""Tests for core.clj batch 14 (lines 3473-3501): array / class / type.

Forms (4):
  into-array, array (private), class, type

Adds RT.seq_to_typed_array — counterpart to JVM's
clojure.lang.RT.seqToTypedArray. Python has no statically-typed
arrays, so the result is always a list; the type-arg form still
checks each element so `(into-array str [...])` rejects a non-string
just like JVM rejects a non-String.
"""

import pytest

import clojure.core  # triggers load

from clojure.lang import (
    Compiler,
    read_string,
    RT,
    Keyword, Symbol,
    PersistentVector,
    PersistentArrayMap,
    Var,
)


def E(src):
    return Compiler.eval(read_string(src))


def K(name):
    return Keyword.intern(None, name)


# --- RT.seq_to_typed_array (the runtime helper) -------------------

def test_rt_seq_to_typed_array_one_arg():
    assert RT.seq_to_typed_array([1, 2, 3]) == [1, 2, 3]

def test_rt_seq_to_typed_array_one_arg_nil():
    assert RT.seq_to_typed_array(None) == []

def test_rt_seq_to_typed_array_two_arg_passes_when_compatible():
    assert RT.seq_to_typed_array(int, [1, 2, 3]) == [1, 2, 3]

def test_rt_seq_to_typed_array_two_arg_allows_nil_elements():
    """Mirrors Java: null is assignable to any reference type."""
    assert RT.seq_to_typed_array(str, ["a", None, "b"]) == ["a", None, "b"]

def test_rt_seq_to_typed_array_two_arg_rejects_mismatch():
    with pytest.raises(TypeError, match="not an instance of"):
        RT.seq_to_typed_array(int, [1, "two"])

def test_rt_seq_to_typed_array_two_arg_nil_seq():
    assert RT.seq_to_typed_array(int, None) == []

def test_rt_seq_to_typed_array_arity_check():
    with pytest.raises(TypeError, match="1 or 2 args"):
        RT.seq_to_typed_array()
    with pytest.raises(TypeError, match="1 or 2 args"):
        RT.seq_to_typed_array(1, 2, 3)


# --- into-array ---------------------------------------------------

def test_into_array_basic():
    out = E("(clojure.core/into-array [1 2 3])")
    assert out == [1, 2, 3]
    assert isinstance(out, list)

def test_into_array_empty():
    assert E("(clojure.core/into-array [])") == []

def test_into_array_from_nil():
    """JVM: (into-array nil) yields an empty array (seq of nil is nil)."""
    assert E("(clojure.core/into-array nil)") == []

def test_into_array_from_seq():
    out = E("(clojure.core/into-array (clojure.core/range 5))")
    assert out == [0, 1, 2, 3, 4]

def test_into_array_with_type_str():
    out = E('(clojure.core/into-array String ["a" "b" "c"])')
    assert out == ["a", "b", "c"]

def test_into_array_with_type_int():
    out = E("(clojure.core/into-array Integer [1 2 3])")
    assert out == [1, 2, 3]

def test_into_array_with_type_rejects_mismatch():
    with pytest.raises(TypeError, match="not an instance of"):
        E('(clojure.core/into-array String ["a" 1])')

def test_into_array_with_type_allows_nil():
    out = E('(clojure.core/into-array String ["a" nil "b"])')
    assert out == ["a", None, "b"]


# --- array (private) ----------------------------------------------

def test_array_zero_args():
    """`array` takes &items — zero args is allowed and yields []."""
    assert E("(clojure.core/array)") == []

def test_array_basic():
    """array is a thin variadic wrapper around (into-array items)."""
    out = E('(clojure.core/array 1 :a "x")')
    assert out == [1, K("a"), "x"]


# --- class --------------------------------------------------------

def test_class_int():
    assert E("(clojure.core/class 3)") is int

def test_class_str():
    assert E('(clojure.core/class "x")') is str

def test_class_keyword():
    from clojure.lang import Keyword as KCls
    assert E("(clojure.core/class :a)") is KCls

def test_class_vector():
    assert E("(clojure.core/class [1 2 3])") is PersistentVector

def test_class_nil_is_nil():
    """JVM: `(class nil)` returns nil (the body short-circuits when x is nil)."""
    assert E("(clojure.core/class nil)") is None

def test_class_python_object():
    """Pass any Python object through the user namespace."""
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb14-x"), [1, 2, 3])
    assert E("(clojure.core/class user/tcb14-x)") is list


# --- type ---------------------------------------------------------

def test_type_falls_back_to_class():
    """No :type metadata → returns class."""
    assert E("(clojure.core/type 3)") is int

def test_type_returns_meta_type_keyword():
    out = E("(clojure.core/type (clojure.core/with-meta [1 2] {:type :my-tag}))")
    assert out == K("my-tag")

def test_type_vector_no_meta():
    """Vectors with no :type meta fall through to class."""
    assert E("(clojure.core/type [1 2 3])") is PersistentVector

def test_type_meta_other_than_type_key_ignored():
    """Only :type meta counts; other meta keys don't shadow class."""
    out = E("(clojure.core/type (clojure.core/with-meta [1 2] {:tag :other}))")
    assert out is PersistentVector

def test_type_nil_returns_nil():
    """(type nil) → (or (get (meta nil) :type) (class nil)) → nil."""
    assert E("(clojure.core/type nil)") is None
