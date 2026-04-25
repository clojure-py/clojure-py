"""Record structural hashing — hash-combine helper + IHashEq on defrecord."""

from clojure._core import eval_string as e


def test_hash_combine_is_deterministic():
    """`(clojure.lang.RT/hash-combine seed h)` produces a stable combined int."""
    a = e("(clojure.lang.RT/hash-combine 1 2)")
    b = e("(clojure.lang.RT/hash-combine 1 2)")
    assert a == b


def test_hash_combine_is_seed_sensitive():
    """Different seeds produce different combined values for the same h."""
    a = e("(clojure.lang.RT/hash-combine 1 100)")
    b = e("(clojure.lang.RT/hash-combine 2 100)")
    assert a != b


def test_hash_combine_is_h_sensitive():
    """Different h values produce different combined values for the same seed."""
    a = e("(clojure.lang.RT/hash-combine 100 1)")
    b = e("(clojure.lang.RT/hash-combine 100 2)")
    assert a != b


def test_hash_combine_zero_inputs():
    """Combining with zero seed should be deterministic and well-defined."""
    a = e("(clojure.lang.RT/hash-combine 0 0)")
    b = e("(clojure.lang.RT/hash-combine 0 0)")
    assert a == b
