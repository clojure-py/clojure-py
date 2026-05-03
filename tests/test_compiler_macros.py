"""Compiler tests — macroexpansion.

A Var with :macro true metadata is treated as a macro. When the compiler
sees a call form whose head resolves to such a Var, it invokes the macro
function at compile time with (form, env=None, *args) and recursively
compiles the result. Special-form heads and interop heads (`.method`,
`Class.`) are skipped — they aren't macros even if a same-named Var
exists."""

import pytest

from clojure.lang import (
    Compiler,
    read_string,
    Var, Symbol, Keyword, RT,
    PersistentList, PersistentVector,
)


def _eval(src):
    return Compiler.eval(read_string(src))


def _intern_macro(name, fn):
    """Intern a Var with the given fn and mark it as a macro."""
    ns = Compiler.current_ns()
    v = Var.intern(ns, Symbol.intern(name), fn)
    v.set_macro()
    return v


def _intern_fn(name, fn):
    Var.intern(Compiler.current_ns(), Symbol.intern(name), fn)


_intern_fn("cmac-add", lambda a, b: a + b)
_intern_fn("cmac-mul", lambda a, b: a * b)


def _S(name, ns=None):
    return Symbol.intern(ns, name) if ns else Symbol.intern(name)


def _list(*xs):
    """Build a Clojure PersistentList from Python args."""
    if not xs:
        return PersistentList.EMPTY
    s = None
    for x in reversed(xs):
        s = RT.cons(x, s)
    return s


# --- single-step expansion --------------------------------------------

def test_macroexpand_1_returns_input_for_non_macro():
    form = read_string("(cmac-add 1 2)")
    assert Compiler.macroexpand_1(form) is form

def test_macroexpand_1_returns_input_for_special_form():
    form = read_string("(if true 1 2)")
    assert Compiler.macroexpand_1(form) is form

def test_macroexpand_1_returns_input_for_dot_method():
    form = read_string('(.upper "abc")')
    assert Compiler.macroexpand_1(form) is form

def test_macroexpand_1_expands_macro():
    """Define `(my-id x)` that expands to `x`."""
    _intern_macro("cmac-id", lambda form, env, x: x)
    expanded = Compiler.macroexpand_1(read_string("(cmac-id 42)"))
    assert expanded == 42

def test_macro_receives_full_form_as_first_arg():
    captured = []
    def mac(form, env, x):
        captured.append((form, env))
        return x
    _intern_macro("cmac-cap", mac)
    Compiler.eval(read_string("(cmac-cap 99)"))
    form, env = captured[0]
    assert form == read_string("(cmac-cap 99)")
    assert env is None


# --- recursive expansion -----------------------------------------------

def test_macroexpand_iterates_until_stable():
    """A macro that expands to a call to another macro."""
    _intern_macro("cmac-inner", lambda form, env, x: _list(_S("if"), x, 1, 2))
    _intern_macro("cmac-outer", lambda form, env, x:
                  _list(_S("cmac-inner"), x))
    expanded = Compiler.macroexpand(read_string("(cmac-outer true)"))
    assert expanded == read_string("(if true 1 2)")


# --- typical macros eval correctly ------------------------------------

def test_when_macro():
    """Build a `when` macro: (when test body...) → (if test (do body...))"""
    def when(form, env, test, *body):
        return _list(_S("if"), test,
                     RT.cons(_S("do"), _list(*body)))
    _intern_macro("cmac-when", when)
    assert _eval("(cmac-when true 1 2 3)") == 3
    assert _eval("(cmac-when false 1 2 3)") is None
    assert _eval("(cmac-when true (cmac-add 10 32))") == 42

def test_unless_macro():
    """`(unless test body...)` → `(if test nil (do body...))`."""
    def unless(form, env, test, *body):
        return _list(_S("if"), test, None,
                     RT.cons(_S("do"), _list(*body)))
    _intern_macro("cmac-unless", unless)
    assert _eval("(cmac-unless false 99)") == 99
    assert _eval("(cmac-unless true 99)") is None

def test_or_macro_two_clauses():
    """`(or a b)` → `(let* [t a] (if t t b))`."""
    def or2(form, env, a, b):
        t = _S("__or_t__")
        return _list(_S("let*"), PersistentVector.create(t, a),
                     _list(_S("if"), t, t, b))
    _intern_macro("cmac-or2", or2)
    assert _eval("(cmac-or2 false 99)") == 99
    assert _eval("(cmac-or2 42 99)") == 42
    assert _eval("(cmac-or2 nil :default)") == Keyword.intern(None, "default")


# --- defn-like macro: (defn name args body) -> (def name (fn name args body))

def test_defn_like_macro():
    def defn(form, env, name, args, *body):
        return _list(
            _S("def"), name,
            RT.cons(_S("fn*"), RT.cons(name, RT.cons(args, _list(*body)))),
        )
    _intern_macro("cmac-defn", defn)
    _eval("(cmac-defn cmac-square [x] (cmac-mul x x))")
    assert _eval("(cmac-square 7)") == 49


# --- threading (->) macro ---------------------------------------------

def test_thread_first_macro():
    """`(-> x (f a) (g b))` → `(g (f x a) b)`."""
    def thread_first(form, env, x, *forms):
        result = x
        for f in forms:
            if isinstance(f, type(read_string("(a)"))):
                fs = f.seq()
                head = fs.first()
                rest_args = fs.next()
                # Insert `result` as the first arg
                new_args = RT.cons(result, rest_args)
                result = RT.cons(head, new_args)
            else:
                result = _list(f, result)
        return result
    _intern_macro("cmac->", thread_first)
    assert _eval("(cmac-> 5 (cmac-add 10) (cmac-mul 2))") == 30


# --- macros that expand to calls to themselves -------------------------

def test_recursive_macro_expansion_terminates_when_form_unchanged():
    """A macro that returns its input unchanged should NOT loop forever."""
    _intern_macro("cmac-iden", lambda form, env, x: form)
    # macroexpand should terminate (returns same form)
    f = read_string("(cmac-iden 42)")
    assert Compiler.macroexpand(f) is f


# --- qualified macro name ----------------------------------------------

def test_qualified_macro_reference():
    """`foo/bar` resolves through the current ns mapping/alias."""
    ns = Compiler.current_ns()
    _intern_macro("cmac-q", lambda form, env, x: x)
    qname = ns.name.name + "/cmac-q"
    assert _eval("(" + qname + " 13)") == 13


# --- compile-time vs run-time scope -----------------------------------

def test_macro_can_close_over_python_state():
    """The macro fn is a regular Python fn — it can capture mutable
    Python state at definition time."""
    counter = [0]
    def m(form, env):
        counter[0] += 1
        return counter[0]
    _intern_macro("cmac-bump", m)
    assert _eval("(cmac-bump)") == 1
    assert _eval("(cmac-bump)") == 2
    assert _eval("(cmac-bump)") == 3


# --- works inside other forms -----------------------------------------

def test_macro_inside_let_body():
    _intern_macro("cmac-id2", lambda form, env, x: x)
    assert _eval("(let* [x 5] (cmac-id2 x))") == 5

def test_macro_inside_fn_body():
    _intern_macro("cmac-id3", lambda form, env, x: x)
    f = _eval("(fn* [a] (cmac-id3 a))")
    assert f(99) == 99

def test_macro_inside_if_branch():
    _intern_macro("cmac-id4", lambda form, env, x: x)
    assert _eval("(if true (cmac-id4 7) 0)") == 7
