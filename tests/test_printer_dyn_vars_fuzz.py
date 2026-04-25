"""Property-based fuzzing of *print-length* and *print-level*."""

from hypothesis import given, strategies as st
from clojure._core import eval_string as e


small_int = st.integers(min_value=-100, max_value=100)
bounded_lists = st.lists(small_int, min_size=0, max_size=20)


@given(items=bounded_lists, n=st.integers(min_value=0, max_value=10))
def test_print_length_truncates_to_n_or_fewer(items, n):
    """Printed form has at most n elements (plus possibly '...')."""
    items_str = " ".join(str(i) for i in items)
    src = f'(binding [*print-length* {n}] (pr-str [{items_str}]))'
    result = e(src)
    assert result.startswith("[") and result.endswith("]")
    inner = result[1:-1]
    if not inner:
        return
    parts = inner.split(" ")
    if len(items) > n:
        assert parts[-1] == "...", f"Expected truncation marker in {result!r}"
        assert len(parts) == n + 1
    else:
        assert len(parts) == len(items)


@given(depth=st.integers(min_value=0, max_value=4),
       level=st.integers(min_value=0, max_value=5))
def test_print_level_substitutes_pound(depth, level):
    """A nested vector of depth `depth` should be printed with no nested
    structure at depth >= level (replaced with '#').
    """
    inner = "1"
    for _ in range(depth):
        inner = f"[{inner}]"
    src = f'(binding [*print-level* {level}] (pr-str {inner}))'
    result = e(src)
    if level == 0:
        if depth >= 1:
            assert result == "#"
        else:
            assert result == "1"
    elif level > depth:
        assert "#" not in result
