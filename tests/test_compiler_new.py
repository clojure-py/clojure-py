"""Compiler tests — `new` and the `(Class. args)` sugar.

Both forms compile to a regular call where the callable is the class —
in Python, instantiation is just calling the class.

We import Python's `list` / `dict` / `ValueError` / `TypeError` over
their existing user-ns mappings (e.g. `list` is normally
clojure.core/list) for the duration of the tests, then restore the
prior bindings via an autouse fixture so we don't pollute downstream
test files. Without restoration the test suite is order-sensitive:
once these tests run, any later test that does `(list 1 2 3)` would
hit Python's list constructor (max 1 arg) instead of clojure.core/list.
"""

import pytest

import clojure.core  # bootstrap so the original user-ns refers exist
from clojure.lang import (
    Compiler,
    read_string,
    Var, Symbol,
)


class _Box:
    def __init__(self, value=None):
        self.value = value


class _Pair:
    def __init__(self, a, b):
        self.a = a
        self.b = b


# Names whose user-ns mapping we'll save / restore around this module.
_OVERRIDDEN_NAMES = ("list", "dict", "ValueError", "TypeError",
                     "cnw-Box", "cnw-Pair")


@pytest.fixture(autouse=True, scope="module")
def _override_user_ns():
    """Override user-ns mappings for the test module, then restore."""
    ns = Compiler.current_ns()
    saved = {}
    for name in _OVERRIDDEN_NAMES:
        saved[name] = ns.get_mapping(Symbol.intern(name))
    ns.import_class(ValueError)
    ns.import_class(TypeError)
    ns.import_class(list)
    ns.import_class(dict)
    Var.intern(ns, Symbol.intern("cnw-Box"), _Box)
    Var.intern(ns, Symbol.intern("cnw-Pair"), _Pair)
    yield
    # Restore (or unmap if there was no prior mapping).
    for name, prior in saved.items():
        sym = Symbol.intern(name)
        if prior is None:
            ns.unmap(sym)
        elif isinstance(prior, Var):
            ns.refer(sym, prior)
        else:
            ns.import_class(sym, prior)


def _eval(src):
    return Compiler.eval(read_string(src))


# --- new ---------------------------------------------------------------

def test_new_no_args():
    assert _eval("(new list)") == []
    assert _eval("(new dict)") == {}

def test_new_with_args():
    e = _eval('(new ValueError "oops")')
    assert isinstance(e, ValueError)
    assert str(e) == "oops"

def test_new_custom_class():
    p = _eval("(new cnw-Pair 1 2)")
    assert isinstance(p, _Pair)
    assert (p.a, p.b) == (1, 2)

def test_new_requires_class_name():
    with pytest.raises(SyntaxError):
        _eval("(new)")


# --- (Class. args) sugar ----------------------------------------------

def test_ctor_sugar_no_args():
    assert _eval("(list.)") == []
    assert _eval("(dict.)") == {}

def test_ctor_sugar_with_args():
    e = _eval('(ValueError. "boom")')
    assert isinstance(e, ValueError)
    assert str(e) == "boom"

def test_ctor_sugar_custom_class():
    b = _eval("(cnw-Box. 42)")
    assert isinstance(b, _Box)
    assert b.value == 42

def test_ctor_sugar_two_args():
    p = _eval("(cnw-Pair. 7 11)")
    assert isinstance(p, _Pair)
    assert (p.a, p.b) == (7, 11)


# --- combined with throw ----------------------------------------------

def test_throw_constructed_exception():
    with pytest.raises(ValueError, match="bang"):
        _eval('(throw (ValueError. "bang"))')

def test_caught_constructed_exception():
    assert _eval(
        '(try (throw (TypeError. "x")) (catch TypeError e :te))'
    ) == _eval(":te")


# --- combined with let / fn -------------------------------------------

def test_new_in_let():
    assert _eval("(let* [b (cnw-Box. 99)] (.-value b))") == 99

def test_new_in_fn_body():
    f = _eval("(fn* [x y] (cnw-Pair. x y))")
    p = f(3, 4)
    assert (p.a, p.b) == (3, 4)
