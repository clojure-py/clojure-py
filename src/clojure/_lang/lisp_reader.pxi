# Port of clojure.lang.LispReader (and its companion classes
# TaggedLiteral and ReaderConditional).
#
# Faithful 1:1 port — every reader macro, every dispatch macro, every
# corner case (number radix, octal escapes, namespaced maps, reader
# conditionals, syntax-quote, anonymous fn literals, eval-on-read,
# tagged literals, record literals).
#
# Java's RT.* and Compiler.* dependencies are routed through our minimal
# RT and Compiler stubs in runtime_support.pxi.


import re as _re


# --- TaggedLiteral / ReaderConditional ----------------------------------

cdef object _KW_TAG_TL = Keyword.intern(None, "tag")
cdef object _KW_FORM_TL = Keyword.intern(None, "form")
cdef object _KW_SPLICING_RC = Keyword.intern(None, "splicing?")


cdef class TaggedLiteral:
    """Returned by the reader when *suppress-read* is true and a tagged
    literal (#foo bar) is encountered. Mirrors clojure.lang.TaggedLiteral."""

    cdef readonly Symbol tag
    cdef readonly object form

    def __cinit__(self, Symbol tag, form):
        self.tag = tag
        self.form = form

    @staticmethod
    def create(tag, form):
        return TaggedLiteral(tag, form)

    def val_at(self, key, not_found=NOT_FOUND):
        if Util.equiv(_KW_FORM_TL, key):
            return self.form
        if Util.equiv(_KW_TAG_TL, key):
            return self.tag
        return None if not_found is NOT_FOUND else not_found

    def __eq__(self, other):
        if not isinstance(other, TaggedLiteral):
            return False
        return Util.equals(self.tag, (<TaggedLiteral>other).tag) and \
               Util.equals(self.form, (<TaggedLiteral>other).form)

    def __ne__(self, other):
        return not self.__eq__(other)

    def __hash__(self):
        cdef int32_t h = 0
        h = _to_int32_mask(0 if self.tag is None else hash(self.tag))
        h = _to_int32_mask(31 * h + (0 if self.form is None else hash(self.form)))
        return h

    def __repr__(self):
        return f"#{self.tag} {self.form!r}"


ILookup.register(TaggedLiteral)


cdef class ReaderConditional:
    """Returned when *read-cond* is :preserve and a reader conditional is
    encountered. Mirrors clojure.lang.ReaderConditional."""

    cdef readonly object form
    cdef readonly bint splicing

    def __cinit__(self, form, bint splicing):
        self.form = form
        self.splicing = splicing

    @staticmethod
    def create(form, splicing):
        return ReaderConditional(form, bool(splicing))

    def val_at(self, key, not_found=NOT_FOUND):
        if Util.equiv(_KW_FORM_TL, key):
            return self.form
        if Util.equiv(_KW_SPLICING_RC, key):
            return self.splicing
        return None if not_found is NOT_FOUND else not_found

    def __eq__(self, other):
        if not isinstance(other, ReaderConditional):
            return False
        return Util.equals(self.form, (<ReaderConditional>other).form) and \
               self.splicing == (<ReaderConditional>other).splicing

    def __ne__(self, other):
        return not self.__eq__(other)

    def __hash__(self):
        cdef int32_t h = 0
        h = _to_int32_mask(0 if self.form is None else hash(self.form))
        h = _to_int32_mask(31 * h + (1 if self.splicing else 0))
        return h


ILookup.register(ReaderConditional)


# --- LispReader ---------------------------------------------------------

# Symbol constants used by the reader.
cdef object _LR_QUOTE = Symbol.intern("quote")
cdef object _LR_THE_VAR = Symbol.intern("var")
cdef object _LR_UNQUOTE = Symbol.intern("clojure.core", "unquote")
cdef object _LR_UNQUOTE_SPLICING = Symbol.intern("clojure.core", "unquote-splicing")
cdef object _LR_CONCAT = Symbol.intern("clojure.core", "concat")
cdef object _LR_SEQ = Symbol.intern("clojure.core", "seq")
cdef object _LR_LIST = Symbol.intern("clojure.core", "list")
cdef object _LR_APPLY = Symbol.intern("clojure.core", "apply")
cdef object _LR_HASHMAP = Symbol.intern("clojure.core", "hash-map")
cdef object _LR_HASHSET = Symbol.intern("clojure.core", "hash-set")
cdef object _LR_VECTOR = Symbol.intern("clojure.core", "vector")
cdef object _LR_WITH_META = Symbol.intern("clojure.core", "with-meta")
cdef object _LR_DEREF = Symbol.intern("clojure.core", "deref")

cdef object _LR_UNKNOWN = Keyword.intern(None, "unknown")

# Reader opts
cdef object _LR_OPT_EOF = Keyword.intern(None, "eof")
cdef object _LR_OPT_FEATURES = Keyword.intern(None, "features")
cdef object _LR_OPT_READ_COND = Keyword.intern(None, "read-cond")
cdef object _LR_EOFTHROW = Keyword.intern(None, "eofthrow")
cdef object _LR_PLATFORM_KEY = Keyword.intern(None, "clj")
cdef object _LR_COND_ALLOW = Keyword.intern(None, "allow")
cdef object _LR_COND_PRESERVE = Keyword.intern(None, "preserve")
cdef object _LR_DEFAULT_FEATURE = Keyword.intern(None, "default")

cdef object _LR_PLATFORM_FEATURES = PersistentHashSet.create(_LR_PLATFORM_KEY)
cdef object _LR_RESERVED_FEATURES = PersistentHashSet.create(
    Keyword.intern(None, "else"), Keyword.intern(None, "none"))

# Sentinels for delimited-list reading.
cdef object _LR_READ_EOF = object()
cdef object _LR_READ_FINISHED = object()

# Sentinel that "no-op" macros return so the read loop knows to keep going.
cdef object _LR_READER_SENTINEL = object()


# Regex patterns matching Java's exactly.
_INT_PAT = _re.compile(
    r"([-+]?)(?:(0)|([1-9][0-9]*)|0[xX]([0-9A-Fa-f]+)|0([0-7]+)|"
    r"([1-9][0-9]?)[rR]([0-9A-Za-z]+)|0[0-9]+)(N)?")
_RATIO_PAT = _re.compile(r"([-+]?[0-9]+)/([0-9]+)")
_FLOAT_PAT = _re.compile(r"([-+]?[0-9]+(\.[0-9]*)?([eE][-+]?[0-9]+)?)(M)?")
_ARG_PAT = _re.compile(r"%(?:(&)|([1-9][0-9]*))?")
_SYMBOL_PAT = _re.compile(r"[:]?([\D&&[^/]].*/)?(/|[\D&&[^/]][^/]*)"
                          .replace(r"[\D&&[^/]]", r"[^\d/]"))
# Note: Python re doesn't support character-class intersection (`&&`),
# so we expand it manually: `\D && [^/]` == `[^\d/]`.
_SYMBOL_PAT = _re.compile(r"[:]?([^\d/].*/)?(/|[^\d/][^/]*)")
_ARRAY_SYMBOL_PAT = _re.compile(r"([^\d/:].*)/([1-9])")


cdef object _gensym_env_var = None
cdef object _arg_env_var = None
cdef object _read_cond_env_var = None


def _init_lisp_reader_vars():
    global _gensym_env_var, _arg_env_var, _read_cond_env_var
    _gensym_env_var = Var.create(None).set_dynamic()
    _arg_env_var = Var.create(None).set_dynamic()
    _read_cond_env_var = Var.create(None).set_dynamic()


_init_lisp_reader_vars()


# --- low-level char ops -------------------------------------------------

cdef bint _is_whitespace(str ch) noexcept:
    if ch == "" or ch is None:
        return False
    return ch.isspace() or ch == ","


cdef object _read1(reader):
    """Read one char from the reader, returning '' for EOF."""
    return reader.read()


cdef void _unread(reader, ch):
    if ch != "" and ch is not None:
        reader.unread(ch)


# --- macro / dispatch tables (filled in below) --------------------------

cdef dict _MACROS = {}
cdef dict _DISPATCH_MACROS = {}


cdef bint _is_macro(str ch) noexcept:
    return ch in _MACROS


cdef bint _is_terminating_macro(str ch) noexcept:
    if ch in ("#", "'", "%"):
        return False
    return ch in _MACROS


cdef object _get_macro(str ch):
    return _MACROS.get(ch)


# --- ReaderException ----------------------------------------------------

class ReaderException(Exception):
    """Wraps a reader error with line/column info from a
    LineNumberingPushbackReader."""

    def __init__(self, line, column, cause):
        self.line = line
        self.column = column
        self.cause = cause
        self.data = PersistentArrayMap.create(
            Keyword.intern("clojure.error", "line"), line,
            Keyword.intern("clojure.error", "column"), column)
        super().__init__(f"ReaderException at {line}:{column}: {cause}")


# --- top-level read entry points ----------------------------------------

def read(reader, eof_is_error=True, eof_value=None, is_recursive=False, opts=None):
    """Read one form from `reader`. Public entry point."""
    if opts is None:
        opts = _PHM_EMPTY
    elif not isinstance(opts, IPersistentMap):
        # Allow passing a dict.
        opts = PersistentArrayMap.create(*[
            x for kv in opts.items() for x in kv])
    return _read(reader, eof_is_error, eof_value, None, None,
                 is_recursive, opts, [], _resolver_or_none())


def read_string(s, eof_is_error=True, eof_value=None, opts=None):
    """Convenience: read a single form from a string."""
    return read(reader_from_string(s),
                eof_is_error=eof_is_error,
                eof_value=eof_value,
                opts=opts)


def read_all_string(s, opts=None):
    """Read all forms from `s` and return a list."""
    rdr = reader_from_string(s)
    out = []
    sentinel = object()
    while True:
        form = read(rdr, eof_is_error=False, eof_value=sentinel, opts=opts)
        if form is sentinel:
            return out
        out.append(form)


cdef object _resolver_or_none():
    if RT.READER_RESOLVER is None:
        return None
    return RT.READER_RESOLVER.deref()


cdef object _ensure_pending(pending):
    return [] if pending is None else pending


cdef object _install_platform_feature(opts):
    if opts is None or opts is _PHM_EMPTY:
        return PersistentArrayMap.create(_LR_OPT_FEATURES, _LR_PLATFORM_FEATURES)
    features = opts.val_at(_LR_OPT_FEATURES)
    if features is None:
        return opts.assoc(_LR_OPT_FEATURES, _LR_PLATFORM_FEATURES)
    return opts.assoc(_LR_OPT_FEATURES, features.cons(_LR_PLATFORM_KEY))


cdef object _read(reader, bint eof_is_error, object eof_value,
                  object return_on, object return_on_value,
                  bint is_recursive, object opts, list pending_forms,
                  object resolver):
    """Core read loop. Most of the work happens inside the for(;;) ."""
    if RT.READEVAL.deref() is _LR_UNKNOWN:
        raise RuntimeError("Reading disallowed - *read-eval* bound to :unknown")

    opts = _install_platform_feature(opts)

    try:
        while True:
            if pending_forms:
                return pending_forms.pop(0)

            ch = _read1(reader)
            while _is_whitespace(ch):
                ch = _read1(reader)

            if ch == "":
                if eof_is_error:
                    raise RuntimeError("EOF while reading")
                return eof_value

            if return_on is not None and ch == return_on:
                return return_on_value

            if ch.isdigit():
                return _read_number(reader, ch)

            macro_fn = _get_macro(ch)
            if macro_fn is not None:
                ret = macro_fn(reader, ch, opts, pending_forms)
                if ret is reader or ret is _LR_READER_SENTINEL:
                    continue
                return ret

            if ch == "+" or ch == "-":
                ch2 = _read1(reader)
                if ch2 != "" and ch2.isdigit():
                    _unread(reader, ch2)
                    return _read_number(reader, ch)
                _unread(reader, ch2)

            token = _read_token(reader, ch)
            return _interpret_token(token, resolver)
    except ReaderException:
        raise
    except Exception as e:
        if is_recursive or not isinstance(reader, LineNumberingPushbackReader):
            raise
        rdr = <LineNumberingPushbackReader>reader
        raise ReaderException(rdr.get_line_number(), rdr.get_column_number(), e) from e


cdef str _read_token(reader, str initch):
    cdef list buf = [initch]
    while True:
        ch = _read1(reader)
        if ch == "" or _is_whitespace(ch) or _is_terminating_macro(ch):
            _unread(reader, ch)
            return "".join(buf)
        buf.append(ch)


cdef object _read_number(reader, str initch):
    cdef list buf = [initch]
    while True:
        ch = _read1(reader)
        if ch == "" or _is_whitespace(ch) or _is_macro(ch):
            _unread(reader, ch)
            break
        buf.append(ch)
    s = "".join(buf)
    n = _match_number(s)
    if n is None:
        raise ValueError(f"Invalid number: {s}")
    return n


cdef int _read_unicode_char_token(str token, int offset, int length, int base) except -1:
    if len(token) != offset + length:
        raise ValueError(f"Invalid unicode character: \\{token}")
    cdef int uc = 0
    cdef int i, d
    for i in range(offset, offset + length):
        ch = token[i]
        try:
            d = int(ch, base)
        except ValueError:
            raise ValueError(f"Invalid digit: {ch}")
        uc = uc * base + d
    return uc


cdef int _read_unicode_char_stream(reader, str initch, int base, int length, bint exact) except -1:
    cdef int uc, d, i
    try:
        uc = int(initch, base)
    except ValueError:
        raise ValueError(f"Invalid digit: {initch}")
    i = 1
    while i < length:
        ch = _read1(reader)
        if ch == "" or _is_whitespace(ch) or _is_macro(ch):
            _unread(reader, ch)
            break
        try:
            d = int(ch, base)
        except ValueError:
            raise ValueError(f"Invalid digit: {ch}")
        uc = uc * base + d
        i += 1
    if i != length and exact:
        raise ValueError(f"Invalid character length: {i}, should be: {length}")
    return uc


cdef object _interpret_token(str s, resolver):
    if s == "nil":
        return None
    if s == "true":
        return True
    if s == "false":
        return False
    ret = _match_symbol(s, resolver)
    if ret is not None:
        return ret
    raise ValueError(f"Invalid token: {s}")


cdef object _match_symbol(str s, resolver):
    m = _SYMBOL_PAT.fullmatch(s)
    if m is not None:
        ns = m.group(1)
        name = m.group(2)
        # Reject malformed cases.
        if ((ns is not None and ns.endswith(":/"))
                or name.endswith(":")
                or s.find("::", 1) != -1):
            return None
        if s.startswith("::"):
            ks = Symbol.intern(s[2:])
            if resolver is not None:
                if ks.ns is not None:
                    nsym = resolver.resolveAlias(Symbol.intern(ks.ns))
                else:
                    nsym = resolver.currentNS()
                if nsym is not None:
                    return Keyword.intern(nsym.name, ks.name)
                return None
            cur_ns = Compiler.current_ns()
            if ks.ns is not None:
                kns = cur_ns.lookup_alias(Symbol.intern(ks.ns))
            else:
                kns = cur_ns
            if kns is not None:
                return Keyword.intern(kns.name.name, ks.name)
            return None
        is_keyword = s[0] == ":"
        sym = Symbol.intern(s[1:] if is_keyword else s)
        if is_keyword:
            return Keyword.intern(sym)
        return sym
    am = _ARRAY_SYMBOL_PAT.fullmatch(s)
    if am is not None:
        return Symbol.intern(am.group(1), am.group(2))
    return None


cdef object _match_number(str s):
    m = _INT_PAT.fullmatch(s)
    if m is not None:
        if m.group(2) is not None:
            # Plain "0" — possibly with N suffix → BigInt.
            return BigInt(0) if m.group(8) is not None else 0
        negate = m.group(1) == "-"
        n = None
        radix = 10
        if m.group(3) is not None:
            n = m.group(3); radix = 10
        elif m.group(4) is not None:
            n = m.group(4); radix = 16
        elif m.group(5) is not None:
            n = m.group(5); radix = 8
        elif m.group(7) is not None:
            n = m.group(7); radix = int(m.group(6))
        if n is None:
            return None
        bn = int(n, radix)
        if negate:
            bn = -bn
        if m.group(8) is not None:    # N suffix → BigInt
            return BigInt(bn)
        return bn   # Python int handles bignum naturally
    m = _FLOAT_PAT.fullmatch(s)
    if m is not None:
        if m.group(4) is not None:    # M suffix → BigDecimal
            from decimal import Decimal as _Dec
            return BigDecimal(m.group(1))
        return float(s)
    m = _RATIO_PAT.fullmatch(s)
    if m is not None:
        numerator = m.group(1)
        if numerator.startswith("+"):
            numerator = numerator[1:]
        return _ratio_or_int(int(numerator), int(m.group(2)))
    return None


cdef object _ratio_or_int(num, den):
    """Like Numbers.divide for two ints — returns int if divisible, Ratio
    otherwise, sign-normalized."""
    return Numbers.divide(num, den)


cdef list _read_delimited_list(str delim, reader, bint is_recursive,
                               object opts, list pending_forms):
    cdef int firstline = -1
    if isinstance(reader, LineNumberingPushbackReader):
        firstline = (<LineNumberingPushbackReader>reader).get_line_number()

    out = []
    resolver = _resolver_or_none()
    while True:
        form = _read(reader, False, _LR_READ_EOF, delim, _LR_READ_FINISHED,
                     is_recursive, opts, pending_forms, resolver)
        if form is _LR_READ_EOF:
            if firstline < 0:
                raise RuntimeError("EOF while reading")
            raise RuntimeError(f"EOF while reading, starting at line {firstline}")
        if form is _LR_READ_FINISHED:
            return out
        out.append(form)


# --- Reader macro implementations ---------------------------------------

cdef class _StringReader(AFn):
    def __call__(self, reader, doublequote, opts, pending_forms):
        cdef list buf = []
        ch = _read1(reader)
        while ch != '"':
            if ch == "":
                raise RuntimeError("EOF while reading string")
            if ch == "\\":
                ch = _read1(reader)
                if ch == "":
                    raise RuntimeError("EOF while reading string")
                if ch == "t":   ch = "\t"
                elif ch == "r": ch = "\r"
                elif ch == "n": ch = "\n"
                elif ch == "\\": pass
                elif ch == '"': pass
                elif ch == "b": ch = "\b"
                elif ch == "f": ch = "\f"
                elif ch == "u":
                    ch = _read1(reader)
                    try:
                        int(ch, 16)
                    except (ValueError, TypeError):
                        raise RuntimeError(f"Invalid unicode escape: \\u{ch}")
                    ch = chr(_read_unicode_char_stream(reader, ch, 16, 4, True))
                elif ch.isdigit():
                    code = _read_unicode_char_stream(reader, ch, 8, 3, False)
                    if code > 0o377:
                        raise RuntimeError("Octal escape sequence must be in range [0, 377].")
                    ch = chr(code)
                else:
                    raise RuntimeError(f"Unsupported escape character: \\{ch}")
            buf.append(ch)
            ch = _read1(reader)
        return "".join(buf)


cdef class _RegexReader(AFn):
    def __call__(self, reader, doublequote, opts, pending_forms):
        cdef list buf = []
        ch = _read1(reader)
        while ch != '"':
            if ch == "":
                raise RuntimeError("EOF while reading regex")
            buf.append(ch)
            if ch == "\\":
                ch = _read1(reader)
                if ch == "":
                    raise RuntimeError("EOF while reading regex")
                buf.append(ch)
            ch = _read1(reader)
        return _re.compile("".join(buf))


cdef class _CommentReader(AFn):
    def __call__(self, reader, semi, opts, pending_forms):
        ch = _read1(reader)
        while ch != "" and ch != "\n":
            ch = _read1(reader)
        return reader     # signal "keep reading"


cdef class _DiscardReader(AFn):
    def __call__(self, reader, underscore, opts, pending_forms):
        _read(reader, True, None, None, None, True, opts,
              _ensure_pending(pending_forms), _resolver_or_none())
        return reader


cdef class _WrappingReader(AFn):
    cdef object _sym
    def __init__(self, sym):
        self._sym = sym
    def __call__(self, reader, ch, opts, pending_forms):
        o = _read(reader, True, None, None, None, True, opts,
                  _ensure_pending(pending_forms), _resolver_or_none())
        return RT.list(self._sym, o)


cdef class _VarReader(AFn):
    def __call__(self, reader, quote, opts, pending_forms):
        o = _read(reader, True, None, None, None, True, opts,
                  _ensure_pending(pending_forms), _resolver_or_none())
        return RT.list(_LR_THE_VAR, o)


cdef class _MetaReader(AFn):
    def __call__(self, reader, caret, opts, pending_forms):
        cdef int line = -1
        cdef int column = -1
        if isinstance(reader, LineNumberingPushbackReader):
            line = (<LineNumberingPushbackReader>reader).get_line_number()
            column = (<LineNumberingPushbackReader>reader).get_column_number() - 1
        pending_forms = _ensure_pending(pending_forms)
        meta = _read(reader, True, None, None, None, True, opts, pending_forms, _resolver_or_none())
        if isinstance(meta, Symbol) or isinstance(meta, str):
            meta = RT.map(RT.TAG_KEY, meta)
        elif isinstance(meta, Keyword):
            meta = RT.map(meta, True)
        elif isinstance(meta, IPersistentVector):
            meta = RT.map(RT.PARAM_TAGS_KEY, meta)
        elif not isinstance(meta, IPersistentMap):
            raise ValueError("Metadata must be Symbol, Keyword, String, Vector or Map")

        o = _read(reader, True, None, None, None, True, opts, pending_forms, _resolver_or_none())
        if isinstance(o, IMeta):
            if line != -1 and isinstance(o, ISeq):
                meta = RT.assoc(meta, RT.LINE_KEY, RT.get(meta, RT.LINE_KEY, line))
                meta = RT.assoc(meta, RT.COLUMN_KEY, RT.get(meta, RT.COLUMN_KEY, column))
            if isinstance(o, IReference):
                o.reset_meta(meta)
                return o
            existing_meta = RT.meta(o)
            ometa = existing_meta if existing_meta is not None else _PHM_EMPTY
            s = meta.seq() if meta is not None else None
            while s is not None:
                kv = s.first()
                ometa = RT.assoc(ometa, kv.key(), kv.val())
                s = s.next()
            return o.with_meta(ometa)
        raise ValueError("Metadata can only be applied to IMetas")


cdef class _ListReader(AFn):
    def __call__(self, reader, lparen, opts, pending_forms):
        cdef int line = -1
        cdef int column = -1
        if isinstance(reader, LineNumberingPushbackReader):
            line = (<LineNumberingPushbackReader>reader).get_line_number()
            column = (<LineNumberingPushbackReader>reader).get_column_number() - 1
        items = _read_delimited_list(")", reader, True, opts, _ensure_pending(pending_forms))
        if not items:
            return _empty_list
        s = PersistentList.create(items)
        if line != -1:
            existing = RT.meta(s) if RT.meta(s) is not None else _PHM_EMPTY
            existing = RT.assoc(existing, RT.LINE_KEY, RT.get(existing, RT.LINE_KEY, line))
            existing = RT.assoc(existing, RT.COLUMN_KEY, RT.get(existing, RT.COLUMN_KEY, column))
            return s.with_meta(existing)
        return s


cdef class _VectorReader(AFn):
    def __call__(self, reader, lbracket, opts, pending_forms):
        items = _read_delimited_list("]", reader, True, opts, _ensure_pending(pending_forms))
        return PersistentVector.from_iterable(items)


cdef class _MapReader(AFn):
    def __call__(self, reader, lbrace, opts, pending_forms):
        items = _read_delimited_list("}", reader, True, opts, _ensure_pending(pending_forms))
        if len(items) % 2 == 1:
            raise RuntimeError("Map literal must contain an even number of forms")
        return RT.map(*items)


cdef class _SetReader(AFn):
    def __call__(self, reader, lbrace, opts, pending_forms):
        items = _read_delimited_list("}", reader, True, opts, _ensure_pending(pending_forms))
        return PersistentHashSet.create_with_check(*items)


cdef class _UnmatchedDelimiterReader(AFn):
    def __call__(self, reader, rdelim, opts, pending_forms):
        raise RuntimeError(f"Unmatched delimiter: {rdelim}")


cdef class _UnreadableReader(AFn):
    def __call__(self, reader, langle, opts, pending_forms):
        raise RuntimeError("Unreadable form")


cdef class _CharacterReader(AFn):
    def __call__(self, reader, backslash, opts, pending_forms):
        ch = _read1(reader)
        if ch == "":
            raise RuntimeError("EOF while reading character")
        token = _read_token(reader, ch)
        if len(token) == 1:
            return token[0]
        if token == "newline":   return "\n"
        if token == "space":     return " "
        if token == "tab":       return "\t"
        if token == "backspace": return "\b"
        if token == "formfeed":  return "\f"
        if token == "return":    return "\r"
        if token.startswith("u"):
            c = chr(_read_unicode_char_token(token, 1, 4, 16))
            if 0xD800 <= ord(c) <= 0xDFFF:
                raise RuntimeError(
                    f"Invalid character constant: \\u{format(ord(c), 'x')}")
            return c
        if token.startswith("o"):
            length = len(token) - 1
            if length > 3:
                raise RuntimeError(f"Invalid octal escape sequence length: {length}")
            uc = _read_unicode_char_token(token, 1, length, 8)
            if uc > 0o377:
                raise RuntimeError("Octal escape sequence must be in range [0, 377].")
            return chr(uc)
        raise RuntimeError(f"Unsupported character: \\{token}")


# --- Anonymous-fn (#()) reader ------------------------------------------

cdef object _garg(int n):
    name = ("rest" if n == -1 else f"p{n}") + f"__{RT.next_id()}#"
    return Symbol.intern(None, name)


cdef object _register_arg(int n):
    argsyms = _arg_env_var.deref()
    if argsyms is None:
        raise RuntimeError("arg literal not in #()")
    ret = argsyms.val_at(n)
    if ret is None:
        ret = _garg(n)
        _arg_env_var.set(argsyms.assoc(n, ret))
    return ret


cdef class _ArgReader(AFn):
    def __call__(self, reader, pct, opts, pending_forms):
        token = _read_token(reader, "%")
        if _arg_env_var.deref() is None:
            return _interpret_token(token, None)
        m = _ARG_PAT.fullmatch(token)
        if m is None:
            raise RuntimeError("arg literal must be %, %& or %integer")
        if m.group(1) is not None:    # %&
            return _register_arg(-1)
        n = 1 if m.group(2) is None else int(m.group(2))
        return _register_arg(n)


cdef class _FnReader(AFn):
    def __call__(self, reader, lparen, opts, pending_forms):
        if _arg_env_var.deref() is not None:
            raise RuntimeError("Nested #()s are not allowed")
        Var.push_thread_bindings(
            PersistentArrayMap.create(_arg_env_var, _PTM_EMPTY))
        try:
            _unread(reader, "(")
            form = _read(reader, True, None, None, None, True, opts,
                         _ensure_pending(pending_forms), _resolver_or_none())
            args = _PV_EMPTY
            argsyms = _arg_env_var.deref()
            rargs = argsyms.rseq()
            if rargs is not None:
                higharg = rargs.first().key()
                if higharg > 0:
                    for i in range(1, higharg + 1):
                        sym = argsyms.val_at(i)
                        if sym is None:
                            sym = _garg(i)
                        args = args.cons(sym)
                restsym = argsyms.val_at(-1)
                if restsym is not None:
                    args = args.cons(Compiler._AMP_)
                    args = args.cons(restsym)
            return RT.list(Compiler.FN, args, form)
        finally:
            Var.pop_thread_bindings()


# --- Syntax-quote -------------------------------------------------------

cdef bint _is_unquote(form):
    return isinstance(form, ISeq) and Util.equals(RT.first(form), _LR_UNQUOTE)


cdef bint _is_unquote_splicing(form):
    return isinstance(form, ISeq) and Util.equals(RT.first(form), _LR_UNQUOTE_SPLICING)


cdef object _flatten_map(form):
    keyvals = _PV_EMPTY
    s = RT.seq(form)
    while s is not None:
        e = s.first()
        keyvals = keyvals.cons(e.key())
        keyvals = keyvals.cons(e.val())
        s = s.next()
    return keyvals


cdef object _sq_expand_list(seq):
    ret = _PV_EMPTY
    s = seq
    while s is not None:
        item = s.first()
        if _is_unquote(item):
            ret = ret.cons(RT.list(_LR_LIST, RT.second(item)))
        elif _is_unquote_splicing(item):
            ret = ret.cons(RT.second(item))
        else:
            ret = ret.cons(RT.list(_LR_LIST, _syntax_quote(item)))
        s = s.next()
    return ret.seq()


cdef object _syntax_quote(form):
    cdef object ret
    if Compiler.is_special(form):
        ret = RT.list(Compiler.QUOTE, form)
    elif isinstance(form, Symbol):
        sym = form
        resolver = _resolver_or_none()
        if sym.ns is None and sym.name.endswith("#"):
            gmap = _gensym_env_var.deref()
            if gmap is None:
                raise RuntimeError("Gensym literal not in syntax-quote")
            gs = gmap.val_at(sym)
            if gs is None:
                gs = Symbol.intern(None,
                    sym.name[:-1] + f"__{RT.next_id()}__auto__")
                _gensym_env_var.set(gmap.assoc(sym, gs))
            sym = gs
        elif sym.ns is None and sym.name.endswith("."):
            csym = Symbol.intern(None, sym.name[:-1])
            if resolver is not None:
                rc = resolver.resolveClass(csym)
                if rc is not None:
                    csym = rc
            else:
                csym = Compiler.resolve_symbol(csym)
            sym = Symbol.intern(None, csym.name + ".")
        elif sym.ns is None and sym.name.startswith("."):
            pass    # leave method names alone
        elif resolver is not None:
            nsym = None
            if sym.ns is not None:
                alias = Symbol.intern(None, sym.ns)
                nsym = resolver.resolveClass(alias)
                if nsym is None:
                    nsym = resolver.resolveAlias(alias)
            if nsym is not None:
                sym = Symbol.intern(nsym.name, sym.name)
            elif sym.ns is None:
                rsym = resolver.resolveClass(sym)
                if rsym is None:
                    rsym = resolver.resolveVar(sym)
                if rsym is not None:
                    sym = rsym
                else:
                    sym = Symbol.intern(resolver.currentNS().name, sym.name)
        else:
            maybe_class = None
            if sym.ns is not None:
                maybe_class = Compiler.current_ns().get_mapping(Symbol.intern(None, sym.ns))
            if isinstance(maybe_class, type):
                sym = Symbol.intern(maybe_class.__module__ + "." + maybe_class.__name__, sym.name)
            else:
                sym = Compiler.resolve_symbol(sym)
        ret = RT.list(Compiler.QUOTE, sym)
    elif _is_unquote(form):
        return RT.second(form)
    elif _is_unquote_splicing(form):
        raise RuntimeError("splice not in list")
    elif isinstance(form, IPersistentCollection):
        if isinstance(form, IRecord):
            ret = form
        elif isinstance(form, IPersistentMap):
            keyvals = _flatten_map(form)
            ret = RT.list(_LR_APPLY, _LR_HASHMAP,
                RT.list(_LR_SEQ, RT.cons(_LR_CONCAT, _sq_expand_list(keyvals.seq()))))
        elif isinstance(form, IPersistentVector):
            ret = RT.list(_LR_APPLY, _LR_VECTOR,
                RT.list(_LR_SEQ, RT.cons(_LR_CONCAT, _sq_expand_list(form.seq()))))
        elif isinstance(form, IPersistentSet):
            ret = RT.list(_LR_APPLY, _LR_HASHSET,
                RT.list(_LR_SEQ, RT.cons(_LR_CONCAT, _sq_expand_list(form.seq()))))
        elif isinstance(form, (ISeq, IPersistentList)):
            seq = RT.seq(form)
            if seq is None:
                ret = RT.cons(_LR_LIST, None)
            else:
                ret = RT.list(_LR_SEQ, RT.cons(_LR_CONCAT, _sq_expand_list(seq)))
        else:
            raise NotImplementedError("Unknown Collection type")
    elif isinstance(form, (Keyword, int, float, str, BigInt, BigDecimal, Ratio)):
        ret = form
    else:
        ret = RT.list(Compiler.QUOTE, form)

    # Wrap with metadata if present and non-empty (sans line/col).
    if isinstance(form, IObj) and RT.meta(form) is not None:
        new_meta = form.meta().without(RT.LINE_KEY).without(RT.COLUMN_KEY)
        if new_meta.count() > 0:
            return RT.list(_LR_WITH_META, ret, _syntax_quote(form.meta()))
    return ret


cdef class _SyntaxQuoteReader(AFn):
    def __call__(self, reader, backquote, opts, pending_forms):
        Var.push_thread_bindings(
            PersistentArrayMap.create(_gensym_env_var, _PHM_EMPTY))
        try:
            form = _read(reader, True, None, None, None, True, opts,
                         _ensure_pending(pending_forms), _resolver_or_none())
            return _syntax_quote(form)
        finally:
            Var.pop_thread_bindings()


cdef class _UnquoteReader(AFn):
    def __call__(self, reader, comma, opts, pending_forms):
        ch = _read1(reader)
        if ch == "":
            raise RuntimeError("EOF while reading character")
        pending_forms = _ensure_pending(pending_forms)
        if ch == "@":
            o = _read(reader, True, None, None, None, True, opts, pending_forms, _resolver_or_none())
            return RT.list(_LR_UNQUOTE_SPLICING, o)
        _unread(reader, ch)
        o = _read(reader, True, None, None, None, True, opts, pending_forms, _resolver_or_none())
        return RT.list(_LR_UNQUOTE, o)


# --- Symbolic value reader (##Inf, ##-Inf, ##NaN) -----------------------

cdef object _SYMBOLIC_VALUES = PersistentHashMap.create(
    Symbol.intern("Inf"), float("inf"),
    Symbol.intern("-Inf"), float("-inf"),
    Symbol.intern("NaN"), float("nan"))


cdef class _SymbolicValueReader(AFn):
    def __call__(self, reader, quote, opts, pending_forms):
        o = _read(reader, True, None, None, None, True, opts,
                  _ensure_pending(pending_forms), _resolver_or_none())
        if not isinstance(o, Symbol):
            raise RuntimeError(f"Invalid token: ##{o}")
        if not _SYMBOLIC_VALUES.contains_key(o):
            raise RuntimeError(f"Unknown symbolic value: ##{o}")
        return _SYMBOLIC_VALUES.val_at(o)


# --- Eval reader (#=) ---------------------------------------------------

cdef class _EvalReader(AFn):
    def __call__(self, reader, eq, opts, pending_forms):
        if not RT.boolean_cast(RT.READEVAL.deref()):
            raise RuntimeError("EvalReader not allowed when *read-eval* is false.")
        o = _read(reader, True, None, None, None, True, opts,
                  _ensure_pending(pending_forms), _resolver_or_none())
        if isinstance(o, Symbol):
            return RT.class_for_name(str(o))
        if isinstance(o, IPersistentList):
            fs = RT.first(o)
            if Util.equals(fs, _LR_THE_VAR):
                vs = RT.second(o)
                return RT.var(vs.ns, vs.name)
            if fs.name.endswith("."):
                args = RT.to_array(RT.next(o))
                return Reflector.invoke_constructor(
                    RT.class_for_name(fs.name[:-1]), args)
            if Compiler.names_static_member(fs):
                args = RT.to_array(RT.next(o))
                return Reflector.invoke_static_method(fs.ns, fs.name, args)
            v = Compiler.maybe_resolve_in(Compiler.current_ns(), fs)
            if isinstance(v, Var):
                return v.apply_to(RT.next(o))
            raise RuntimeError(f"Can't resolve {fs}")
        raise ValueError("Unsupported #= form")


# --- Namespace map reader (#:foo{...} / #::{...}) ----------------------

cdef class _NamespaceMapReader(AFn):
    def __call__(self, reader, colon, opts, pending_forms):
        cdef bint auto = False
        auto_char = _read1(reader)
        if auto_char == ":":
            auto = True
        else:
            _unread(reader, auto_char)

        sym = None
        next_char = _read1(reader)
        if _is_whitespace(next_char):
            if auto:
                while _is_whitespace(next_char):
                    next_char = _read1(reader)
                if next_char != "{":
                    _unread(reader, next_char)
                    raise RuntimeError("Namespaced map must specify a namespace")
            else:
                _unread(reader, next_char)
                raise RuntimeError("Namespaced map must specify a namespace")
        elif next_char != "{":
            _unread(reader, next_char)
            sym = _read(reader, True, None, None, None, False, opts, pending_forms, _resolver_or_none())
            next_char = _read1(reader)
            while _is_whitespace(next_char):
                next_char = _read1(reader)
        if next_char != "{":
            raise RuntimeError("Namespaced map must specify a map")

        # Resolve the namespace key.
        if auto:
            resolver = _resolver_or_none()
            if sym is None:
                if resolver is not None:
                    ns = resolver.currentNS().name
                else:
                    ns = Compiler.current_ns().get_name().name
            elif not isinstance(sym, Symbol) or sym.ns is not None:
                raise RuntimeError(f"Namespaced map must specify a valid namespace: {sym}")
            else:
                if resolver is not None:
                    resolved_ns = resolver.resolveAlias(sym)
                else:
                    rns = Compiler.current_ns().lookup_alias(sym)
                    resolved_ns = None if rns is None else rns.name
                if resolved_ns is None:
                    raise RuntimeError(f"Unknown auto-resolved namespace alias: {sym}")
                ns = resolved_ns.name
        elif not isinstance(sym, Symbol) or sym.ns is not None:
            raise RuntimeError(f"Namespaced map must specify a valid namespace: {sym}")
        else:
            ns = sym.name

        kvs = _read_delimited_list("}", reader, True, opts, _ensure_pending(pending_forms))
        if len(kvs) % 2 == 1:
            raise RuntimeError("Namespaced map literal must contain an even number of forms")

        out = []
        for i in range(0, len(kvs), 2):
            key = kvs[i]
            val = kvs[i + 1]
            if isinstance(key, Keyword):
                if key.get_namespace() is None:
                    key = Keyword.intern(ns, key.get_name())
                elif key.get_namespace() == "_":
                    key = Keyword.intern(None, key.get_name())
            elif isinstance(key, Symbol):
                if key.ns is None:
                    key = Symbol.intern(ns, key.name)
                elif key.ns == "_":
                    key = Symbol.intern(None, key.name)
            out.append(key)
            out.append(val)
        return RT.map(*out)


# --- Reader conditionals (#?(...) and #?@(...)) -------------------------

cdef bint _is_preserve_read_cond(opts):
    if not RT.boolean_cast(_read_cond_env_var.deref()):
        return False
    if not isinstance(opts, IPersistentMap):
        return False
    return Util.equals(_LR_COND_PRESERVE, opts.val_at(_LR_OPT_READ_COND))


cdef bint _has_feature(feature, opts):
    if not isinstance(feature, Keyword):
        raise RuntimeError(f"Feature should be a keyword: {feature}")
    if Util.equals(_LR_DEFAULT_FEATURE, feature):
        return True
    custom = opts.val_at(_LR_OPT_FEATURES)
    return custom is not None and custom.contains(feature)


cdef object _READ_STARTED = object()


cdef object _read_cond_delimited(reader, bint splicing, opts, list pending_forms):
    cdef object result = _READ_STARTED
    cdef bint toplevel = (pending_forms is None)
    pending_forms = _ensure_pending(pending_forms)
    cdef int firstline = -1
    if isinstance(reader, LineNumberingPushbackReader):
        firstline = (<LineNumberingPushbackReader>reader).get_line_number()

    while True:
        if result is _READ_STARTED:
            form = _read(reader, False, _LR_READ_EOF, ")", _LR_READ_FINISHED,
                         True, opts, pending_forms, None)
            if form is _LR_READ_EOF:
                if firstline < 0:
                    raise RuntimeError("EOF while reading")
                raise RuntimeError(f"EOF while reading, starting at line {firstline}")
            if form is _LR_READ_FINISHED:
                break
            if _LR_RESERVED_FEATURES.contains(form):
                raise RuntimeError(f"Feature name {form} is reserved.")
            if _has_feature(form, opts):
                form = _read(reader, False, _LR_READ_EOF, ")", _LR_READ_FINISHED,
                             True, opts, pending_forms, _resolver_or_none())
                if form is _LR_READ_EOF:
                    if firstline < 0:
                        raise RuntimeError("EOF while reading")
                    raise RuntimeError(f"EOF while reading, starting at line {firstline}")
                if form is _LR_READ_FINISHED:
                    if firstline < 0:
                        raise RuntimeError("read-cond requires an even number of forms.")
                    raise RuntimeError(f"read-cond starting on line {firstline} requires an even number of forms")
                result = form

        # Discard the next form.
        Var.push_thread_bindings(PersistentArrayMap.create(RT.SUPPRESS_READ, True))
        try:
            form = _read(reader, False, _LR_READ_EOF, ")", _LR_READ_FINISHED,
                         True, opts, pending_forms, _resolver_or_none())
        finally:
            Var.pop_thread_bindings()
        if form is _LR_READ_EOF:
            if firstline < 0:
                raise RuntimeError("EOF while reading")
            raise RuntimeError(f"EOF while reading, starting at line {firstline}")
        if form is _LR_READ_FINISHED:
            break

    if result is _READ_STARTED:
        return reader

    if splicing:
        if not isinstance(result, (list, tuple, IPersistentVector, IPersistentList, ISeq)):
            raise RuntimeError("Spliced form list in read-cond-splicing must be a sequence")
        if toplevel:
            raise RuntimeError("Reader conditional splicing not allowed at the top level.")
        # Materialize and prepend to pending.
        items = list(result) if not isinstance(result, IPersistentVector) else [result.nth(i) for i in range(result.count())]
        pending_forms[0:0] = items
        return reader
    return result


cdef class _ConditionalReader(AFn):
    def __call__(self, reader, mode, opts, pending_forms):
        if not (opts is not None and (
                Util.equals(_LR_COND_ALLOW, opts.val_at(_LR_OPT_READ_COND))
                or Util.equals(_LR_COND_PRESERVE, opts.val_at(_LR_OPT_READ_COND)))):
            raise RuntimeError("Conditional read not allowed")

        ch = _read1(reader)
        if ch == "":
            raise RuntimeError("EOF while reading character")
        cdef bint splicing = False
        if ch == "@":
            splicing = True
            ch = _read1(reader)
        while _is_whitespace(ch):
            ch = _read1(reader)
        if ch == "":
            raise RuntimeError("EOF while reading character")
        if ch != "(":
            raise RuntimeError("read-cond body must be a list")

        Var.push_thread_bindings(PersistentArrayMap.create(_read_cond_env_var, True))
        try:
            if _is_preserve_read_cond(opts):
                list_reader = _get_macro(ch)
                form = list_reader(reader, ch, opts, _ensure_pending(pending_forms))
                return ReaderConditional.create(form, splicing)
            return _read_cond_delimited(reader, splicing, opts, pending_forms)
        finally:
            Var.pop_thread_bindings()


# --- Constructor / record reader (#foo[...] or #foo{...}) --------------

cdef class _CtorReader(AFn):
    def __call__(self, reader, first_char, opts, pending_forms):
        pending_forms = _ensure_pending(pending_forms)
        name = _read(reader, True, None, None, None, False, opts, pending_forms, _resolver_or_none())
        if not isinstance(name, Symbol):
            raise RuntimeError("Reader tag must be a symbol")
        sym = name
        form = _read(reader, True, None, None, None, True, opts, pending_forms, _resolver_or_none())

        if _is_preserve_read_cond(opts) or RT.suppress_read():
            return TaggedLiteral.create(sym, form)
        if "." in sym.name:
            return self._read_record(form, sym, opts, pending_forms)
        return self._read_tagged(form, sym, opts, pending_forms)

    cdef object _read_tagged(self, o, Symbol tag, opts, pending_forms):
        data_readers = RT.DATA_READERS.deref()
        data_reader = RT.get(data_readers, tag)
        if data_reader is None:
            data_readers = RT.DEFAULT_DATA_READERS.deref()
            data_reader = RT.get(data_readers, tag)
            if data_reader is None:
                default_reader = RT.DEFAULT_DATA_READER_FN.deref()
                if default_reader is not None:
                    return default_reader(tag, o)
                raise RuntimeError(f"No reader function for tag {tag}")
        return data_reader(o)

    cdef object _read_record(self, form, Symbol record_name, opts, pending_forms):
        if not RT.boolean_cast(RT.READEVAL.deref()):
            raise RuntimeError(
                "Record construction syntax can only be used when *read-eval* == true")
        record_class = RT.class_for_name_non_loading(str(record_name))
        if isinstance(form, IPersistentMap):
            # Long form: #Class{:k v ...} → Class.create(map)
            for s in (RT.keys(form) if RT.keys(form) is not None else iter(())):
                if not isinstance(s, Keyword):
                    raise RuntimeError(
                        f"Unreadable defrecord form: key must be Keyword, got {s}")
            return Reflector.invoke_static_method(record_class, "create", [form])
        if isinstance(form, IPersistentVector):
            # Short form: positional args to the constructor.
            return Reflector.invoke_constructor(record_class, RT.to_array(form))
        raise RuntimeError(f"Unreadable constructor form starting with \"#{record_name}\"")


# --- Dispatch reader (#) ------------------------------------------------

cdef object _CTOR_READER = _CtorReader()


cdef class _DispatchReader(AFn):
    def __call__(self, reader, hash_ch, opts, pending_forms):
        ch = _read1(reader)
        if ch == "":
            raise RuntimeError("EOF while reading character")
        fn = _DISPATCH_MACROS.get(ch)
        if fn is None:
            _unread(reader, ch)
            pending_forms = _ensure_pending(pending_forms)
            result = _CTOR_READER(reader, ch, opts, pending_forms)
            if result is not None:
                return result
            raise RuntimeError(f"No dispatch macro for: {ch}")
        return fn(reader, ch, opts, pending_forms)


# --- Wire up the macro tables -------------------------------------------

_MACROS['"'] = _StringReader()
_MACROS[';'] = _CommentReader()
_MACROS["'"] = _WrappingReader(_LR_QUOTE)
_MACROS['@'] = _WrappingReader(_LR_DEREF)
_MACROS['^'] = _MetaReader()
_MACROS['`'] = _SyntaxQuoteReader()
_MACROS['~'] = _UnquoteReader()
_MACROS['('] = _ListReader()
_MACROS[')'] = _UnmatchedDelimiterReader()
_MACROS['['] = _VectorReader()
_MACROS[']'] = _UnmatchedDelimiterReader()
_MACROS['{'] = _MapReader()
_MACROS['}'] = _UnmatchedDelimiterReader()
_MACROS['\\'] = _CharacterReader()
_MACROS['%'] = _ArgReader()
_MACROS['#'] = _DispatchReader()


_DISPATCH_MACROS['^'] = _MetaReader()
_DISPATCH_MACROS['#'] = _SymbolicValueReader()
_DISPATCH_MACROS["'"] = _VarReader()
_DISPATCH_MACROS['"'] = _RegexReader()
_DISPATCH_MACROS['('] = _FnReader()
_DISPATCH_MACROS['{'] = _SetReader()
_DISPATCH_MACROS['='] = _EvalReader()
_DISPATCH_MACROS['!'] = _CommentReader()
_DISPATCH_MACROS['<'] = _UnreadableReader()
_DISPATCH_MACROS['_'] = _DiscardReader()
_DISPATCH_MACROS['?'] = _ConditionalReader()
_DISPATCH_MACROS[':'] = _NamespaceMapReader()
