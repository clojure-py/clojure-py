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


def test_hash_combine_matches_jvm_anchor():
    """Pinned values mirror Java `Util.hashCombine` for negative seeds."""
    # Reference: Java arithmetic shift `int >> 2` on a negative seed.
    def jvm(seed: int, h: int) -> int:
        MASK = 0xFFFFFFFF
        s32 = seed & MASK
        sh_left = (seed << 6) & MASK
        sh_right = (seed >> 2) & MASK  # Python arithmetic shift, mask to 32
        combined = (h + 0x9e3779b9 + sh_left + sh_right) & MASK
        result = s32 ^ combined
        if result >= 0x80000000:
            result -= 0x100000000
        return result

    cases = [(1, 2), (-1, 0), (0, -1), (-100, 50), (12345, -67890), (-2147483648, 1)]
    for seed, h in cases:
        actual = e(f"(clojure.lang.RT/hash-combine {seed} {h})")
        expected = jvm(seed, h)
        assert actual == expected, f"hash-combine({seed}, {h}): got {actual}, expected {expected}"


# ---------- Record structural hashing ----------

# Define test records once at module-import time.
e("(defrecord HashRec [a b])")
e("(defrecord HashRec2 [a b])")


def test_record_same_fields_same_hash():
    """Two records of the same type with identical fields hash equal."""
    src = "(let [r1 (->HashRec 1 2) r2 (->HashRec 1 2)] (= (hash r1) (hash r2)))"
    assert e(src) is True


def test_record_different_fields_different_hash():
    """Same record type, different field values → different hash (almost always)."""
    src = "(let [r1 (->HashRec 1 2) r2 (->HashRec 1 3)] (not= (hash r1) (hash r2)))"
    assert e(src) is True


def test_record_cross_type_different_hash():
    """Different record types with the same fields → different hash."""
    src = "(let [a (->HashRec 1 2) b (->HashRec2 1 2)] (not= (hash a) (hash b)))"
    assert e(src) is True


def test_record_hash_contract_holds():
    """`(= r1 r2) ⇒ (hash r1) == (hash r2)` — the universal hash contract."""
    src = "(let [r1 (->HashRec 1 2) r2 (->HashRec 1 2)] [(= r1 r2) (= (hash r1) (hash r2))])"
    pair = e(src)
    eq, hash_eq = pair[0], pair[1]
    assert eq and hash_eq


def test_record_no_op_assoc_preserves_hash():
    """`(assoc r :a same-value)` rebuilds the record but with same fields → same hash."""
    src = "(let [r (->HashRec 1 1)] (= (hash (->HashRec 1 1)) (hash (assoc r :a 1))))"
    assert e(src) is True


def test_record_hash_stable_across_calls():
    """Hashing the same record twice produces the same result."""
    src = "(let [r (->HashRec 5 6) h1 (hash r) h2 (hash r)] (= h1 h2))"
    assert e(src) is True
