from clojure._core import Keyword, keyword

def test_keyword_no_ns():
    k = keyword("foo")
    assert k.name == "foo"
    assert k.ns is None

def test_keyword_with_ns():
    k = keyword("ns", "foo")
    assert k.ns == "ns"
    assert k.name == "foo"

def test_keyword_interned_identity():
    assert keyword("foo") is keyword("foo")
    assert keyword("a", "b") is keyword("a", "b")

def test_keyword_distinct_by_ns():
    assert keyword("foo") is not keyword("ns", "foo")

def test_keyword_hash_stable():
    assert hash(keyword("foo")) == hash(keyword("foo"))

def test_keyword_repr():
    assert repr(keyword("foo")) == ":foo"
    assert repr(keyword("ns", "foo")) == ":ns/foo"

def test_keyword_callable_get():
    d = {keyword("a"): 1, keyword("b"): 2}
    assert keyword("a")(d) == 1
    assert keyword("c")(d) is None
    assert keyword("c")(d, "default") == "default"

def test_keyword_from_slash_form():
    k = keyword("ns/foo")
    assert k.ns == "ns"
    assert k.name == "foo"

def test_keyword_concurrent_intern():
    import threading
    results = []
    def worker():
        for _ in range(1000):
            results.append(keyword("shared"))
    ts = [threading.Thread(target=worker) for _ in range(16)]
    for t in ts: t.start()
    for t in ts: t.join()
    first = results[0]
    assert all(r is first for r in results)
