"""Tests for core.clj batch 20 (lines 3924-4046): Java array operations.

Forms (12):
  alength, aclone, aget, aset,
  aset-int, aset-long, aset-boolean, aset-float, aset-double,
  aset-short, aset-byte, aset-char,
  make-array, to-array-2d.

Backend additions:
  clojure.lang.Array — counterpart to java.lang.reflect.Array. Backs
                       all the array ops. Numeric primitive arrays
                       use Python's array.array for fixed-width
                       homogeneous storage; Object[] equivalent uses
                       a list. Type code map:
                         int   → 'q' (signed 64-bit)
                         float → 'd' (double)
                         else  → list
  RT.alength / RT.aclone / RT.aget / RT.aset — runtime helpers
                       backing the inline forms.

Compiler bug fix worth calling out:
  syntax-quote was auto-qualifying `&` (rest-args separator) to
  clojure.core/&, which broke fn* arg parsing on any macro that
  emitted `[args & rest]` via syntax-quote. def-aset is the canonical
  case. JVM Clojure leaves & alone inside syntax-quote; we now do too.
"""

import array as _array

import pytest

import clojure.core  # triggers load

from clojure.lang import (
    Compiler,
    read_string,
    Var, Symbol, Keyword,
    RT, Array,
)


def E(src):
    return Compiler.eval(read_string(src))


def K(name):
    return Keyword.intern(None, name)


# --- Array shim ---------------------------------------------------

def test_array_new_instance_int_returns_typed_array():
    arr = Array.newInstance(int, 5)
    assert isinstance(arr, _array.array)
    assert arr.typecode == "q"
    assert list(arr) == [0, 0, 0, 0, 0]

def test_array_new_instance_float_returns_typed_array():
    arr = Array.newInstance(float, 3)
    assert isinstance(arr, _array.array)
    assert arr.typecode == "d"

def test_array_new_instance_object_returns_list():
    arr = Array.newInstance(object, 4)
    assert isinstance(arr, list)
    assert arr == [None, None, None, None]

def test_array_new_instance_multidim():
    arr = Array.newInstance(int, [2, 3])
    assert len(arr) == 2
    assert all(isinstance(r, _array.array) for r in arr)
    assert all(list(r) == [0, 0, 0] for r in arr)

def test_array_set_int_rejects_non_int_on_typed_array():
    arr = Array.newInstance(int, 3)
    with pytest.raises((TypeError, ValueError)):
        Array.setInt(arr, 0, "x")

def test_array_set_int_coerces_float_to_int():
    arr = Array.newInstance(int, 3)
    Array.setInt(arr, 0, 3.7)
    assert arr[0] == 3

def test_array_set_char_from_codepoint():
    arr = [None, None, None]
    Array.setChar(arr, 0, 65)
    assert arr[0] == "A"

def test_array_set_char_from_str():
    arr = [None, None]
    Array.setChar(arr, 0, "Z")
    assert arr[0] == "Z"

def test_array_get_length_uniform_across_types():
    assert Array.getLength(Array.newInstance(int, 5)) == 5
    assert Array.getLength(Array.newInstance(object, 7)) == 7


# --- alength ------------------------------------------------------

def test_alength_int_array():
    assert E("(clojure.core/alength (clojure.core/make-array Integer 5))") == 5

def test_alength_object_array():
    assert E("(clojure.core/alength (clojure.core/make-array Object 3))") == 3

def test_alength_multidim():
    """alength returns the outer dimension."""
    assert E("(clojure.core/alength (clojure.core/make-array Integer 4 7))") == 4


# --- aclone -------------------------------------------------------

def test_aclone_typed_int_array_is_independent():
    src = Array.newInstance(int, 3)
    Array.setInt(src, 0, 42)
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb20-src"), src)
    clone = E("(clojure.core/aclone user/tcb20-src)")
    assert isinstance(clone, _array.array)
    assert list(clone) == [42, 0, 0]
    # Mutating the clone doesn't affect the original.
    clone[0] = 99
    assert src[0] == 42

def test_aclone_object_array_is_independent():
    src = Array.newInstance(object, 3)
    src[0] = "x"
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb20-osrc"), src)
    clone = E("(clojure.core/aclone user/tcb20-osrc)")
    assert isinstance(clone, list)
    assert clone == ["x", None, None]
    clone[0] = "y"
    assert src[0] == "x"


# --- aget ---------------------------------------------------------

def test_aget_int_array():
    arr = Array.newInstance(int, 3)
    Array.setInt(arr, 1, 99)
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb20-aget"), arr)
    assert E("(clojure.core/aget user/tcb20-aget 1)") == 99

def test_aget_object_array():
    arr = Array.newInstance(object, 3)
    arr[2] = K("end")
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb20-oaget"), arr)
    assert E("(clojure.core/aget user/tcb20-oaget 2)") == K("end")

def test_aget_multi_dim_indices():
    arr = Array.newInstance(int, [3, 3])
    Array.setInt(arr[1], 2, 42)
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb20-mget"), arr)
    assert E("(clojure.core/aget user/tcb20-mget 1 2)") == 42


# --- aset ---------------------------------------------------------

def test_aset_returns_value():
    arr = Array.newInstance(object, 3)
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb20-rset"), arr)
    out = E("(clojure.core/aset user/tcb20-rset 0 :hello)")
    assert out == K("hello")

def test_aset_object_array():
    arr = Array.newInstance(object, 2)
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb20-oset"), arr)
    E("(clojure.core/aset user/tcb20-oset 0 [1 2 3])")
    E('(clojure.core/aset user/tcb20-oset 1 "x")')
    assert list(arr[0]) == [1, 2, 3]
    assert arr[1] == "x"

def test_aset_multi_dim():
    arr = Array.newInstance(int, [2, 2])
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb20-mset"), arr)
    E("(clojure.core/aset user/tcb20-mset 0 1 42)")
    assert arr[0][1] == 42


# --- aset-* (typed) -----------------------------------------------

def _typed_setter_test(setter, type_class, valid_val, invalid_val):
    arr = Array.newInstance(type_class, 3)
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb20-typed"), arr)
    E(f"(clojure.core/{setter} user/tcb20-typed 0 {valid_val})")
    if invalid_val is not None:
        with pytest.raises((TypeError, ValueError)):
            E(f'(clojure.core/{setter} user/tcb20-typed 1 {invalid_val})')

def test_aset_int():
    _typed_setter_test("aset-int", int, 42, '"x"')

def test_aset_long():
    _typed_setter_test("aset-long", int, 9999999999, '"x"')

def test_aset_short():
    _typed_setter_test("aset-short", int, 100, '"x"')

def test_aset_byte():
    _typed_setter_test("aset-byte", int, 5, '"x"')

def test_aset_float():
    _typed_setter_test("aset-float", float, 3.14, None)

def test_aset_double():
    _typed_setter_test("aset-double", float, 2.71, None)

def test_aset_boolean():
    """boolean array uses a list since Python has no bool typecode."""
    arr = Array.newInstance(object, 3)
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb20-bool"), arr)
    E("(clojure.core/aset-boolean user/tcb20-bool 0 true)")
    E("(clojure.core/aset-boolean user/tcb20-bool 1 false)")
    E("(clojure.core/aset-boolean user/tcb20-bool 2 :truthy)")
    assert arr == [True, False, True]

def test_aset_char():
    arr = Array.newInstance(object, 2)
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb20-char"), arr)
    E('(clojure.core/aset-char user/tcb20-char 0 65)')
    E('(clojure.core/aset-char user/tcb20-char 1 "Z")')
    assert arr == ["A", "Z"]


# --- make-array ---------------------------------------------------

def test_make_array_int_initializes_zeros():
    arr = E("(clojure.core/make-array Integer 5)")
    assert list(arr) == [0, 0, 0, 0, 0]

def test_make_array_float_initializes_zeros():
    arr = E("(clojure.core/make-array Float 3)")
    assert list(arr) == [0.0, 0.0, 0.0]

def test_make_array_object_initializes_nones():
    arr = E("(clojure.core/make-array Object 4)")
    assert arr == [None, None, None, None]

def test_make_array_two_dim():
    arr = E("(clojure.core/make-array Integer 2 3)")
    assert len(arr) == 2
    assert [list(r) for r in arr] == [[0, 0, 0], [0, 0, 0]]

def test_make_array_three_dim():
    arr = E("(clojure.core/make-array Integer 2 2 2)")
    assert len(arr) == 2
    assert all(len(r) == 2 for r in arr)
    assert all(list(c) == [0, 0] for r in arr for c in r)


# --- to-array-2d --------------------------------------------------

def test_to_array_2d_basic():
    out = E("(clojure.core/to-array-2d (clojure.core/list [1 2] [3 4 5]))")
    assert isinstance(out, list)
    assert out[0] == [1, 2]
    assert out[1] == [3, 4, 5]

def test_to_array_2d_ragged():
    """JVM doc says 'potentially-ragged' — sub-arrays can have different lengths."""
    out = E("(clojure.core/to-array-2d (clojure.core/list [1] [2 3] [4 5 6]))")
    assert out[0] == [1]
    assert out[1] == [2, 3]
    assert out[2] == [4, 5, 6]

def test_to_array_2d_empty():
    out = E("(clojure.core/to-array-2d (clojure.core/list))")
    assert out == []


# --- the syntax-quote `&` fix -------------------------------------

def test_syntax_quote_leaves_amp_unqualified():
    """Regression: this is the bug that blocked def-aset's loading."""
    expanded = E('(clojure.core/macroexpand-1 (quote `[a b & c]))') if False else None
    # Direct check: build a syntax-quote form and inspect the result.
    out = E("`[a b & c]")
    # The `&` should be a bare unqualified Symbol.
    parts = list(out)
    amp = parts[2]  # third element
    assert isinstance(amp, Symbol)
    assert amp.ns is None
    assert amp.name == "&"

def test_def_aset_macro_loaded():
    """def-aset itself should be defined; without the & fix, core.clj
    fails to load past the first (def-aset ...) form."""
    from clojure.lang import Namespace
    ns = Namespace.find(Symbol.intern("clojure.core"))
    assert ns.find_interned_var(Symbol.intern("def-aset")) is not None

def test_aset_int_macro_loaded():
    """The first def-aset call — verifying the macro expansion succeeded."""
    from clojure.lang import Namespace
    ns = Namespace.find(Symbol.intern("clojure.core"))
    assert ns.find_interned_var(Symbol.intern("aset-int")) is not None
