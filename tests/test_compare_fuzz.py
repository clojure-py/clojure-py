"""Property-based tests for clojure.core/compare.

Matches the project's preference for fuzzing data-structure ops against a
Python reference implementation instead of one-shot unit tests."""

from hypothesis import given, strategies as st
from clojure._core import eval_string


def _py_sign(a, b):
    return (a > b) - (a < b)


nums = st.one_of(st.integers(-10**6, 10**6), st.floats(min_value=-1e6, max_value=1e6, allow_nan=False))
strs = st.text(min_size=0, max_size=20)


@given(nums, nums)
def test_compare_ints_and_floats_match_python(a, b):
    got = eval_string(f"(compare {a!r} {b!r})")
    # `compare` returns -1/0/1 or any signed value; we compare by sign.
    assert _py_sign(got, 0) == _py_sign(a, b)


@given(strs, strs)
def test_compare_strings_match_python(a, b):
    a_s = a.replace('\\', '\\\\').replace('"', '\\"')
    b_s = b.replace('\\', '\\\\').replace('"', '\\"')
    got = eval_string(f'(compare "{a_s}" "{b_s}")')
    assert _py_sign(got, 0) == _py_sign(a, b)


@given(nums)
def test_compare_reflexive(x):
    assert eval_string(f"(compare {x!r} {x!r})") == 0


@given(nums, nums)
def test_compare_antisymmetric(a, b):
    ab = eval_string(f"(compare {a!r} {b!r})")
    ba = eval_string(f"(compare {b!r} {a!r})")
    assert _py_sign(ab, 0) == -_py_sign(ba, 0)
