"""Tests for core.clj batch 19 (lines 3848-3922): subvec, with-open,
doto, memfn, time.

with-open is verified to work with Python file-likes (open(),
io.StringIO) just as well as our BufferedReader and
LineNumberingPushbackReader — the macro emits (.close name) and
anything with a Python .close() method satisfies that.

Backend additions:
  System.nanoTime           — wraps time.monotonic_ns for time macro.
  System.currentTimeMillis  — wraps time.time*1000 (added for symmetry).

Bug fix worth calling out:
  The clojure.lang shim classes had __name__ values like '_System'
  because they were defined as `class _System:` inside a closure.
  syntax-quote's resolve_symbol uses cls.__module__ + "." + cls.__name__
  to qualify a class symbol; that produced
  "clojure.core.<locals>._System" — unreachable at runtime. Each shim
  now has __module__/__name__/__qualname__ set so the FQN is
  "clojure.lang.System" etc., which RT.class_for_name resolves back.
  Symptom was that (System/nanoTime) inside macros emitted via
  syntax-quote (like in `time`) would fail to compile.
"""

import io
import os
import tempfile

import pytest

import clojure.core  # triggers load

from clojure.lang import (
    Compiler,
    read_string,
    Var, Symbol, Namespace,
    PersistentArrayMap,
    PersistentVector,
    BufferedReader,
    RT,
)


def E(src):
    return Compiler.eval(read_string(src))


# --- subvec --------------------------------------------------------

def test_subvec_two_arg_uses_count_as_end():
    out = E("(clojure.core/subvec [10 20 30 40 50] 2)")
    assert isinstance(out, PersistentVector)
    assert list(out) == [30, 40, 50]

def test_subvec_three_arg():
    out = E("(clojure.core/subvec [10 20 30 40 50] 1 4)")
    assert list(out) == [20, 30, 40]

def test_subvec_full_range_returns_self():
    """JVM optimization: full-range subvec returns the input vec."""
    src = E("[1 2 3]")
    out = RT.subvec(src, 0, 3)
    assert out is src

def test_subvec_empty_range():
    assert list(E("(clojure.core/subvec [1 2 3] 1 1)")) == []

def test_subvec_out_of_range_raises():
    with pytest.raises(IndexError):
        E("(clojure.core/subvec [1 2 3] 0 10)")
    with pytest.raises(IndexError):
        E("(clojure.core/subvec [1 2 3] 5)")


# --- doto ----------------------------------------------------------

def test_doto_calls_methods_in_order():
    """doto evaluates x, then calls each form with x as first arg."""
    class Bag:
        def __init__(self):
            self.items = []
        def add(self, x):
            self.items.append(x)
            return self
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb19-make-bag"), Bag)
    out = E("(clojure.core/doto (user/tcb19-make-bag) (.add 1) (.add 2) (.add 3))")
    assert out.items == [1, 2, 3]

def test_doto_returns_x_not_last_form_value():
    """doto returns x — even if the last call returns something else."""
    class Bag:
        def __init__(self):
            self.items = []
        def add(self, x):
            self.items.append(x)
            return "ignored-return"
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb19-bag2"), Bag)
    out = E("(clojure.core/doto (user/tcb19-bag2) (.add :a))")
    assert isinstance(out, Bag)
    assert out.items == [":ignored"] or out.items == [E(":a")]

def test_doto_with_no_forms_just_returns_x():
    out = E("(clojure.core/doto 42)")
    assert out == 42

def test_doto_evaluates_x_only_once():
    """Critical: x is bound once via gensym — side effects shouldn't repeat."""
    counter = [0]
    def make():
        counter[0] += 1
        class O:
            x = "ok"
            def setx(self, v):
                self.x = v
        return O()
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb19-once"), make)
    E("(clojure.core/doto (user/tcb19-once) (.setx 1) (.setx 2))")
    assert counter[0] == 1


# --- memfn ---------------------------------------------------------

def test_memfn_zero_arg():
    upper = E("(clojure.core/memfn upper)")
    class H:
        def upper(self):
            return "UP"
    assert upper(H()) == "UP"

def test_memfn_with_args():
    getter = E("(clojure.core/memfn get k)")
    class M:
        def get(self, k):
            return f"got-{k}"
    assert getter(M(), "x") == "got-x"

def test_memfn_passes_through_to_method():
    """memfn-produced fn really invokes the named method."""
    upper = E("(clojure.core/memfn upper)")
    # Python's str doesn't have an `upper` attr that takes self — but
    # str.upper IS a bound method, so memfn would call s.upper().
    assert upper("hello") == "HELLO"


# --- time ----------------------------------------------------------

def _capture_out(*forms):
    core_ns = Namespace.find(Symbol.intern("clojure.core"))
    out_var = core_ns.find_interned_var(Symbol.intern("*out*"))
    buf = io.StringIO()
    Var.push_thread_bindings(PersistentArrayMap.create(out_var, buf))
    try:
        results = [E(f) for f in forms]
    finally:
        Var.pop_thread_bindings()
    return results, buf.getvalue()

def test_time_returns_expr_value():
    results, _ = _capture_out("(clojure.core/time (clojure.core/+ 1 2 3))")
    assert results[0] == 6

def test_time_prints_elapsed():
    _, output = _capture_out("(clojure.core/time 42)")
    assert "Elapsed time:" in output
    assert "msecs" in output

def test_time_with_side_effecting_expr():
    counter = []
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb19-touch!"),
               lambda: (counter.append(1), 99)[1])
    results, _ = _capture_out("(clojure.core/time (user/tcb19-touch!))")
    assert results[0] == 99
    assert counter == [1]


# --- with-open: standard cases ------------------------------------

def test_with_open_closes_resource():
    """The macro emits (.close name) on every binding."""
    closed = [False]
    class Resource:
        def close(self):
            closed[0] = True
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb19-make-r"), Resource)
    E("(clojure.core/with-open [r (user/tcb19-make-r)] :body)")
    assert closed[0] is True

def test_with_open_closes_on_exception():
    closed = [False]
    class Resource:
        def close(self):
            closed[0] = True
        def boom(self):
            raise RuntimeError("oops")
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb19-boom-r"), Resource)
    with pytest.raises(RuntimeError, match="oops"):
        E("(clojure.core/with-open [r (user/tcb19-boom-r)] (.boom r))")
    assert closed[0] is True

def test_with_open_returns_body_value():
    class Resource:
        def close(self): pass
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb19-quiet-r"), Resource)
    out = E("(clojure.core/with-open [r (user/tcb19-quiet-r)] 42)")
    assert out == 42

def test_with_open_multiple_bindings_close_in_reverse():
    """JVM doc: 'a finally clause that calls (.close name) on each name
    in reverse order.' Verify by checking close ordering."""
    order = []
    class Resource:
        def __init__(self, label):
            self.label = label
        def close(self):
            order.append(self.label)
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb19-r"), Resource)
    E("""(clojure.core/with-open
            [a (user/tcb19-r "A")
             b (user/tcb19-r "B")
             c (user/tcb19-r "C")]
            :body)""")
    assert order == ["C", "B", "A"]

def test_with_open_empty_bindings_just_runs_body():
    out = E("(clojure.core/with-open [] (clojure.core/+ 1 2))")
    assert out == 3

def test_with_open_assert_args_non_vector():
    with pytest.raises(Exception, match="vector for its binding"):
        E("(clojure.core/with-open (a 1) :body)")

def test_with_open_assert_args_odd_bindings():
    with pytest.raises(Exception, match="even number of forms"):
        E("(clojure.core/with-open [a 1 b] :body)")

def test_with_open_rejects_non_symbol_binding_name():
    with pytest.raises(Exception, match="only allows Symbols"):
        E("(clojure.core/with-open [[a b] (clojure.core/range 2)] :body)")


# --- with-open with Python sources --------------------------------

def test_with_open_works_with_python_open():
    """User's specific concern: with-open should work with Python's
    open(path) — file objects have .close()."""
    with tempfile.NamedTemporaryFile(mode="w", suffix=".txt",
                                     delete=False) as f:
        f.write("line one\nline two\n")
        tmppath = f.name
    try:
        Var.intern(Compiler.current_ns(), Symbol.intern("tcb19-tmppath"), tmppath)
        Var.intern(Compiler.current_ns(), Symbol.intern("tcb19-py-open"), open)
        out = E('(clojure.core/with-open [f (user/tcb19-py-open user/tcb19-tmppath "r")] (.read f))')
        assert out == "line one\nline two\n"
    finally:
        os.unlink(tmppath)

def test_with_open_python_file_actually_closed():
    """The file object's .closed attribute should be True after with-open."""
    with tempfile.NamedTemporaryFile(mode="w", suffix=".txt",
                                     delete=False) as f:
        f.write("data")
        tmppath = f.name
    try:
        Var.intern(Compiler.current_ns(), Symbol.intern("tcb19-cap-tmp"), tmppath)
        Var.intern(Compiler.current_ns(), Symbol.intern("tcb19-cap-open"), open)
        # Capture the file in a Python list so we can check it post-with-open.
        captured = []
        def opener(path, mode):
            f = open(path, mode)
            captured.append(f)
            return f
        Var.intern(Compiler.current_ns(), Symbol.intern("tcb19-spy-open"), opener)
        E('(clojure.core/with-open [f (user/tcb19-spy-open user/tcb19-cap-tmp "r")] (.read f))')
        assert captured[0].closed is True
    finally:
        os.unlink(tmppath)

def test_with_open_works_with_io_stringio():
    """io.StringIO is closeable — verify."""
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb19-make-sio"),
               lambda: io.StringIO("hello"))
    out = E("(clojure.core/with-open [s (user/tcb19-make-sio)] (.read s))")
    assert out == "hello"

def test_with_open_works_with_buffered_reader():
    """Our own BufferedReader shim — its read_line is exercised here."""
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb19-make-br"),
               lambda: BufferedReader(io.StringIO("first\nsecond\n")))
    out = E("(clojure.core/with-open [r (user/tcb19-make-br)] (.read_line r))")
    assert out == "first"
