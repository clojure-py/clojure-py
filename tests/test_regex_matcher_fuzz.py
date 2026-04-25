"""Property fuzz: nth on Matcher returns the i-th group of the last match."""

from hypothesis import given, strategies as st
from clojure._core import eval_string as e


@given(parts=st.lists(
    st.text(alphabet="0123456789", min_size=1, max_size=4),
    min_size=1, max_size=5,
))
def test_nth_returns_each_group(parts):
    """(\\d+)/(\\d+)/.../(\\d+) against n/n/.../n — nth i matches part i-1."""
    pat_inner = "/".join("(\\d+)" for _ in parts)
    s = "/".join(parts)
    src = f'''
    (let [m (re-matcher #"{pat_inner}" "{s}")]
      (re-find m)
      (vec (for [i (range {len(parts) + 1})] (nth m i))))
    '''
    result = e(src)
    expected = [s] + parts
    assert list(result) == expected
