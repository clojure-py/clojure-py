"""Tests for core.clj batch 29 (JVM 5306-5400): typed-array constructors
+ amap / areduce.

Forms (11):
  amap, areduce (macros),
  float-array, boolean-array, byte-array, char-array, short-array,
  double-array, object-array, int-array, long-array.

Backend additions:
  Numbers.{int,long,short,byte,float,double,boolean,char}_array
    8 typed array factories. Each handles three call shapes:
      (size_or_seq):
        int N → array of N zeros (or appropriate default)
        seq    → array sized to seq, elements coerced
      (size, init_val):
        int + Number/str/bool → array of size, all elements = init_val
      (size, seq):
        int + seq → array of size, prefix from seq, rest = default

  RT.object_array
    Single-arity factory for Object[] equivalent. JVM puts this on
    RT (not Numbers) since object isn't a numeric primitive type;
    we follow.

Storage:
  int / long / short → array.array typecode 'q' or 'h' (signed
                       64-bit / 16-bit). Python int has no fixed
                       width; long_array uses the widest typecode.
  float / double      → typecode 'f' / 'd'.
  byte                → 'b' (signed 8-bit).
  boolean / char      → list (Python's array module has no native
                        bool typecode; 'u' for unicode is deprecated
                        in Python 3.16+).
  object              → list.
"""

import array as _array

import pytest

import clojure.core  # bootstrap

from clojure.lang import (
    Compiler,
    read_string,
    Var, Symbol, Keyword,
)


def E(src):
    return Compiler.eval(read_string(src))


def K(name):
    return Keyword.intern(None, name)


# --- int-array ----------------------------------------------------

def test_int_array_size():
    out = E("(int-array 5)")
    assert isinstance(out, _array.array)
    assert out.typecode == "q"
    assert list(out) == [0, 0, 0, 0, 0]

def test_int_array_seq():
    out = E("(int-array [10 20 30])")
    assert list(out) == [10, 20, 30]

def test_int_array_size_init():
    out = E("(int-array 5 7)")
    assert list(out) == [7, 7, 7, 7, 7]

def test_int_array_size_seq_prefix():
    """Size N + seq M (M < N) → first M from seq, rest 0."""
    out = E("(int-array 5 [1 2 3])")
    assert list(out) == [1, 2, 3, 0, 0]

def test_int_array_rejects_non_int_element():
    """Typed array.array assignment validates element type."""
    with pytest.raises((TypeError, ValueError)):
        E('(int-array ["a" "b"])')


# --- long-array (collapses to int_array in Python) ---------------

def test_long_array_basic():
    out = E("(long-array 3)")
    assert list(out) == [0, 0, 0]

def test_long_array_seq():
    out = E("(long-array [100 200 300])")
    assert list(out) == [100, 200, 300]

def test_long_array_arbitrary_precision():
    """Python ints are arbitrary precision — 2^60 fits without
    overflow concerns."""
    out = E("(long-array [1152921504606846976])")  # 2^60
    assert out[0] == 1152921504606846976


# --- short-array / byte-array ------------------------------------

def test_short_array_seq():
    out = E("(short-array [1 2 3])")
    assert isinstance(out, _array.array)
    assert out.typecode == "h"
    assert list(out) == [1, 2, 3]

def test_short_array_overflow_rejected():
    """Short is 16-bit — large values raise."""
    with pytest.raises((OverflowError, ValueError, TypeError)):
        E("(short-array [99999])")

def test_byte_array_seq():
    out = E("(byte-array [1 2 127])")
    assert out.typecode == "b"
    assert list(out) == [1, 2, 127]


# --- float-array / double-array ----------------------------------

def test_float_array_size():
    out = E("(float-array 3)")
    assert out.typecode == "f"
    assert list(out) == [0.0, 0.0, 0.0]

def test_float_array_seq():
    out = E("(float-array [1.5 2.5 3.5])")
    assert list(out) == [1.5, 2.5, 3.5]

def test_double_array_seq():
    out = E("(double-array [1.5 2.5])")
    assert out.typecode == "d"
    assert list(out) == [1.5, 2.5]

def test_double_array_size_init():
    out = E("(double-array 4 0.25)")
    assert list(out) == [0.25, 0.25, 0.25, 0.25]


# --- boolean-array ------------------------------------------------

def test_boolean_array_size():
    """Boolean uses a list — no native typecode."""
    out = E("(boolean-array 3)")
    assert isinstance(out, list)
    assert out == [False, False, False]

def test_boolean_array_seq():
    out = E("(boolean-array [true false true])")
    assert out == [True, False, True]

def test_boolean_array_size_init():
    out = E("(boolean-array 3 true)")
    assert out == [True, True, True]

def test_boolean_array_truthiness():
    """Non-bool truthy values get coerced via bool()."""
    out = E("(boolean-array [:keyword nil 1 0])")
    assert out == [True, False, True, False]


# --- char-array ---------------------------------------------------

def test_char_array_from_codepoints():
    out = E("(char-array [65 66 67])")
    assert out == ["A", "B", "C"]

def test_char_array_from_str():
    """Single-char strings pass through."""
    out = E('(char-array ["a" "b"])')
    assert out == ["a", "b"]

def test_char_array_size_init_str():
    out = E('(char-array 3 "X")')
    assert out == ["X", "X", "X"]

def test_char_array_size_init_int():
    out = E("(char-array 3 65)")
    assert out == ["A", "A", "A"]

def test_char_array_size_only():
    """Default char is null-terminator-style \x00."""
    out = E("(char-array 2)")
    assert out == ["\x00", "\x00"]


# --- object-array ------------------------------------------------

def test_object_array_size():
    out = E("(object-array 3)")
    assert isinstance(out, list)
    assert out == [None, None, None]

def test_object_array_seq():
    out = E("(object-array [:a 1 \"x\"])")
    assert out == [K("a"), 1, "x"]

def test_object_array_heterogeneous():
    """Object[] holds anything — including Clojure data structures."""
    out = E('(object-array [[1 2] {:a 1} :kw "str" 42])')
    assert len(out) == 5


# --- amap ---------------------------------------------------------

def test_amap_doubles():
    """Map over int array, multiplying each element by 10."""
    out = E("""
      (let [src (int-array [1 2 3 4])]
        (amap src i ret (* (aget src i) 10)))""")
    assert list(out) == [10, 20, 30, 40]

def test_amap_clones_first():
    """amap clones the source so the original is unchanged."""
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb29-src"),
               E("(int-array [1 2 3])"))
    E("(amap user/tcb29-src i ret (+ (aget user/tcb29-src i) 100))")
    # Source unchanged.
    src = E("user/tcb29-src")
    assert list(src) == [1, 2, 3]


# --- areduce -----------------------------------------------------

def test_areduce_sum():
    out = E("""
      (let [src (int-array [1 2 3 4 5])]
        (areduce src i sum 0 (+ sum (aget src i))))""")
    assert out == 15

def test_areduce_product():
    out = E("""
      (let [src (int-array [1 2 3 4])]
        (areduce src i p 1 (* p (aget src i))))""")
    assert out == 24

def test_areduce_with_init():
    out = E("""
      (let [src (double-array [1.5 2.5 3.5])]
        (areduce src i acc 100.0 (+ acc (aget src i))))""")
    assert out == 107.5

def test_areduce_empty_returns_init():
    out = E("""
      (let [src (int-array 0)]
        (areduce src i acc :init (str acc i)))""")
    assert out == K("init")
