from clojure._core import IEquiv, IHashEq, equiv, hash_eq, Protocol


def test_iequiv_is_registered():
    assert isinstance(IEquiv, Protocol)


def test_ihasheq_is_registered():
    assert isinstance(IHashEq, Protocol)


def test_equiv_on_ints():
    assert equiv(1, 1) is True
    assert equiv(1, 2) is False


def test_equiv_on_strings():
    assert equiv("a", "a") is True
    assert equiv("a", "b") is False


def test_equiv_on_none():
    assert equiv(None, None) is True


def test_equiv_cross_type_false():
    # Python's 1 == "1" is False — fallback should reflect that.
    assert equiv(1, "1") is False


def test_hash_eq_on_int():
    # hash_eq for ints uses Murmur3.hashLong (vanilla Clojure parity), so
    # it diverges from Python's `hash(int)` which is the int value itself.
    h = hash_eq(42)
    assert isinstance(h, int)
    assert h != 42  # Murmur3-mixed; not identity
    assert hash_eq(42) == hash_eq(42)
    # Distinct ints get distinct hashes (no trivial collisions).
    assert hash_eq(42) != hash_eq(43)


def test_hash_eq_on_string():
    assert hash_eq("hello") == hash("hello")


def test_hash_eq_equivalent_values_same_hash():
    assert hash_eq(42) == hash_eq(42)


def test_equiv_true_false_booleans():
    assert equiv(True, True) is True
    assert equiv(False, False) is True
    assert equiv(True, False) is False
