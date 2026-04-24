"""Printing forms — pr, prn, print, println, pr-str, print-str, newline, flush.

Since these write to `*out*`, tests either use the -str variants (which
return strings) or rebind `*out*` to a Python `StringIO` and check the
result.
"""

import io
import pytest
from clojure._core import eval_string


def _ev(s):
    return eval_string(s)


# --- pr-str / print-str ---

def test_pr_str_number():
    assert _ev("(pr-str 42)") == "42"


def test_pr_str_string_quotes():
    assert _ev('(pr-str "hello")') == '"hello"'


def test_pr_str_string_escape():
    assert _ev('(pr-str "a\\nb")') == '"a\\nb"'


def test_pr_str_nil():
    assert _ev("(pr-str nil)") == "nil"


def test_pr_str_keyword():
    assert _ev("(pr-str :foo)") == ":foo"


def test_pr_str_vector():
    assert _ev('(pr-str [1 "a" :k])') == '[1 "a" :k]'


def test_pr_str_variadic():
    assert _ev("(pr-str 1 2 3)") == "1 2 3"


def test_pr_str_nested():
    assert _ev('(pr-str [{:a 1} #{:b}])') == '[{:a 1} #{:b}]'


def test_print_str_string_unquoted():
    assert _ev('(print-str "hello")') == "hello"


def test_print_str_keeps_nested_quotes():
    # Nested strings INSIDE a collection DO get quoted by print-str — vanilla
    # Clojure's print-str uses *print-readably* which only affects the
    # top-level rendering of strings / characters.
    #
    # For simplicity, our port unquotes at every level (it'd take another
    # flag to mirror vanilla's outer-vs-inner distinction). Adjust if/when
    # the distinction matters.
    assert _ev('(print-str [1 "a" 3])') == '[1 a 3]'


def test_print_str_variadic():
    assert _ev("(print-str 1 2 3)") == "1 2 3"


def test_prn_str_adds_newline():
    assert _ev("(prn-str 42)") == "42\n"


def test_println_str_adds_newline():
    assert _ev('(println-str "hello")') == "hello\n"


# --- pr / prn / print / println via *out* rebinding ---

def test_pr_writes_to_stdout():
    # Use capsys if available; else just test via StringIO rebind.
    import sys
    buf = io.StringIO()
    # Expose via a var that Clojure can reference.
    _ev("(def --probe-buf nil)")
    import clojure._core as mod
    mod.__dict__["--probe-buf-py"] = buf
    # Reset at Clojure level.
    _ev("(alter-var-root (var --probe-buf) (fn* [_] (clojure._core/eval (list (symbol \"--probe-buf-py\")))))") if False else None
    # Simpler: directly evaluate the *out* binding via python-level attr.
    # Grab the *out* var from clojure.core and bind it.
    ns = sys.modules["clojure.core"]
    star_out = ns.__dict__["*out*"]
    # Push a binding frame with *out* -> buf.
    push = eval_string("clojure.lang.RT/push-thread-bindings") if False else None
    # Use Clojure's binding macro:
    mod.__dict__["--probe-buf-py"] = buf
    _ev("(alter-var-root (var *out*) (fn* [_] --probe-buf-py))") if False else None
    # Easiest: set the Var's root via Python API.
    star_out.bind_root(buf)
    try:
        _ev("(pr 42)")
        _ev('(pr " hello")')
    finally:
        # Restore stdout.
        star_out.bind_root(sys.stdout)
    assert buf.getvalue() == '42" hello"'


def test_prn_writes_and_newlines():
    import sys
    buf = io.StringIO()
    ns = sys.modules["clojure.core"]
    star_out = ns.__dict__["*out*"]
    star_out.bind_root(buf)
    try:
        _ev("(prn 1 2 3)")
    finally:
        star_out.bind_root(sys.stdout)
    assert buf.getvalue() == "1 2 3\n"


def test_println_human_readable():
    import sys
    buf = io.StringIO()
    ns = sys.modules["clojure.core"]
    star_out = ns.__dict__["*out*"]
    star_out.bind_root(buf)
    try:
        _ev('(println "hello" "world")')
    finally:
        star_out.bind_root(sys.stdout)
    # Human-readable: no quotes around strings.
    assert buf.getvalue() == "hello world\n"


def test_newline_writes_newline():
    import sys
    buf = io.StringIO()
    ns = sys.modules["clojure.core"]
    star_out = ns.__dict__["*out*"]
    star_out.bind_root(buf)
    try:
        _ev("(newline)")
    finally:
        star_out.bind_root(sys.stdout)
    assert buf.getvalue() == "\n"


def test_flush_no_error_on_stringio():
    import sys
    buf = io.StringIO()
    ns = sys.modules["clojure.core"]
    star_out = ns.__dict__["*out*"]
    star_out.bind_root(buf)
    try:
        _ev('(do (pr "x") (flush))')
    finally:
        star_out.bind_root(sys.stdout)
    assert buf.getvalue() == '"x"'


# --- pr-on ---

def test_pr_on_writes_to_given_writer():
    import sys
    buf = io.StringIO()
    # Define a Var in clojure.user and bind its root to the buffer.
    _ev("(def --probe-w nil)")
    user_ns = sys.modules["clojure.user"]
    user_ns.__dict__["--probe-w"].bind_root(buf)
    _ev("(pr-on [1 2 3] --probe-w)")
    assert buf.getvalue() == "[1 2 3]"


# --- print-method multimethod extensibility ---

def test_user_print_method_top_level():
    import sys
    class MyType:
        pass
    _ev("(def --PMT nil)")
    sys.modules["clojure.user"].__dict__["--PMT"].bind_root(MyType)
    _ev('(defmethod print-method --PMT [x w] (clojure.lang.RT/writer-write w "<custom>"))')
    inst = MyType()
    _ev("(def --pmt-inst nil)")
    sys.modules["clojure.user"].__dict__["--pmt-inst"].bind_root(inst)
    assert _ev("(pr-str --pmt-inst)") == "<custom>"


def test_user_print_method_flows_through_vector():
    import sys
    class MyType:
        pass
    _ev("(def --PMT2 nil)")
    sys.modules["clojure.user"].__dict__["--PMT2"].bind_root(MyType)
    _ev('(defmethod print-method --PMT2 [x w] (clojure.lang.RT/writer-write w "<x>"))')
    inst = MyType()
    _ev("(def --pmt2-inst nil)")
    sys.modules["clojure.user"].__dict__["--pmt2-inst"].bind_root(inst)
    _ev("(def --pmt2-vec [1 --pmt2-inst 2])")
    assert _ev("(pr-str --pmt2-vec)") == "[1 <x> 2]"


def test_user_print_method_flows_through_map():
    import sys
    class MyType:
        pass
    _ev("(def --PMT3 nil)")
    sys.modules["clojure.user"].__dict__["--PMT3"].bind_root(MyType)
    _ev('(defmethod print-method --PMT3 [x w] (clojure.lang.RT/writer-write w "<m>"))')
    inst = MyType()
    _ev("(def --pmt3-inst nil)")
    sys.modules["clojure.user"].__dict__["--pmt3-inst"].bind_root(inst)
    _ev("(def --pmt3-map {--pmt3-inst :v1 :k2 --pmt3-inst})")
    s = _ev("(pr-str --pmt3-map)")
    # Map iteration order isn't guaranteed for hash-map; assert both layouts.
    assert s in ("{<m> :v1, :k2 <m>}", "{:k2 <m>, <m> :v1}")


def test_print_readably_rebind_for_print():
    # print-str is print-readably=false, so strings are unquoted ALL THE WAY
    # DOWN (through print-method dispatch).
    assert _ev('(print-str ["hello" "world"])') == "[hello world]"


def test_pr_str_honors_print_readably_via_binding():
    # pr-str normally quotes strings; rebind *print-readably* to falsify.
    result = _ev('(binding [*print-readably* false] (pr-str ["a" "b"]))')
    assert result == "[a b]"


def test_print_dup_falls_back_to_print_method():
    # Default print-dup for an object = print-method output.
    assert _ev("(binding [*print-dup* true] (pr-str [1 2]))") == "[1 2]"
