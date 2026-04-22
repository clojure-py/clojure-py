"""create-ns, find-ns, and auto-parent placeholder semantics."""

import sys
import types
from clojure._core import create_ns, find_ns, symbol, ClojureNamespace


# Use unique names to avoid cross-test pollution via sys.modules.

def test_create_simple():
    ns = create_ns(symbol("test.simple"))
    assert isinstance(ns, ClojureNamespace)
    assert sys.modules["test.simple"] is ns


def test_idempotent():
    ns1 = create_ns(symbol("test.idempotent"))
    ns2 = create_ns(symbol("test.idempotent"))
    assert ns1 is ns2


def test_dunder_metadata_populated():
    ns = create_ns(symbol("test.meta"))
    assert ns.__clj_ns__ == symbol("test.meta")
    assert ns.__clj_ns_meta__ is None
    assert ns.__clj_aliases__ == {}
    assert ns.__clj_refers__ == {}
    assert ns.__clj_imports__ == {}


def test_dotted_auto_parent_is_plain_ModuleType_not_ClojureNamespace():
    """When we create a.b.c, the intermediate a and a.b are bare ModuleType
    placeholders — NOT ClojureNamespaces. They exist only so Python's import
    machinery works. Clojure namespaces are flat."""
    create_ns(symbol("ap.b.c"))
    assert "ap" in sys.modules
    assert "ap.b" in sys.modules
    assert "ap.b.c" in sys.modules

    # Bare placeholders: exact type is types.ModuleType (not a subclass).
    assert type(sys.modules["ap"]) is types.ModuleType
    assert type(sys.modules["ap"]) is not ClojureNamespace
    assert not isinstance(sys.modules["ap"], ClojureNamespace)

    assert type(sys.modules["ap.b"]) is types.ModuleType
    assert not isinstance(sys.modules["ap.b"], ClojureNamespace)

    # Terminal is a ClojureNamespace subclass instance.
    assert isinstance(sys.modules["ap.b.c"], ClojureNamespace)

    # Placeholders have no Clojure metadata:
    assert not hasattr(sys.modules["ap"], "__clj_ns__")
    assert not hasattr(sys.modules["ap.b"], "__clj_ns__")


def test_find_ns_skips_placeholders():
    """find-ns returns a namespace only for ClojureNamespace entries — never
    for bare ModuleType placeholders, even if they were auto-created by us."""
    create_ns(symbol("fn.b.c"))
    assert find_ns(symbol("fn.b.c")) is sys.modules["fn.b.c"]
    assert find_ns(symbol("fn")) is None   # placeholder — not a namespace
    assert find_ns(symbol("fn.b")) is None  # placeholder — not a namespace
    assert find_ns(symbol("fn.nonexistent")) is None


def test_explicit_create_upgrades_placeholder():
    """If we later explicitly create-ns a name currently held by a bare
    placeholder, it's replaced with a real ClojureNamespace. Identity of the
    old placeholder is not preserved — users shouldn't pre-cache references
    to auto-created placeholders."""
    create_ns(symbol("up.b.c"))
    placeholder = sys.modules["up"]
    assert type(placeholder) is types.ModuleType

    created = create_ns(symbol("up"))
    assert isinstance(created, ClojureNamespace)
    assert sys.modules["up"] is created
    assert sys.modules["up"] is not placeholder  # replaced


def test_parent_attribute_link():
    ns = create_ns(symbol("pa.child"))
    # sys.modules["pa"] is a bare placeholder; it has .child attribute wired.
    assert sys.modules["pa"].child is ns
    # Explicitly create pa — it should re-wire the attribute link.
    pa = create_ns(symbol("pa"))
    assert pa.child is ns


def test_python_import_works():
    """import a.b.c from Python should resolve via the placeholder/namespace chain."""
    create_ns(symbol("imptest.b.c"))
    import imptest.b.c  # noqa: F401
    assert imptest.b.c is sys.modules["imptest.b.c"]


def test_find_ns_nonexistent_returns_none():
    assert find_ns(symbol("definitely.does.not.exist")) is None


def test_namespace_is_subclass_of_ModuleType():
    ns = create_ns(symbol("subclass.check"))
    assert isinstance(ns, types.ModuleType)
