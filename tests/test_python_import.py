"""Python import-machinery integration: `import foo` / `from foo import bar`
should locate `foo.clj` on `sys.path`, load it, and expose its interned Vars
as module attributes.
"""
import sys
import textwrap

import pytest
import clojure  # installs ClojureFinder on sys.meta_path
from clojure._core import ClojureNamespace, Var


@pytest.fixture
def clj_path(tmp_path, monkeypatch):
    monkeypatch.syspath_prepend(str(tmp_path))
    return tmp_path


def _drop(name):
    sys.modules.pop(name, None)


def _write(path, text):
    path.write_text(textwrap.dedent(text).lstrip())


def test_flat_namespace_callable(clj_path):
    _write(clj_path / "flat_mod.clj", """
        (ns flat-mod)
        (defn greet [x] (str "hi " x))
    """)
    _drop("flat_mod")
    _drop("flat-mod")
    from flat_mod import greet
    assert greet("world") == "hi world"


def test_dotted_namespace_callable(clj_path):
    (clj_path / "pkg").mkdir()
    _write(clj_path / "pkg" / "sub_mod.clj", """
        (ns pkg.sub-mod)
        (defn add2 [x] (+ x 2))
    """)
    for n in ("pkg", "pkg.sub_mod", "pkg.sub-mod"):
        _drop(n)
    from pkg.sub_mod import add2
    assert add2(40) == 42


def test_non_callable_var_arithmetic(clj_path):
    _write(clj_path / "vals_mod.clj", """
        (ns vals-mod)
        (def answer 42)
    """)
    _drop("vals_mod")
    _drop("vals-mod")
    from vals_mod import answer
    # Var passes __add__ through to its root.
    assert answer + 1 == 43


def test_loaded_module_is_clojure_namespace(clj_path):
    _write(clj_path / "mod_kind.clj", """
        (ns mod-kind)
        (def x 1)
    """)
    _drop("mod_kind")
    _drop("mod-kind")
    import mod_kind
    assert isinstance(mod_kind, ClojureNamespace)
    assert isinstance(mod_kind.x, Var)


def test_missing_clj_passes_through(clj_path):
    # Nothing to write — just confirm the finder returns None cleanly.
    _drop("definitely_not_a_clj_module_xyzzy")
    with pytest.raises(ModuleNotFoundError):
        import definitely_not_a_clj_module_xyzzy  # noqa: F401


def test_clojure_ns_authoritative_for_dashed_names(clj_path):
    """File `my_lib.clj` with `(ns my-lib)` must be reachable via BOTH
    Python `import my_lib` AND Clojure `(find-ns 'my-lib)`, pointing at
    the same ns object."""
    _write(clj_path / "my_lib.clj", """
        (ns my-lib)
        (defn top [] 1)
    """)
    _drop("my_lib")
    _drop("my-lib")
    import my_lib
    assert my_lib is sys.modules["my_lib"]
    assert my_lib is sys.modules["my-lib"]
    # Clojure-side lookup via find-ns finds the same object.
    from clojure._core import eval_string
    found = eval_string("(find-ns 'my-lib)")
    assert found is my_lib
    # And `my-lib/top` works from Clojure.
    assert eval_string("(my-lib/top)") == 1


def test_no_ns_form_keeps_initial_module(clj_path):
    """A file without an explicit `(ns ...)` form should load into the
    initial (spec.name) namespace — no rewiring needed."""
    _write(clj_path / "no_ns.clj", """
        (def answer 7)
    """)
    _drop("no_ns")
    import no_ns
    # answer is a Var; deref via arithmetic.
    assert no_ns.answer + 0 == 7
    assert sys.modules["no_ns"] is no_ns
