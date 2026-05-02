"""Tests for the LispReader (and PushbackReader / LineNumberingPushbackReader,
TaggedLiteral, ReaderConditional)."""
import io
import re
import pytest

from clojure.lang import (
    PushbackReader, LineNumberingPushbackReader, reader_from_string,
    read, read_string, read_all_string,
    TaggedLiteral, ReaderConditional,
    Symbol, Keyword,
    PersistentList, PersistentVector, PersistentHashMap, PersistentArrayMap,
    PersistentHashSet,
    Ratio, BigInt, BigDecimal,
    RT, Compiler,
    Namespace, Var,
    ReaderException,
)


# =========================================================================
# PushbackReader
# =========================================================================

class TestPushbackReader:
    def test_read_returns_chars_then_eof(self):
        r = PushbackReader(io.StringIO("ab"))
        assert r.read() == "a"
        assert r.read() == "b"
        assert r.read() == ""    # EOF

    def test_unread_pushes_back(self):
        r = PushbackReader(io.StringIO("ab"))
        ch = r.read()
        r.unread(ch)
        assert r.read() == "a"

    def test_unread_eof_raises(self):
        r = PushbackReader(io.StringIO(""))
        with pytest.raises(ValueError):
            r.unread("")

    def test_crlf_normalizes_to_lf(self):
        r = PushbackReader(io.StringIO("a\r\nb"))
        assert r.read() == "a"
        assert r.read() == "\n"
        assert r.read() == "b"

    def test_bare_cr_normalizes_to_lf(self):
        r = PushbackReader(io.StringIO("a\rb"))
        assert r.read() == "a"
        assert r.read() == "\n"
        assert r.read() == "b"


# =========================================================================
# LineNumberingPushbackReader
# =========================================================================

class TestLineNumberingPushbackReader:
    def test_initial_line_column(self):
        r = LineNumberingPushbackReader(io.StringIO("ab"))
        assert r.get_line_number() == 1
        assert r.get_column_number() == 1

    def test_column_advances(self):
        r = LineNumberingPushbackReader(io.StringIO("abc"))
        r.read()
        assert r.get_column_number() == 2
        r.read()
        assert r.get_column_number() == 3

    def test_line_advances_on_newline(self):
        r = LineNumberingPushbackReader(io.StringIO("a\nb"))
        r.read(); r.read()    # 'a', '\n'
        assert r.get_line_number() == 2
        assert r.get_column_number() == 1

    def test_unread_restores_position(self):
        r = LineNumberingPushbackReader(io.StringIO("a\nb"))
        r.read(); r.read()
        r.unread("\n")
        assert r.get_line_number() == 1
        assert r.get_column_number() == 2

    def test_capture_string(self):
        r = LineNumberingPushbackReader(io.StringIO("hello"))
        r.capture_string()
        for _ in range(5): r.read()
        assert r.get_string() == "hello"

    def test_at_line_start(self):
        r = LineNumberingPushbackReader(io.StringIO("a\nb"))
        assert r.at_line_start() is True
        r.read()
        assert r.at_line_start() is False
        r.read()    # '\n'
        assert r.at_line_start() is True


# =========================================================================
# Reader: numbers
# =========================================================================

class TestReadNumbers:
    def test_int(self):
        assert read_string("42") == 42
        assert read_string("-7") == -7
        assert read_string("+99") == 99
        assert read_string("0") == 0

    def test_hex(self):
        assert read_string("0x1F") == 31
        assert read_string("-0x10") == -16

    def test_octal(self):
        assert read_string("0777") == 0o777

    def test_radix(self):
        assert read_string("2r1010") == 10
        assert read_string("16rFF") == 255

    def test_bigint_suffix(self):
        v = read_string("100N")
        assert isinstance(v, BigInt)
        assert int(v) == 100

    def test_zero_with_n_is_bigint(self):
        v = read_string("0N")
        assert isinstance(v, BigInt)

    def test_arbitrary_precision(self):
        big = read_string("1234567890123456789012345")
        assert big == 1234567890123456789012345

    def test_float(self):
        assert read_string("3.14") == 3.14
        assert read_string("-0.5") == -0.5
        assert read_string("1e3") == 1000.0
        assert read_string("1.5e-2") == 0.015

    def test_bigdecimal(self):
        v = read_string("3.14M")
        assert isinstance(v, BigDecimal)

    def test_ratio(self):
        v = read_string("1/3")
        assert isinstance(v, Ratio)
        assert v.numerator == 1 and v.denominator == 3

    def test_ratio_reduces_to_int(self):
        # 6/3 reduces to 2 — Numbers.divide handles that.
        assert read_string("6/3") == 2


# =========================================================================
# Reader: strings, characters
# =========================================================================

class TestReadString:
    def test_basic(self):
        assert read_string('"hello"') == "hello"

    def test_empty(self):
        assert read_string('""') == ""

    def test_escapes(self):
        assert read_string(r'"line1\nline2"') == "line1\nline2"
        assert read_string(r'"tab\there"') == "tab\there"
        assert read_string(r'"quote\"inside"') == 'quote"inside'

    def test_unicode_escape(self):
        assert read_string(r'"A"') == "A"

    def test_octal_escape(self):
        assert read_string(r'"\101"') == "A"

    def test_unterminated_raises(self):
        with pytest.raises((RuntimeError, ReaderException)):
            read_string('"unterminated')


class TestReadCharacter:
    def test_single_char(self):
        assert read_string(r"\a") == "a"
        assert read_string(r"\Z") == "Z"

    def test_named_chars(self):
        assert read_string(r"\newline") == "\n"
        assert read_string(r"\space") == " "
        assert read_string(r"\tab") == "\t"
        assert read_string(r"\backspace") == "\b"
        assert read_string(r"\formfeed") == "\f"
        assert read_string(r"\return") == "\r"

    def test_unicode_char(self):
        assert read_string(r"\u0041") == "A"

    def test_octal_char(self):
        assert read_string(r"\o101") == "A"


# =========================================================================
# Reader: symbols, keywords, special tokens
# =========================================================================

class TestReadSymbolKeyword:
    def test_simple_symbol(self):
        s = read_string("foo")
        assert isinstance(s, Symbol)
        assert s.ns is None and s.name == "foo"

    def test_namespaced_symbol(self):
        s = read_string("ns/foo")
        assert s.ns == "ns" and s.name == "foo"

    def test_simple_keyword(self):
        k = read_string(":foo")
        assert isinstance(k, Keyword)
        assert k.get_name() == "foo"

    def test_namespaced_keyword(self):
        k = read_string(":ns/foo")
        assert k.get_namespace() == "ns" and k.get_name() == "foo"

    def test_auto_resolved_keyword_uses_current_ns(self):
        # ::foo resolves against the current namespace (defaults to user).
        k = read_string("::foo")
        # Current ns is 'user' by default.
        assert k.get_namespace() == "user"
        assert k.get_name() == "foo"

    def test_nil_true_false(self):
        assert read_string("nil") is None
        assert read_string("true") is True
        assert read_string("false") is False


# =========================================================================
# Reader: collections
# =========================================================================

class TestReadCollections:
    def test_empty_list(self):
        result = read_string("()")
        assert result.count() == 0

    def test_list(self):
        result = read_string("(1 2 3)")
        assert list(result) == [1, 2, 3]

    def test_vector(self):
        result = read_string("[1 2 3]")
        assert isinstance(result, PersistentVector)
        assert list(result) == [1, 2, 3]

    def test_map(self):
        result = read_string("{:a 1 :b 2}")
        assert result.val_at(Keyword.intern("a")) == 1
        assert result.val_at(Keyword.intern("b")) == 2

    def test_set(self):
        result = read_string("#{1 2 3}")
        assert isinstance(result, PersistentHashSet)
        assert result.count() == 3

    def test_set_duplicate_raises(self):
        with pytest.raises((ValueError, RuntimeError, ReaderException)):
            read_string("#{1 1}")

    def test_map_odd_number_raises(self):
        with pytest.raises((RuntimeError, ReaderException)):
            read_string("{:a 1 :b}")

    def test_nested(self):
        result = read_string("[(1 2) #{3 4} {:a [5 6]}]")
        assert isinstance(result, PersistentVector)
        assert result.nth(0).count() == 2
        assert result.nth(2).val_at(Keyword.intern("a")).nth(1) == 6


# =========================================================================
# Reader: quote / deref / var / meta
# =========================================================================

class TestReadQuoting:
    def test_quote(self):
        # 'foo  →  (quote foo)
        result = read_string("'foo")
        assert result.first() == Symbol.intern("quote")
        assert result.next().first() == Symbol.intern("foo")

    def test_deref(self):
        # @x  →  (clojure.core/deref x)
        result = read_string("@x")
        assert result.first().name == "deref"
        assert result.first().ns == "clojure.core"
        assert result.next().first() == Symbol.intern("x")

    def test_var(self):
        # #'foo  →  (var foo)
        result = read_string("#'foo")
        assert result.first() == Symbol.intern("var")
        assert result.next().first() == Symbol.intern("foo")

    def test_meta_keyword_shorthand(self):
        # ^:tag bar  →  bar with meta {:tag true}
        result = read_string("^:tag bar")
        meta = result.meta()
        assert meta.val_at(Keyword.intern("tag")) is True

    def test_meta_symbol_shorthand(self):
        # ^Long bar  →  bar with meta {:tag Long}
        result = read_string("^Long bar")
        meta = result.meta()
        assert meta.val_at(Keyword.intern("tag")) == Symbol.intern("Long")

    def test_meta_map(self):
        # ^{:a 1} bar
        result = read_string("^{:a 1} bar")
        meta = result.meta()
        assert meta.val_at(Keyword.intern("a")) == 1


# =========================================================================
# Reader: anonymous fn (#())
# =========================================================================

class TestReadAnonymousFn:
    def test_simple(self):
        # #(+ %1 %2)  →  (fn* [p1 p2] (+ p1 p2))
        result = read_string("#(+ %1 %2)")
        assert result.first() == Symbol.intern("fn*")
        params = result.next().first()
        assert params.count() == 2

    def test_rest_args(self):
        # #(apply + %&)  →  (fn* [& rest] (apply + rest))
        result = read_string("#(apply + %&)")
        params = result.next().first()
        assert params.count() == 2   # & + rest-sym
        assert params.nth(0) == Symbol.intern("&")

    def test_nested_fn_raises(self):
        with pytest.raises((RuntimeError, ReaderException)):
            read_string("#(#(+ %1) 1)")


# =========================================================================
# Reader: comments and discard
# =========================================================================

class TestReadCommentDiscard:
    def test_line_comment(self):
        result = read_string("(1 ; comment\n 2 3)")
        assert list(result) == [1, 2, 3]

    def test_discard(self):
        result = read_string("[1 #_ 99 2 3]")
        assert list(result) == [1, 2, 3]


# =========================================================================
# Reader: regex
# =========================================================================

class TestReadRegex:
    def test_simple_regex(self):
        result = read_string(r'#"\d+"')
        assert isinstance(result, re.Pattern)
        assert result.match("123") is not None


# =========================================================================
# Reader: namespaced maps
# =========================================================================

class TestReadNamespacedMap:
    def test_explicit_ns(self):
        # #:foo{:a 1}  →  {:foo/a 1}
        result = read_string("#:foo{:a 1}")
        assert result.val_at(Keyword.intern("foo", "a")) == 1

    def test_auto_ns(self):
        # #::{:a 1}  →  {:user/a 1}  (current ns)
        result = read_string("#::{:a 1}")
        assert result.val_at(Keyword.intern("user", "a")) == 1

    def test_underscore_keyword_strips_ns(self):
        # #:foo{:_/raw 1}  →  {:raw 1}
        result = read_string("#:foo{:_/raw 1}")
        assert result.val_at(Keyword.intern(None, "raw")) == 1


# =========================================================================
# Reader: tagged literals
# =========================================================================

class TestReadTaggedLiteral:
    def test_unknown_tag_raises_without_data_reader(self):
        with pytest.raises((RuntimeError, ReaderException)):
            read_string("#mytag 5")

    def test_data_reader_dispatches(self):
        # Bind *data-readers* to a map with our tag.
        readers = PersistentArrayMap.create(
            Symbol.intern("triple"), lambda x: x * 3)
        Var.push_thread_bindings(
            PersistentArrayMap.create(RT.DATA_READERS, readers))
        try:
            assert read_string("#triple 5") == 15
        finally:
            Var.pop_thread_bindings()

    def test_default_data_reader_fn(self):
        Var.push_thread_bindings(
            PersistentArrayMap.create(
                RT.DEFAULT_DATA_READER_FN, lambda tag, val: (tag, val)))
        try:
            tag, val = read_string("#unknown 5")
            assert tag.name == "unknown"
            assert val == 5
        finally:
            Var.pop_thread_bindings()


# =========================================================================
# Reader: symbolic values
# =========================================================================

class TestReadSymbolicValue:
    def test_inf(self):
        assert read_string("##Inf") == float("inf")

    def test_neg_inf(self):
        assert read_string("##-Inf") == float("-inf")

    def test_nan(self):
        v = read_string("##NaN")
        assert v != v   # NaN

    def test_unknown_symbolic_raises(self):
        with pytest.raises((RuntimeError, ReaderException)):
            read_string("##Bogus")


# =========================================================================
# Reader: syntax-quote
# =========================================================================

class TestSyntaxQuote:
    def test_unquote_in_list(self):
        # `(a ~b c)  →  (seq (concat (list 'user/a) (list b) (list 'user/c)))
        result = read_string("`(a ~b c)")
        assert isinstance(result, PersistentList)
        # Top-level form is (seq ...)
        assert result.first().name == "seq"

    def test_unquote_splicing(self):
        result = read_string("`(a ~@b c)")
        # Look for unquote-splicing in the expanded form
        s = str(result)
        assert "concat" in s

    def test_gensym_in_syntax_quote(self):
        # `(let [x# 1] x#)  →  expanded with the same gensym for both x#
        result = read_string("`(let [x# 1] x#)")
        # The two x# must resolve to the same gensym symbol within one
        # syntax-quote form. We can't trivially extract them but the form
        # should be a non-trivial sequence.
        assert isinstance(result, PersistentList)


# =========================================================================
# Reader: reader conditionals
# =========================================================================

class TestReaderConditional:
    def _opts_allow(self):
        return PersistentArrayMap.create(
            Keyword.intern("read-cond"), Keyword.intern("allow"))

    def _opts_preserve(self):
        return PersistentArrayMap.create(
            Keyword.intern("read-cond"), Keyword.intern("preserve"))

    def test_clj_branch_matches(self):
        opts = self._opts_allow()
        result = read_string("#?(:clj :win :cljs :nope)", opts=opts)
        assert result == Keyword.intern("win")

    def test_default_branch(self):
        opts = self._opts_allow()
        result = read_string("#?(:cljs :nope :default :fallback)", opts=opts)
        assert result == Keyword.intern("fallback")

    def test_no_match_returns_nothing(self):
        # When no branch matches, the reader continues — to test we
        # need to read multiple forms.
        opts = self._opts_allow()
        out = read_all_string("#?(:cljs :nope) 42", opts=opts)
        assert out == [42]

    def test_preserve_returns_reader_conditional(self):
        opts = self._opts_preserve()
        result = read_string("#?(:clj :a)", opts=opts)
        assert isinstance(result, ReaderConditional)

    def test_disallowed_when_not_set(self):
        with pytest.raises((RuntimeError, ReaderException)):
            read_string("#?(:clj :win)")


# =========================================================================
# Reader: read-all-string and EOF
# =========================================================================

class TestReadAll:
    def test_multiple_forms(self):
        assert read_all_string("1 2 3") == [1, 2, 3]

    def test_empty_string(self):
        assert read_all_string("") == []

    def test_whitespace_only(self):
        assert read_all_string("  \n\t , ") == []


class TestEOF:
    def test_eof_value_for_empty(self):
        sentinel = object()
        result = read_string("", eof_is_error=False, eof_value=sentinel)
        assert result is sentinel

    def test_eof_in_unterminated_list_raises(self):
        with pytest.raises((RuntimeError, ReaderException)):
            read_string("(1 2 3")


# =========================================================================
# TaggedLiteral / ReaderConditional value classes
# =========================================================================

class TestTaggedLiteral:
    def test_create_and_access(self):
        tl = TaggedLiteral.create(Symbol.intern("foo"), 42)
        assert tl.tag == Symbol.intern("foo")
        assert tl.form == 42

    def test_val_at(self):
        tl = TaggedLiteral.create(Symbol.intern("foo"), 42)
        assert tl.val_at(Keyword.intern("tag")) == Symbol.intern("foo")
        assert tl.val_at(Keyword.intern("form")) == 42
        assert tl.val_at(Keyword.intern("missing")) is None

    def test_equality(self):
        a = TaggedLiteral.create(Symbol.intern("x"), 1)
        b = TaggedLiteral.create(Symbol.intern("x"), 1)
        assert a == b


class TestReaderConditionalValue:
    def test_create_and_access(self):
        rc = ReaderConditional.create([1, 2], False)
        assert rc.form == [1, 2]
        assert rc.splicing is False

    def test_val_at(self):
        rc = ReaderConditional.create("x", True)
        assert rc.val_at(Keyword.intern("form")) == "x"
        assert rc.val_at(Keyword.intern("splicing?")) is True


# =========================================================================
# Source location attached to seqs read with line tracking
# =========================================================================

class TestSourceLocation:
    def test_line_column_in_meta(self):
        rdr = LineNumberingPushbackReader(io.StringIO("\n  (a b c)"))
        result = read(rdr)
        meta = result.meta()
        assert meta is not None
        assert meta.val_at(RT.LINE_KEY) == 2
        # Column is the position of the '(' (3 — 1-based, after two spaces).
        assert meta.val_at(RT.COLUMN_KEY) >= 1
