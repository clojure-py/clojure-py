"""Direct unit tests for PersistentVector __lt__/__gt__/__le__/__ge__.

These exercise the JVM compareTo semantics (length-first, then element-wise
via the Comparable protocol). The fuzz file covers the random-input property.
"""

import pytest
from clojure._core import vector, eval as _eval, read_string


def _v(*xs):
    return vector(*xs)


def test_lt_length_first():
    # length 2 vs length 1 → left greater (NOT lexicographic)
    assert (_v(1, 2) < _v(3)) is False
    assert (_v(3) < _v(1, 2)) is True


def test_lt_equal_length_element_wise():
    assert (_v(1, 2) < _v(1, 3)) is True
    assert (_v(1, 3) < _v(1, 2)) is False
    assert (_v(1, 2) < _v(1, 2)) is False


def test_lt_empty():
    assert (_v() < _v(1)) is True
    assert (_v(1) < _v()) is False
    assert (_v() < _v()) is False


def test_lt_nested():
    # element-wise recursion: [:a [1]] < [:a [2]] because [1] < [2]
    a = _eval(read_string("[:a [1]]"))
    b = _eval(read_string("[:a [2]]"))
    assert (a < b) is True
    assert (b < a) is False


def test_lt_against_non_vector_raises():
    # Vanilla raises ClassCastException; we surface as TypeError from the
    # Python __lt__ (which `compare_builtin` translates to IllegalArgumentException
    # at the `compare` API level — but at the `<` level Python wants TypeError).
    with pytest.raises(TypeError):
        _v(1) < "a"


def test_gt_mirrors_lt():
    assert (_v(3) > _v(1, 2)) is False
    assert (_v(1, 2) > _v(3)) is True
    assert (_v(1, 3) > _v(1, 2)) is True
    assert (_v(1, 2) > _v(1, 2)) is False


def test_le_ge():
    assert (_v(1, 2) <= _v(1, 2)) is True
    assert (_v(1, 2) <= _v(1, 3)) is True
    assert (_v(1, 3) <= _v(1, 2)) is False
    assert (_v(1, 2) >= _v(1, 2)) is True
    assert (_v(1, 3) >= _v(1, 2)) is True


def test_compare_via_clojure_api():
    # `(compare a b)` routes through the Comparable protocol, which after this
    # task should reach PersistentVector's __lt__/__gt__ via compare_builtin.
    assert _eval(read_string("(compare [1 2] [3])")) == 1
    assert _eval(read_string("(compare [3] [1 2])")) == -1
    assert _eval(read_string("(compare [1 2] [1 2])")) == 0
    assert _eval(read_string("(compare [] [1])")) == -1
    assert _eval(read_string("(compare [] [])")) == 0
    assert _eval(read_string("(compare [:a] [:b])")) == -1
    assert _eval(read_string("(compare [:a [1]] [:a [2]])")) == -1
