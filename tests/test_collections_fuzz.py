"""Property-based fuzzing of persistent + transient collections vs Python references.

Generates random op sequences and asserts observable equivalence with
`list` / `dict` / `set`. Catches HAMT bugs (path-copy mistakes, bitmap
miscounts, transient-edit-token bugs) that example-based tests miss.
"""

import pytest
from hypothesis import given, settings, strategies as st, assume
from clojure._core import (
    vector, hash_map, array_map, hash_set, keyword,
    transient, persistent_bang, conj_bang, assoc_bang, dissoc_bang, disj_bang, pop_bang,
    IllegalStateException,
)


# ---------- strategies ----------

# Simple hashable/equal values. We avoid floats (NaN != NaN breaks reference reasoning).
simple_values = st.one_of(
    st.integers(min_value=-1000, max_value=1000),
    st.text(max_size=10),
    st.booleans(),
    st.none(),
)

# Keys also simple hashable.
simple_keys = st.one_of(
    st.integers(min_value=-1000, max_value=1000),
    st.text(max_size=10),
)


# ---------- Vector ----------

@st.composite
def vector_op_sequence(draw):
    """A list of ops applied to a vector. Each op is ('conj', v) or ('pop',) or ('assoc', i, v)."""
    n = draw(st.integers(min_value=0, max_value=50))
    ops = []
    expected_len = 0
    for _ in range(n):
        if expected_len == 0:
            # Can only conj.
            ops.append(('conj', draw(simple_values)))
            expected_len += 1
        else:
            choice = draw(st.integers(min_value=0, max_value=2))
            if choice == 0:
                ops.append(('conj', draw(simple_values)))
                expected_len += 1
            elif choice == 1:
                ops.append(('pop',))
                expected_len -= 1
            else:
                idx = draw(st.integers(min_value=0, max_value=expected_len - 1))
                ops.append(('assoc', idx, draw(simple_values)))
    return ops


@given(vector_op_sequence())
@settings(deadline=None)
def test_vector_ops_match_python_list(ops):
    v = vector()
    ref = []
    for op in ops:
        if op[0] == 'conj':
            v = v.conj(op[1])
            ref.append(op[1])
        elif op[0] == 'pop':
            v = v.pop()
            ref.pop()
        elif op[0] == 'assoc':
            _, i, val = op
            v = v.assoc_n(i, val)
            ref[i] = val
    # Invariants:
    assert len(v) == len(ref)
    for i, expected in enumerate(ref):
        assert v.nth(i) == expected, f"mismatch at index {i}: {v.nth(i)!r} vs {expected!r}"
    # Iteration order.
    assert list(v) == ref


@given(st.lists(simple_values, max_size=100))
@settings(deadline=None)
def test_vector_conj_then_iter_equals_list(values):
    v = vector()
    for x in values:
        v = v.conj(x)
    assert list(v) == values
    assert len(v) == len(values)


# ---------- Vector via transient ----------

@given(st.lists(simple_values, max_size=100))
@settings(deadline=None)
def test_transient_vector_conj_round_trip(values):
    t = transient(vector())
    for x in values:
        conj_bang(t, x)
    v = persistent_bang(t)
    assert list(v) == values


@given(st.lists(simple_values, min_size=1, max_size=100))
@settings(deadline=None)
def test_transient_vector_assoc_bang(values):
    # Build persistent via conj.
    v = vector()
    for x in values:
        v = v.conj(x)
    # Transient-mutate every other index.
    t = transient(v)
    for i in range(0, len(values), 2):
        t = assoc_bang(t, i, "MUT")
    v2 = persistent_bang(t)
    for i in range(len(values)):
        if i % 2 == 0:
            assert v2.nth(i) == "MUT"
        else:
            assert v2.nth(i) == values[i]


# ---------- HashMap / ArrayMap ----------

@st.composite
def map_op_sequence(draw):
    """Sequence of ('assoc', k, v) or ('dissoc', k) ops."""
    n = draw(st.integers(min_value=0, max_value=80))
    ops = []
    for _ in range(n):
        if draw(st.booleans()):
            ops.append(('assoc', draw(simple_keys), draw(simple_values)))
        else:
            ops.append(('dissoc', draw(simple_keys)))
    return ops


@given(map_op_sequence())
@settings(deadline=None)
def test_hash_map_ops_match_python_dict(ops):
    m = hash_map()
    ref = {}
    for op in ops:
        if op[0] == 'assoc':
            _, k, v = op
            m = m.assoc(k, v)
            ref[k] = v
        else:
            _, k = op
            m = m.without(k)
            ref.pop(k, None)
    # The result may be PersistentArrayMap or PersistentHashMap due to promotion —
    # that's fine. Both satisfy val_at / contains_key / count.
    assert len(m) == len(ref)
    for k, expected in ref.items():
        assert m.val_at(k) == expected
    # Keys iteration:
    assert set(iter(m)) == set(ref.keys())
    # Negative check:
    missing_key = "__definitely_not_in_the_map__"
    if missing_key not in ref:
        assert m.val_at(missing_key) is None
        assert m.contains_key(missing_key) is False


@given(map_op_sequence())
@settings(deadline=None)
def test_array_map_ops_match_python_dict(ops):
    """array_map may promote to hash_map past 8 entries — both must still be correct."""
    m = array_map()
    ref = {}
    for op in ops:
        if op[0] == 'assoc':
            _, k, v = op
            m = m.assoc(k, v)
            ref[k] = v
        else:
            _, k = op
            m = m.without(k)
            ref.pop(k, None)
    assert len(m) == len(ref)
    for k, expected in ref.items():
        assert m.val_at(k) == expected


# ---------- HashMap via transient ----------

@given(st.dictionaries(simple_keys, simple_values, max_size=100))
@settings(deadline=None)
def test_transient_hash_map_build_round_trip(ref_dict):
    t = transient(hash_map())
    for k, v in ref_dict.items():
        assoc_bang(t, k, v)
    m = persistent_bang(t)
    assert len(m) == len(ref_dict)
    for k, v in ref_dict.items():
        assert m.val_at(k) == v


@given(st.dictionaries(simple_keys, simple_values, min_size=1, max_size=100), st.data())
@settings(deadline=None)
def test_transient_hash_map_dissoc_bang(ref_dict, data):
    m = hash_map()
    for k, v in ref_dict.items():
        m = m.assoc(k, v)
    keys = list(ref_dict.keys())
    remove_key = data.draw(st.sampled_from(keys))
    t = transient(m)
    dissoc_bang(t, remove_key)
    m2 = persistent_bang(t)
    assert len(m2) == len(ref_dict) - 1
    assert m2.val_at(remove_key) is None
    for k, v in ref_dict.items():
        if k != remove_key:
            assert m2.val_at(k) == v


# ---------- HashSet ----------

@st.composite
def set_op_sequence(draw):
    n = draw(st.integers(min_value=0, max_value=60))
    ops = []
    for _ in range(n):
        if draw(st.booleans()):
            ops.append(('conj', draw(simple_values)))
        else:
            ops.append(('disj', draw(simple_values)))
    return ops


@given(set_op_sequence())
@settings(deadline=None)
def test_hash_set_ops_match_python_set(ops):
    s = hash_set()
    ref = set()
    for op in ops:
        if op[0] == 'conj':
            s = s.conj(op[1])
            ref.add(op[1])
        else:
            s = s.disjoin(op[1])
            ref.discard(op[1])
    assert len(s) == len(ref)
    for x in ref:
        assert s.contains(x)
    # Iterate.
    assert set(iter(s)) == ref


@given(st.sets(simple_values, max_size=100))
@settings(deadline=None)
def test_transient_hash_set_round_trip(ref_set):
    t = transient(hash_set())
    for x in ref_set:
        conj_bang(t, x)
    s = persistent_bang(t)
    assert len(s) == len(ref_set)
    for x in ref_set:
        assert s.contains(x)


# ---------- Structural sharing integrity ----------

@given(st.lists(simple_values, min_size=0, max_size=100), st.lists(simple_values, max_size=50))
@settings(deadline=None)
def test_vector_structural_sharing_preserved(initial, derived_ops):
    v1 = vector()
    for x in initial:
        v1 = v1.conj(x)
    # Make derivative.
    v2 = v1
    for x in derived_ops:
        v2 = v2.conj(x)
    # v1 must be unchanged.
    assert len(v1) == len(initial)
    assert list(v1) == initial


@given(st.dictionaries(simple_keys, simple_values, max_size=50),
       st.dictionaries(simple_keys, simple_values, max_size=20))
@settings(deadline=None)
def test_hash_map_structural_sharing_preserved(initial, derived):
    m1 = hash_map()
    for k, v in initial.items():
        m1 = m1.assoc(k, v)
    m2 = m1
    for k, v in derived.items():
        m2 = m2.assoc(k, v)
    # m1 must be unchanged.
    assert len(m1) == len(initial)
    for k, v in initial.items():
        assert m1.val_at(k) == v


# ---------- Deep HAMT stress ----------

@given(st.integers(min_value=33, max_value=2000))
@settings(deadline=None, max_examples=50)  # fewer cases since each is expensive
def test_vector_deep_trie_correctness(n):
    """Vectors past the tail-only threshold (32) exercise multi-level trie."""
    v = vector()
    for i in range(n):
        v = v.conj(i)
    assert len(v) == n
    for i in range(n):
        assert v.nth(i) == i


@given(st.integers(min_value=33, max_value=2000))
@settings(deadline=None, max_examples=50)
def test_hash_map_deep_trie_correctness(n):
    m = hash_map()
    for i in range(n):
        m = m.assoc(i, i * 2)
    assert len(m) == n
    for i in range(n):
        assert m.val_at(i) == i * 2
