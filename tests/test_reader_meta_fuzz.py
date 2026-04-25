"""Property-based fuzzing of reader meta merge and map dup-key rejection."""

from hypothesis import given, strategies as st
import pytest

from clojure._core import eval as _eval, read_string, ReaderError


# Keyword names: short ASCII identifiers a-z.
kw_names = st.from_regex(r"[a-z]{1,4}", fullmatch=True).map(lambda s: ":" + s)


# Distinct list of keyword names.
distinct_kws = st.lists(kw_names, min_size=1, max_size=8, unique=True)


# Two distinct keywords for conflict tests.
two_distinct_kws = st.lists(kw_names, min_size=2, max_size=2, unique=True)


# ---------- Chained meta merge ----------

@given(kws=distinct_kws)
def test_chained_keyword_meta_merges_all(kws):
    """`^:a ^:b ^:c sym` produces meta containing all the keys."""
    chain = " ".join(f"^{kw}" for kw in kws) + " sym"
    # Clojure string literals use double quotes only; repr() gives Python single-quoted
    # strings which Clojure won't recognise, so we build the literal explicitly.
    src_lit = '"' + chain + '"'
    for kw in kws:
        v = _eval(read_string(f'(get (meta (read-string {src_lit})) {kw})'))
        assert v is True, f"missing or wrong value for {kw} in meta of {chain}"


# ---------- Map dup-key ----------

@given(keys=distinct_kws)
def test_distinct_kw_keys_accepted(keys):
    """A map built from distinct keyword keys reads successfully."""
    pairs = " ".join(f"{kw} {i}" for i, kw in enumerate(keys))
    src = "{" + pairs + "}"
    n = _eval(read_string(f"(count {src})"))
    assert n == len(keys)


@given(kws=two_distinct_kws)
def test_duplicate_kw_key_rejected(kws):
    """Duplicating the first key triggers ReaderError."""
    a, b = kws
    src = f"{{{a} 1 {b} 2 {a} 3}}"
    with pytest.raises(ReaderError) as ei:
        read_string(src)
    msg = str(ei.value)
    assert "Duplicate key" in msg
    assert a in msg
