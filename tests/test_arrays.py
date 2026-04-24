"""Java-array analogue tests.

The runtime representation is a plain Python `list`. Typed variants all
alias `aset`; `aset-char` and `aset-boolean` apply light coercion.
"""

import pytest
from clojure._core import eval_string


def _ev(s):
    return eval_string(s)


# --- make-array ---

def test_make_array_1d():
    assert _ev("(make-array 5)") == [None, None, None, None, None]


def test_make_array_1d_with_type():
    # Type arg is accepted for API compat but ignored.
    assert _ev("(make-array :anything 3)") == [None, None, None]


def test_make_array_2d():
    result = _ev("(make-array 2 3)")
    assert result == [[None, None, None], [None, None, None]]


def test_make_array_2d_with_type():
    assert _ev("(make-array :int 2 3)") == [[None, None, None], [None, None, None]]


def test_make_array_3d():
    expected = [[[None] * 2 for _ in range(2)] for _ in range(2)]
    assert _ev("(make-array 2 2 2)") == expected


def test_make_array_requires_dims():
    import pytest
    with pytest.raises(Exception):
        _ev("(make-array)")


# --- aget / aset ---

def test_aget_basic():
    assert _ev("(aget [10 20 30] 1)") == 20


def test_aset_returns_value():
    assert _ev("(let* [a (make-array 3)] (aset a 0 :x))") == _ev(":x")


def test_aset_mutates_in_place():
    assert _ev("(let* [a (make-array 3)] (aset a 0 :x) (aset a 1 :y) (aget a 0))") == _ev(":x")


def test_aget_multi_dim():
    src = "(let* [a (make-array 2 3)] (aset a 0 1 :x) (aget a 0 1))"
    assert _ev(src) == _ev(":x")


def test_aset_multi_dim():
    src = "(let* [a (make-array 2 2)] (aset a 0 0 :tl) (aset a 0 1 :tr) (aset a 1 0 :bl) (aset a 1 1 :br) a)"
    result = _ev(src)
    assert result == [[_ev(":tl"), _ev(":tr")], [_ev(":bl"), _ev(":br")]]


# --- alength / aclone ---

def test_alength_list():
    assert _ev("(alength [1 2 3 4 5])") == 5


def test_alength_empty():
    assert _ev("(alength (make-array 0))") == 0


def test_aclone_independent():
    # Mutating the clone doesn't affect the original.
    src = "(let* [a (make-array 3)] (aset a 0 :orig) (let* [b (aclone a)] (aset b 0 :changed) [(aget a 0) (aget b 0)]))"
    result = _ev(src)
    assert result[0] == _ev(":orig")
    assert result[1] == _ev(":changed")


# --- to-array / into-array / to-array-2d ---

def test_to_array_from_vec():
    assert _ev("(to-array [1 2 3 4])") == [1, 2, 3, 4]


def test_to_array_from_list():
    assert _ev("(to-array '(1 2 3))") == [1, 2, 3]


def test_to_array_from_seq():
    assert _ev("(to-array (range 5))") == [0, 1, 2, 3, 4]


def test_into_array_one_arg():
    assert _ev("(into-array [1 2 3])") == [1, 2, 3]


def test_into_array_with_type():
    assert _ev("(into-array :anything [1 2 3])") == [1, 2, 3]


def test_to_array_2d_nested():
    assert _ev("(to-array-2d [[1 2] [3 4] [5 6]])") == [[1, 2], [3, 4], [5, 6]]


# --- Typed variants ---

def test_aset_int():
    assert _ev("(let* [a (make-array 1)] (aset-int a 0 42) (aget a 0))") == 42


def test_aset_long():
    assert _ev("(let* [a (make-array 1)] (aset-long a 0 9999999999) (aget a 0))") == 9999999999


def test_aset_float():
    result = _ev("(let* [a (make-array 1)] (aset-float a 0 3.14) (aget a 0))")
    assert abs(result - 3.14) < 1e-6


def test_aset_double():
    result = _ev("(let* [a (make-array 1)] (aset-double a 0 3.14) (aget a 0))")
    assert abs(result - 3.14) < 1e-6


def test_aset_boolean():
    assert _ev("(let* [a (make-array 1)] (aset-boolean a 0 true) (aget a 0))") is True


# --- amap / areduce macros ---

def test_amap_basic():
    assert _ev("(amap [1 2 3 4 5] i ret (* 10 (aget ret i)))") == [10, 20, 30, 40, 50]


def test_amap_identity():
    assert _ev("(amap [7 8 9] i ret (aget ret i))") == [7, 8, 9]


def test_areduce_sum():
    assert _ev("(let* [a [1 2 3 4 5]] (areduce a i sum 0 (+ sum (aget a i))))") == 15


def test_areduce_max():
    assert _ev("(let* [a [3 7 1 5 2]] (areduce a i m 0 (if (> (aget a i) m) (aget a i) m)))") == 7


# --- Interop with Clojure collections ---

def test_alength_of_vector():
    # A PersistentVector should also work.
    assert _ev("(alength (vec (range 7)))") == 7


def test_aget_on_python_list():
    # Python list (the runtime representation of an "array") supports aget.
    assert _ev("(aget (to-array '(10 20 30)) 2)") == 30
