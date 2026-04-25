"""Property-based fuzzing of record hash contract.

`(= r1 r2) ⇒ (hash r1) == (hash r2)` is a universal hash contract.
We exercise it over random small-int field tuples.
"""

from hypothesis import given, strategies as st
from clojure._core import eval_string as e


# Define a record type once at module init.
e("(defrecord FuzzRec [a b c])")


small = st.integers(min_value=-100, max_value=100)


@given(a1=small, b1=small, c1=small, a2=small, b2=small, c2=small)
def test_record_hash_respects_equality(a1, b1, c1, a2, b2, c2):
    src = (
        f"(let [r1 (->FuzzRec {a1} {b1} {c1}) "
        f"      r2 (->FuzzRec {a2} {b2} {c2})] "
        f"  [(= r1 r2) (= (hash r1) (hash r2))])"
    )
    eq, hash_eq = e(src)
    if eq:
        assert hash_eq, (
            f"Hash contract violated: ({a1} {b1} {c1}) == ({a2} {b2} {c2}) "
            f"but hashes differ"
        )


@given(a=small, b=small, c=small)
def test_record_self_hash_stable(a, b, c):
    """Same fields, different instance — hashes match."""
    src = (
        f"(let [r1 (->FuzzRec {a} {b} {c}) "
        f"      r2 (->FuzzRec {a} {b} {c})] "
        f"  (= (hash r1) (hash r2)))"
    )
    assert e(src) is True
