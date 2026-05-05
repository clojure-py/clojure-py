"""Tests for the core_print.clj port (JVM helper file loaded from core.clj
right after the case macro).

Adaptations covered:
  - Number method handles Inf / -Inf / NaN (JVM's Double-specific impl).
  - py.re/Pattern method uses .-pattern (Python attr) instead of .pattern
    (JVM method call).
  - Throwable's structured #error map is reduced to a one-liner
    (#error "Type: msg") since we haven't ported StackTraceElement /
    Throwable->map yet.
  - IRecord, TaggedLiteral, ReaderConditional, PrintWriter-on, java.util
    collection prefer-method calls — all skipped, the deps don't exist.

Compiler-side additions:
  - .getName / .getSimpleName / .isArray / .charAt added to
    JAVA_METHOD_FALLBACKS.
  - System.identityHashCode added to host_shims.System (returns id(o)).
"""

import io
import re

import pytest

import clojure.core  # bootstrap

from clojure.lang import (
    Compiler,
    read_string,
    Keyword, Symbol, Namespace, Var, PersistentArrayMap,
)


def E(src):
    return Compiler.eval(read_string(src))


def K(name):
    return Keyword.intern(None, name)


def P(src):
    """pr `src` to a string, return the result."""
    return E("(clojure.core/with-out-str (clojure.core/pr " + src + "))")


# --- scalars ------------------------------------------------------

def test_pr_nil():
    assert P("nil") == "nil"

def test_pr_true_false():
    assert P("true") == "true"
    assert P("false") == "false"

def test_pr_integer():
    assert P("42") == "42"
    assert P("-7") == "-7"
    assert P("0") == "0"

def test_pr_float():
    assert P("1.5") == "1.5"
    assert P("-3.14") == "-3.14"

def test_pr_inf_nan():
    assert P("(/ 1.0 0.0)") == "##Inf"
    assert P("(/ -1.0 0.0)") == "##-Inf"
    assert P("(- (/ 0.0 0.0))") == "##NaN"

def test_pr_keyword():
    assert P(":foo") == ":foo"
    assert P(":a/b") == ":a/b"

def test_pr_symbol():
    assert P("(quote foo)") == "foo"
    assert P("(quote my.ns/bar)") == "my.ns/bar"


# --- strings -----------------------------------------------------

def test_pr_string_basic():
    assert P('"hello"') == '"hello"'

def test_pr_string_escape_newline():
    assert P('"a\nb"') == '"a\\nb"'

def test_pr_string_escape_tab():
    assert P('"x\ty"') == '"x\\ty"'

def test_pr_string_escape_quote():
    """A literal double-quote in the input string must render as \\\" so
    the output round-trips through the reader."""
    out = P('"a\\"b"')
    assert out == '"a\\"b"'

def test_pr_string_escape_backslash():
    out = P('"x\\\\y"')   # source: "x\\y"
    assert out == '"x\\\\y"'

def test_pr_empty_string():
    assert P('""') == '""'


# --- collections -------------------------------------------------

def test_pr_vector():
    assert P("[1 2 3]") == "[1 2 3]"
    assert P("[]") == "[]"

def test_pr_list():
    assert P("(quote (1 2 3))") == "(1 2 3)"
    assert P("(quote ())") == "()"

def test_pr_map():
    out = P("{:a 1}")
    assert out == "{:a 1}"

def test_pr_map_multi():
    out = P("(sorted-map :a 1 :b 2)")
    assert out == "{:a 1, :b 2}"

def test_pr_set():
    """Sets aren't ordered — accept either {1 2 3} permutation."""
    out = P("#{1 2 3}")
    assert out.startswith("#{")
    assert out.endswith("}")
    inner = out[2:-1].split(" ")
    assert sorted(inner) == ["1", "2", "3"]

def test_pr_nested():
    out = P("[1 [2 3] {:k :v}]")
    assert out == "[1 [2 3] {:k :v}]"

def test_pr_lazy_seq():
    """Lazy seqs print like lists."""
    assert P("(map inc [1 2 3])") == "(2 3 4)"

def test_pr_range():
    assert P("(range 5)") == "(0 1 2 3 4)"


# --- regex / class -----------------------------------------------

def test_pr_regex():
    assert P('#"abc"') == '#"abc"'

def test_pr_regex_with_special_chars():
    """Regex pattern preserved verbatim."""
    out = P('#"\\d+"')
    # Reader produces compiled Pattern; print-method emits #"<pattern>"
    assert out.startswith('#"')
    assert out.endswith('"')

def test_pr_class():
    """A class prints as its qualified name."""
    out = P("py.__builtins__/int")
    # int.__name__ == 'int'
    assert out == "int"


# --- *print-readably* off ----------------------------------------

def test_pr_string_when_print_readably_false():
    """When *print-readably* is false, strings print without quotes/escapes."""
    out = E("""
      (binding [clojure.core/*print-readably* false]
        (clojure.core/with-out-str (clojure.core/pr "hi")))""")
    assert out == "hi"


# --- *print-length* limit ---------------------------------------

def test_pr_print_length_truncates():
    out = E("""
      (binding [clojure.core/*print-length* 3]
        (clojure.core/with-out-str (clojure.core/pr [1 2 3 4 5])))""")
    assert out == "[1 2 3 ...]"

def test_pr_print_length_zero():
    out = E("""
      (binding [clojure.core/*print-length* 0]
        (clojure.core/with-out-str (clojure.core/pr [1 2 3])))""")
    assert out == "[...]"


# --- *print-level* limit ----------------------------------------

def test_pr_print_level_truncates():
    """*print-level* limits collection nesting depth. print-sequential
    decrements the level on entry; when it goes negative the collection
    prints as `#`. With *print-level* 1 the outer vector is allowed but
    nested vectors inside it become `#`."""
    out = E("""
      (binding [clojure.core/*print-level* 1]
        (clojure.core/with-out-str (clojure.core/pr [1 [2 [3]]])))""")
    assert out == "[1 #]"

def test_pr_print_level_zero():
    """*print-level* 0 ⇒ the outermost collection itself prints as `#`."""
    out = E("""
      (binding [clojure.core/*print-level* 0]
        (clojure.core/with-out-str (clojure.core/pr [1 2 3])))""")
    assert out == "#"

def test_pr_print_level_two():
    """*print-level* 2 lets us see one level of nesting inside the outer."""
    out = E("""
      (binding [clojure.core/*print-level* 2]
        (clojure.core/with-out-str (clojure.core/pr [1 [2 [3]]])))""")
    assert out == "[1 [2 #]]"


# --- char-escape-string and char-name-string --------------------

def test_char_escape_string_var():
    """The lookup map exists and maps newline/tab/etc."""
    m = E("clojure.core/char-escape-string")
    # Map keyed by single-char strings (Python: char == str of length 1)
    # so direct lookup with a 1-char string works.
    assert m["\n"] == "\\n"
    assert m["\t"] == "\\t"
    assert m['"'] == '\\"'
    assert m["\\"] == "\\\\"

def test_char_name_string_var():
    m = E("clojure.core/char-name-string")
    assert m["\n"] == "newline"
    assert m["\t"] == "tab"
    assert m[" "] == "space"


# --- prn / println -----------------------------------------------

def test_prn_appends_newline():
    out = E("(clojure.core/with-out-str (clojure.core/prn 42))")
    assert out == "42\n"

def test_println_no_quotes():
    """println uses pr with *print-readably* false."""
    out = E('(clojure.core/with-out-str (clojure.core/println "hi"))')
    assert out == "hi\n"


# --- print-method dispatch via :type meta -----------------------

def test_print_method_type_meta_dispatch():
    """Attaching :type meta routes dispatch to a keyword-keyed method."""
    E("""
      (defmethod clojure.core/print-method :tcb35-tagged
        [x w]
        (.write w "<tagged>"))""")
    out = E("""
      (clojure.core/with-out-str
        (clojure.core/pr (with-meta [1 2] {:type :tcb35-tagged})))""")
    assert out == "<tagged>"


# --- print-initialized signal -----------------------------------

def test_print_initialized_var_set():
    assert E("clojure.core/print-initialized") is True


# --- *print-namespace-maps* -------------------------------------

def test_print_namespace_maps_lifts():
    """When *print-namespace-maps* true and all keys share a namespace,
    the map prints with #:ns{} prefix."""
    out = E("""
      (binding [clojure.core/*print-namespace-maps* true]
        (clojure.core/with-out-str
          (clojure.core/pr (sorted-map :ns/a 1 :ns/b 2))))""")
    assert out == "#:ns{:a 1, :b 2}"

def test_print_namespace_maps_no_lift_when_mixed():
    """Mixed namespaces ⇒ no lift, print as plain map."""
    out = E("""
      (binding [clojure.core/*print-namespace-maps* true]
        (clojure.core/with-out-str
          (clojure.core/pr (sorted-map :ns1/a 1 :ns2/b 2))))""")
    assert out == "{:ns1/a 1, :ns2/b 2}"

def test_print_namespace_maps_default_off():
    """Without binding, namespaced keys print verbatim."""
    out = E('(clojure.core/with-out-str (clojure.core/pr (sorted-map :n/a 1 :n/b 2)))')
    assert out == "{:n/a 1, :n/b 2}"


# --- minimal Throwable handling ---------------------------------

def test_pr_throwable():
    """Our Throwable printer is a minimal one-liner — just the class
    name and message inside #error \"...\"."""
    out = E("""
      (try (throw (ex-info "boom" {}))
           (catch Throwable e
             (clojure.core/with-out-str (clojure.core/pr e))))""")
    assert "#error" in out
    assert "boom" in out
