"""Tests for core.clj batch 15 (lines 3455-3471): the `import` macro
plus the `import*` special form it expands into.

Forms (1 macro + 1 special form):
  import        — 4 call shapes: 'sym, '(pkg Cls...), [pkg Cls...], mixed
  import*       — newly-implemented compiler special form (was a stub)

Backend additions:
  Compiler.import_class_by_name  — runtime helper invoked by import*
  _compile_import_star           — the special-form emitter

The compiler also gained recognition of `clojure.core/<special>`
(qualified) special-form heads in addition to bare ones, so a macro
whose body emits `'clojure.core/import*` (as JVM's import does)
dispatches correctly. This was a side-effect needed for batch 15;
worth its own tests too.
"""

import collections
import json

import pytest

import clojure.core  # triggers load

from clojure.lang import (
    Compiler,
    read_string,
    Var, Symbol, Namespace,
    RT,
)


def E(src):
    return Compiler.eval(read_string(src))


# --- compiler change: clojure.core/<special> dispatch -------------

def test_qualified_let_star():
    """Side-effect of the import-batch refactor: `clojure.core/let*` now
    dispatches as a special form, mirroring JVM's syntax-quote
    auto-qualification."""
    assert E("(clojure.core/let* [x 5] x)") == 5

def test_qualified_if():
    assert E("(clojure.core/if true 1 2)") == 1

def test_qualified_do():
    assert E("(clojure.core/do 1 2 3)") == 3

def test_qualified_quote():
    out = E("(clojure.core/quote (a b c))")
    # Symbols in the quoted list — just check the str form.
    assert str(out) == "(a b c)"


# --- import* special form -----------------------------------------

def test_import_star_basic():
    """import* returns the class and installs it in the current ns."""
    out = E('(import* "collections.OrderedDict")')
    assert out is collections.OrderedDict
    assert E("OrderedDict") is collections.OrderedDict
    # Can construct via the short name.
    assert isinstance(E("(OrderedDict)"), collections.OrderedDict)

def test_import_star_qualified():
    """clojure.core/import* dispatches the same as bare import*."""
    out = E('(clojure.core/import* "collections.UserList")')
    assert out is collections.UserList
    assert E("UserList") is collections.UserList

def test_import_star_missing_class_raises():
    with pytest.raises((ImportError, ModuleNotFoundError, AttributeError)):
        E('(import* "nonexistent.module.Class")')

def test_import_star_non_string_arg_raises():
    with pytest.raises(SyntaxError, match="string literal"):
        E("(import* :keyword)")

def test_import_star_zero_args_raises():
    with pytest.raises(SyntaxError, match="class-name string"):
        E("(import*)")

def test_import_star_too_many_args_raises():
    with pytest.raises(SyntaxError, match="exactly one"):
        E('(import* "a.b" "c.d")')

def test_import_star_runtime_semantics_when_false():
    """The form runs at execution time, not compile time. (when false ...)
    must not import."""
    # Make sure it's NOT already imported.
    Compiler.current_ns().unmap(Symbol.intern("UserString"))
    E('(clojure.core/when false (import* "collections.UserString"))')
    with pytest.raises(NameError):
        E("UserString")

def test_import_star_runtime_semantics_when_true():
    Compiler.current_ns().unmap(Symbol.intern("UserDict"))
    E('(clojure.core/when true (import* "collections.UserDict"))')
    assert E("UserDict") is collections.UserDict

def test_import_star_inside_fn_body():
    """A fn that imports on call. The fn returns the class."""
    Compiler.current_ns().unmap(Symbol.intern("Error"))
    E('(clojure.core/def __tcb15-import-fn (fn* [] (import* "shutil.Error")))')
    out = E("(__tcb15-import-fn)")
    import shutil
    assert out is shutil.Error
    assert E("Error") is shutil.Error


# --- import macro: 4 call shapes ----------------------------------

def test_import_single_symbol():
    Compiler.current_ns().unmap(Symbol.intern("Counter"))
    out = E("(clojure.core/import 'collections.Counter)")
    assert out is collections.Counter
    assert E("Counter") is collections.Counter

def test_import_quoted_list_form():
    """`(import '(pkg A B C))` — quoted list with package symbol head."""
    for name in ("ChainMap", "deque"):
        Compiler.current_ns().unmap(Symbol.intern(name))
    E("(clojure.core/import '(collections ChainMap deque))")
    assert E("ChainMap") is collections.ChainMap
    assert E("deque") is collections.deque

def test_import_vector_form():
    """`(import [pkg A B C])` — vector form, no quote needed (the macro
    treats anything non-symbol as a sequential spec)."""
    for name in ("JSONDecoder", "JSONEncoder"):
        Compiler.current_ns().unmap(Symbol.intern(name))
    E("(clojure.core/import [json JSONDecoder JSONEncoder])")
    assert E("JSONDecoder") is json.JSONDecoder
    assert E("JSONEncoder") is json.JSONEncoder

def test_import_mixed_specs():
    """Single-symbol and list specs in one call."""
    for name in ("SameFileError", "Decimal", "Context"):
        Compiler.current_ns().unmap(Symbol.intern(name))
    E("(clojure.core/import 'shutil.SameFileError '(decimal Decimal Context))")
    import shutil
    import decimal
    assert E("SameFileError") is shutil.SameFileError
    assert E("Decimal") is decimal.Decimal
    assert E("Context") is decimal.Context

def test_import_returns_last_imported_class():
    """`do` returns the value of its last form; the last form is the
    last import*, which evaluates to its class."""
    Compiler.current_ns().unmap(Symbol.intern("namedtuple"))
    Compiler.current_ns().unmap(Symbol.intern("OrderedDict"))
    out = E("(clojure.core/import '(collections namedtuple OrderedDict))")
    assert out is collections.OrderedDict

def test_import_deep_dotted_path():
    """Symbol with multiple dots — last segment is the short name."""
    Compiler.current_ns().unmap(Symbol.intern("JSONDecodeError"))
    out = E("(clojure.core/import 'json.decoder.JSONDecodeError)")
    assert out is json.decoder.JSONDecodeError
    assert E("JSONDecodeError") is json.decoder.JSONDecodeError

def test_import_missing_class_raises():
    with pytest.raises((ImportError, ModuleNotFoundError, AttributeError)):
        E("(clojure.core/import 'nonexistent.fake.Class)")


# --- macroexpansion sanity check ----------------------------------

def test_import_expands_to_do_of_import_stars():
    """The macro expands to `(do (clojure.core/import* "...") ...)`."""
    expanded = clojure.core
    # Use the macro Var directly to expand a sample form.
    import_macro = E("(clojure.core/var clojure.core/import)").deref()
    sample = read_string("(clojure.core/import '(collections A B))")
    out = import_macro(sample, None,
                       read_string("'(collections A B)"))
    # Out should be a seq starting with `do`, then two import* calls.
    parts = list(out)
    assert str(parts[0]) == "do"
    assert len(parts) == 3
    # Each call should be (clojure.core/import* "collections.X")
    for call in parts[1:]:
        cl = list(call)
        assert str(cl[0]) == "clojure.core/import*"
        assert isinstance(cl[1], str)
        assert cl[1].startswith("collections.")
