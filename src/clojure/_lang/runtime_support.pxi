# Runtime support — RT helper namespace, Compiler stubs, Reflector for
# Python interop.
#
# JVM Clojure ships these as huge classes; we provide only what the LispReader
# (and later the Compiler/core) actually need. The set will grow as more
# pieces of the runtime come online.


import importlib as _importlib
import builtins as _builtins
import itertools as _runtime_itertools


cdef object _RT_ID_COUNTER = _runtime_itertools.count(1)


# --- RT — runtime helpers ------------------------------------------------

class RT:
    """Java's clojure.lang.RT — static helpers for the rest of the runtime."""

    # Vars (initialized below in _init_rt_runtime_vars)
    READEVAL = None
    SUPPRESS_READ = None
    READER_RESOLVER = None
    DATA_READERS = None
    DEFAULT_DATA_READERS = None
    DEFAULT_DATA_READER_FN = None
    CURRENT_NS = None        # *ns*
    PRINT_META = None
    PRINT_DUP = None

    # Keyword constants
    LINE_KEY = Keyword.intern(None, "line")
    COLUMN_KEY = Keyword.intern(None, "column")
    TAG_KEY = Keyword.intern(None, "tag")
    PARAM_TAGS_KEY = Keyword.intern(None, "param-tags")
    FILE_KEY = Keyword.intern(None, "file")

    # Truth constants (Clojure has these as Boolean.TRUE/FALSE)
    T = True
    F = False

    # --- seq ops ---

    @staticmethod
    def list(*args):
        return PersistentList.create(args)

    @staticmethod
    def list_star(*args):
        # (list* a b c rest) → a cons'd onto (cons b (cons c rest)).
        if len(args) == 0:
            return None
        if len(args) == 1:
            return RT.seq(args[0])
        head = args[:-1]
        tail = args[-1]
        s = RT.seq(tail)
        for x in reversed(head):
            s = Cons(x, s)
        return s

    @staticmethod
    def cons(x, seq):
        if seq is None:
            return PersistentList.create([x])
        if isinstance(seq, ISeq):
            return Cons(x, seq)
        return Cons(x, RT.seq(seq))

    @staticmethod
    def seq(x):
        if x is None:
            return None
        if isinstance(x, Seqable):
            return x.seq()
        if isinstance(x, str):
            return None if len(x) == 0 else IteratorSeq.from_iterable(x)
        if isinstance(x, (list, tuple)):
            return None if len(x) == 0 else IteratorSeq.from_iterable(x)
        try:
            it = iter(x)
        except TypeError:
            raise TypeError(
                f"Don't know how to create ISeq from: {type(x).__name__}")
        return IteratorSeq.from_iterable(x)

    @staticmethod
    def first(x):
        s = RT.seq(x)
        return None if s is None else s.first()

    @staticmethod
    def second(x):
        s = RT.seq(x)
        if s is None: return None
        n = s.next()
        return None if n is None else n.first()

    @staticmethod
    def next(x):
        s = RT.seq(x)
        return None if s is None else s.next()

    @staticmethod
    def rest(x):
        s = RT.seq(x)
        return _empty_list if s is None else s.more()

    @staticmethod
    def more(x):
        # JVM clojure.lang.RT.more — same as our `rest`. Kept as a
        # separate name so the 1:1 core.clj translation can use it.
        return RT.rest(x)

    @staticmethod
    def nth(coll, idx, not_found=NOT_FOUND):
        """JVM-style nth. Indexed lookup with O(1) for vectors and
        O(n) for seqs. With not_found returns it instead of raising
        on out-of-range; without not_found, raises IndexError."""
        if coll is None:
            if not_found is NOT_FOUND:
                raise IndexError("nth on nil")
            return not_found
        # Indexed (vector / list / tuple / str): direct index
        if hasattr(coll, "nth"):
            if not_found is NOT_FOUND:
                return coll.nth(idx)
            return coll.nth(idx, not_found)
        if isinstance(coll, (list, tuple, str, bytes)):
            try:
                return coll[idx]
            except IndexError:
                if not_found is NOT_FOUND:
                    raise
                return not_found
        # Seq / iterable — walk
        s = RT.seq(coll)
        cur = s
        i = 0
        while cur is not None:
            if i == idx:
                return cur.first()
            cur = cur.next()
            i += 1
        if not_found is NOT_FOUND:
            raise IndexError("nth: index out of bounds")
        return not_found

    @staticmethod
    def int_cast(x):
        """JVM RT.intCast — coerce to int. Forwards to Numbers.int_cast."""
        return Numbers.int_cast(x)

    @staticmethod
    def unchecked_int_cast(x):
        return Numbers.int_cast(x)

    @staticmethod
    def unchecked_long_cast(x):
        return Numbers.int_cast(x)

    @staticmethod
    def long_cast(x):
        return Numbers.int_cast(x)

    @staticmethod
    def iter(coll):
        """JVM RT.iter — return a Python iterator over the collection.
        For nil returns an empty iterator."""
        if coll is None:
            return iter(())
        if hasattr(coll, "__iter__"):
            return iter(coll)
        # Seq protocol fallback
        s = RT.seq(coll)
        return iter(s) if s is not None else iter(())

    @staticmethod
    def chunk_iterator_seq(it):
        """JVM RT.chunkIteratorSeq — wrap an iterator as a (lazy) seq.
        We currently materialize via IteratorSeq, which preserves seq
        semantics but loses the chunking optimization. The transducer
        callers (sequence, etc.) work correctly; only bulk-throughput
        of long pipelines suffers."""
        if it is None:
            return None
        return IteratorSeq.from_iterable(it)

    @staticmethod
    def peek(coll):
        """Look at the first item of a list/queue or last of a vector,
        without removing. Returns nil for nil/empty."""
        if coll is None:
            return None
        if hasattr(coll, "peek"):
            return coll.peek()
        if isinstance(coll, (list, tuple)):
            return coll[len(coll) - 1] if coll else None
        return None

    @staticmethod
    def pop(coll):
        """Drop the peek end of a list/queue/vector. Raises on empty."""
        if coll is None:
            return None
        if hasattr(coll, "pop"):
            return coll.pop()
        raise TypeError(
            "Can't pop from " + type(coll).__name__)

    @staticmethod
    def find(coll, key):
        """Returns the map entry [k v] for key, or nil if not present."""
        if coll is None:
            return None
        if hasattr(coll, "entry_at"):
            return coll.entry_at(key)
        if hasattr(coll, "get") and hasattr(coll, "contains_key"):
            if coll.contains_key(key):
                return MapEntry(key, coll.get(key))
            return None
        try:
            if key in coll:
                return MapEntry(key, coll[key])
        except TypeError:
            pass
        return None

    @staticmethod
    def subvec(v, start, end):
        """Slice of a vector: returns a new vector with elements
        [start, end)."""
        if start < 0 or end < start or end > v.count():
            raise IndexError("subvec out of range")
        if start == 0 and end == v.count():
            return v
        # Build using create on the slice.
        items = []
        cdef int i = start
        while i < end:
            items.append(v.nth(i))
            i += 1
        return PersistentVector.create(*items)

    @staticmethod
    def count(x):
        if x is None:
            return 0
        # Python builtins (str/list/tuple) have a `count` method that
        # takes a value to count occurrences of — not what Clojure
        # `count` means. Use len() for those.
        if isinstance(x, (str, list, tuple, dict, bytes)):
            return len(x)
        if hasattr(x, "count"):
            return x.count()
        return len(x)

    # --- map / set ops ---

    @staticmethod
    def map(*args):
        if len(args) == 0:
            return _PHM_EMPTY
        if len(args) % 2 != 0:
            raise ValueError("RT.map requires an even number of args")
        if len(args) <= 16:
            return PersistentArrayMap.create(*args)
        return PersistentHashMap.create(*args)

    @staticmethod
    def map_unique_keys(*args):
        if len(args) <= 16:
            return PersistentArrayMap.create(*args)
        return PersistentHashMap.create(*args)

    @staticmethod
    def set(*args):
        return PersistentHashSet.from_iterable(args)

    @staticmethod
    def assoc(coll, k, v):
        if coll is None:
            return PersistentArrayMap.create(k, v)
        return coll.assoc(k, v)

    @staticmethod
    def dissoc(coll, k):
        if coll is None:
            return None
        return coll.without(k)

    @staticmethod
    def get(coll, k, not_found=None):
        if coll is None:
            return not_found
        if hasattr(coll, "val_at"):
            return coll.val_at(k, not_found)
        try:
            return coll[k]
        except (KeyError, IndexError, TypeError):
            return not_found

    @staticmethod
    def contains(coll, k):
        if coll is None:
            return False
        if hasattr(coll, "contains_key"):
            return coll.contains_key(k)
        if hasattr(coll, "contains"):
            return coll.contains(k)
        return k in coll

    @staticmethod
    def conj(coll, x):
        if coll is None:
            return PersistentList.create([x])
        return coll.cons(x)

    @staticmethod
    def meta(o):
        if hasattr(o, "meta"):
            return o.meta()
        return None

    @staticmethod
    def keys(m):
        if m is None: return None
        if hasattr(m, "seq"):
            s = m.seq()
            if s is None: return None
            return _SetKeySeq(s)
        return None

    @staticmethod
    def vals(m):
        if m is None: return None
        if hasattr(m, "seq"):
            s = m.seq()
            if s is None: return None
            # Walk the entry seq and yield each value. Materialize via
            # IteratorSeq so the result is a Clojure ISeq.
            vs = []
            cur = s
            while cur is not None:
                e = cur.first()
                vs.append(e.val())
                cur = cur.next()
            return IteratorSeq.from_iterable(vs)
        return None

    # --- vars / namespaces ---

    @staticmethod
    def var(ns_name, name=None):
        if name is None:
            # var(symbol) form
            sym = ns_name
            if isinstance(sym, str):
                sym = Symbol.intern(sym)
            ns = Namespace.find_or_create(Symbol.intern(sym.ns))
            return ns.intern(Symbol.intern(sym.name))
        ns = Namespace.find_or_create(Symbol.intern(ns_name))
        return ns.intern(Symbol.intern(name))

    # --- class lookup ---

    @staticmethod
    def class_for_name(name):
        """Resolve a dotted Python name to a class. 'collections.Counter' →
        the class, 'int' → builtins.int.

        Uses rfind+slice rather than str.rsplit/split — the latter
        segfault under Cython 3.2 + CPython 3.14t (free-threading
        codegen bug)."""
        cdef int dot = name.rfind(".")
        if dot < 0:
            return getattr(_builtins, name)
        mod_name = name[:dot]
        cls_name = name[dot + 1:]
        mod = _importlib.import_module(mod_name)
        return getattr(mod, cls_name)

    @staticmethod
    def class_for_name_non_loading(name):
        # Python doesn't separate "load" from "lookup" at this level.
        return RT.class_for_name(name)

    # --- arrays / coercion ---

    @staticmethod
    def to_array(coll):
        if coll is None:
            return []
        if isinstance(coll, list):
            return list(coll)
        s = RT.seq(coll)
        out = []
        while s is not None:
            out.append(s.first())
            s = s.next()
        return out

    @staticmethod
    def boolean_cast(x):
        # Clojure: only false and nil are falsy; everything else is true.
        if x is None or x is False:
            return False
        return True

    @staticmethod
    def is_reduced(x):
        return isinstance(x, Reduced)

    # --- ID / suppress-read ---

    @staticmethod
    def next_id():
        return next(_RT_ID_COUNTER)

    @staticmethod
    def suppress_read():
        if RT.SUPPRESS_READ is None:
            return False
        try:
            return bool(RT.SUPPRESS_READ.deref())
        except Exception:
            return False


# Initialize the runtime Vars (called after Var/Namespace are loaded).
def _init_rt_runtime_vars():
    ns = Namespace.find_or_create(Symbol.intern("clojure.core"))
    user_ns = Namespace.find_or_create(Symbol.intern("user"))

    RT.READEVAL = Var.intern(ns, Symbol.intern("*read-eval*"), True).set_dynamic()
    RT.SUPPRESS_READ = Var.intern(ns, Symbol.intern("*suppress-read*"), False).set_dynamic()
    RT.READER_RESOLVER = Var.intern(ns, Symbol.intern("*reader-resolver*"), None).set_dynamic()
    RT.DATA_READERS = Var.intern(ns, Symbol.intern("*data-readers*"), _PHM_EMPTY).set_dynamic()
    RT.DEFAULT_DATA_READERS = Var.intern(ns, Symbol.intern("default-data-readers"), _PHM_EMPTY)
    RT.DEFAULT_DATA_READER_FN = Var.intern(
        ns, Symbol.intern("*default-data-reader-fn*"), None).set_dynamic()
    RT.CURRENT_NS = Var.intern(ns, Symbol.intern("*ns*"), user_ns).set_dynamic()
    RT.PRINT_META = Var.intern(ns, Symbol.intern("*print-meta*"), False).set_dynamic()
    RT.PRINT_DUP = Var.intern(ns, Symbol.intern("*print-dup*"), False).set_dynamic()


_init_rt_runtime_vars()


# --- Compiler stubs ------------------------------------------------------

class Compiler:
    """Just enough of clojure.lang.Compiler for the LispReader to function.
    Real Compiler logic arrives in a much later slice."""

    QUOTE = Symbol.intern("quote")
    THE_VAR = Symbol.intern("var")
    FN = Symbol.intern("fn*")
    DO_SYM = Symbol.intern("do")
    DEF_SYM = Symbol.intern("def")
    LET_SYM = Symbol.intern("let*")
    LOOP_SYM = Symbol.intern("loop*")
    RECUR_SYM = Symbol.intern("recur")
    IF_SYM = Symbol.intern("if")
    NEW_SYM = Symbol.intern("new")
    THROW_SYM = Symbol.intern("throw")
    TRY_SYM = Symbol.intern("try")
    CATCH_SYM = Symbol.intern("catch")
    FINALLY_SYM = Symbol.intern("finally")
    LETFN_SYM = Symbol.intern("letfn*")
    SET_BANG = Symbol.intern("set!")
    DOT_SYM = Symbol.intern(".")
    IMPORT_STAR = Symbol.intern("import*")
    DEFTYPE = Symbol.intern("deftype*")
    REIFY = Symbol.intern("reify*")
    CASE_SYM = Symbol.intern("case*")
    MONITOR_ENTER = Symbol.intern("monitor-enter")
    MONITOR_EXIT = Symbol.intern("monitor-exit")
    _AMP_ = Symbol.intern("&")

    SPECIAL_FORMS = frozenset([
        Symbol.intern("def"), Symbol.intern("loop*"), Symbol.intern("recur"),
        Symbol.intern("if"), Symbol.intern("case*"), Symbol.intern("let*"),
        Symbol.intern("letfn*"), Symbol.intern("do"), Symbol.intern("fn*"),
        Symbol.intern("quote"), Symbol.intern("var"), Symbol.intern("import*"),
        Symbol.intern("."), Symbol.intern("set!"), Symbol.intern("deftype*"),
        Symbol.intern("reify*"), Symbol.intern("try"), Symbol.intern("throw"),
        Symbol.intern("monitor-enter"), Symbol.intern("monitor-exit"),
        Symbol.intern("catch"), Symbol.intern("finally"), Symbol.intern("new"),
    ])

    @staticmethod
    def is_special(form):
        return isinstance(form, Symbol) and form in Compiler.SPECIAL_FORMS

    @staticmethod
    def current_ns():
        return RT.CURRENT_NS.deref()

    @staticmethod
    def maybe_resolve_in(ns, sym):
        """Look up `sym` in `ns`'s mappings — returns Var, class, or None."""
        if not isinstance(sym, Symbol):
            return None
        if sym.ns is not None:
            # Qualified: look up the namespace first.
            target_ns = Namespace.find(Symbol.intern(sym.ns))
            if target_ns is None:
                # Try alias.
                target_ns = ns.lookup_alias(Symbol.intern(sym.ns))
            if target_ns is None:
                return None
            return target_ns.find_interned_var(Symbol.intern(sym.name))
        return ns.get_mapping(sym)

    @staticmethod
    def names_static_member(sym):
        """True if sym looks like ClassName/staticMember AND ns part resolves
        to a class in the current namespace. Used by EvalReader for #=."""
        if not isinstance(sym, Symbol) or sym.ns is None:
            return False
        ns = Compiler.current_ns()
        cls = ns.get_mapping(Symbol.intern(sym.ns))
        return isinstance(cls, type)

    @staticmethod
    def maybe_special_tag(sym):
        """Stub for clojure.lang.Compiler$HostExpr/maybeSpecialTag.
        On the JVM this returns a primitive Class ref for special tags
        like 'long' or 'int'; in our port there are no primitive type
        hints, so always returns nil."""
        return None

    @staticmethod
    def maybe_class(sym, string_ok):
        """Stub for clojure.lang.Compiler$HostExpr/maybeClass — looks up
        a tag symbol as a class, returning nil on failure. We delegate
        to RT.class_for_name and swallow errors."""
        if isinstance(sym, Symbol):
            try:
                return RT.class_for_name(sym.name)
            except (ImportError, AttributeError):
                return None
        if string_ok and isinstance(sym, str):
            try:
                return RT.class_for_name(sym)
            except (ImportError, AttributeError):
                return None
        return None

    @staticmethod
    def resolve_symbol(sym):
        """Resolve `sym` in the current namespace.  Returns a fully-qualified
        Symbol (or `sym` unchanged if it's already qualified or nothing
        resolves)."""
        if not isinstance(sym, Symbol):
            return sym
        ns = Compiler.current_ns()
        if sym.ns is not None:
            # Try to resolve the ns part as a class name; if so, qualify with
            # the class's full name. Otherwise return as-is.
            cls = ns.get_mapping(Symbol.intern(sym.ns))
            if isinstance(cls, type):
                return Symbol.intern(cls.__module__ + "." + cls.__name__, sym.name)
            # Maybe it's a namespace alias.
            aliased = ns.lookup_alias(Symbol.intern(sym.ns))
            if aliased is not None:
                return Symbol.intern(aliased.name.name, sym.name)
            return sym
        # Unqualified: look up in current ns.
        v = ns.get_mapping(sym)
        if isinstance(v, Var):
            return Symbol.intern(v.ns.name.name, v.sym.name)
        if isinstance(v, type):
            return Symbol.intern(None, v.__module__ + "." + v.__name__)
        # Dotted name (`clojure.lang.MultiFn`) — try class lookup before
        # falling back to ns-qualifying. Skip names that aren't shaped
        # like Java FQNs (`.`, `..`, `.method`, `Class.`, etc.).
        n = sym.name
        if ("." in n
                and not n.startswith(".")
                and not n.endswith(".")
                and ".." not in n):
            try:
                RT.class_for_name(n)
                return sym  # resolves as a class — leave as-is
            except (ImportError, AttributeError, ValueError):
                pass
        return Symbol.intern(ns.name.name, sym.name)


# --- Reflector — Python interop ------------------------------------------

class Reflector:
    """clojure.lang.Reflector equivalent for Python — there's no separate
    reflection step in Python, so these are mostly trivial thin wrappers."""

    @staticmethod
    def invoke_constructor(cls, args):
        return cls(*args)

    @staticmethod
    def invoke_static_method(cls_or_name, method_name, args):
        cls = cls_or_name if isinstance(cls_or_name, type) else RT.class_for_name(cls_or_name)
        m = getattr(cls, method_name)
        return m(*args)

    @staticmethod
    def invoke_instance_method(obj, method_name, args):
        m = getattr(obj, method_name)
        return m(*args)

    @staticmethod
    def get_static_field(cls_or_name, field_name):
        cls = cls_or_name if isinstance(cls_or_name, type) else RT.class_for_name(cls_or_name)
        return getattr(cls, field_name)

    @staticmethod
    def get_instance_field(obj, field_name):
        return getattr(obj, field_name)
