"""Property-based fuzzing of PersistentVector compare.

Asserts JVM compareTo semantics: length-first, then element-wise via
clojure.core/compare. Oracle is a Python implementation of the same
algorithm using simple ints.
"""

import functools
from hypothesis import given, strategies as st

from clojure._core import vector, eval as _eval, read_string


# Small ints keep the search space tractable for the recursive element compare.
small_ints = st.integers(min_value=-100, max_value=100)
small_vecs = st.lists(small_ints, min_size=0, max_size=8)


def _oracle_compare(a, b):
    """JVM APersistentVector.compareTo over int lists."""
    if len(a) < len(b): return -1
    if len(a) > len(b): return 1
    for x, y in zip(a, b):
        if x < y: return -1
        if x > y: return 1
    return 0


def _to_clj_vec(xs):
    return vector(*xs)


def _print_vec(xs):
    return "[" + " ".join(str(x) for x in xs) + "]"


@given(a=small_vecs, b=small_vecs)
def test_compare_matches_oracle(a, b):
    cl_result = _eval(read_string(f"(compare {_print_vec(a)} {_print_vec(b)})"))
    expected = _oracle_compare(a, b)
    assert cl_result == expected, f"compare {a} {b}: got {cl_result}, expected {expected}"


@given(a=small_vecs, b=small_vecs)
def test_lt_matches_oracle(a, b):
    av = _to_clj_vec(a)
    bv = _to_clj_vec(b)
    expected = _oracle_compare(a, b) < 0
    assert (av < bv) is expected


@given(a=small_vecs, b=small_vecs)
def test_compare_is_antisymmetric(a, b):
    """compare(a, b) and compare(b, a) have opposite signs (or both zero)."""
    ab = _oracle_compare(a, b)
    ba = _oracle_compare(b, a)
    if ab == 0:
        assert ba == 0
    else:
        assert ab * ba < 0


@given(xs=small_vecs)
def test_compare_self_is_zero(xs):
    src = _print_vec(xs)
    assert _eval(read_string(f"(compare {src} {src})")) == 0


@given(xs=st.lists(small_vecs, min_size=0, max_size=10))
def test_sorted_set_orders_by_compare(xs):
    """sorted-set of vectors orders them per clojure.core/compare."""
    src = "(vec (sorted-set " + " ".join(_print_vec(v) for v in xs) + "))"
    result = _eval(read_string(src))
    # Build expected: dedupe + sort by oracle compare.
    seen = []
    for v in xs:
        if v not in seen:
            seen.append(v)
    seen.sort(key=functools.cmp_to_key(_oracle_compare))
    assert [list(v) for v in result] == seen
