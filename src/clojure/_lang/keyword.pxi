# Port of clojure.lang.Keyword.
#
# Keywords intern by Symbol identity into a global WeakValueDictionary. Two
# Keyword.intern calls with the same Symbol return the same instance (until
# the Keyword is GC'd). Direct construction is allowed but bypasses interning;
# equality stays structural so correctness is preserved either way.

import weakref
from threading import Lock


cdef object _kw_table = weakref.WeakValueDictionary()
cdef object _kw_lock = Lock()


cdef class Keyword:
    """clojure.lang.Keyword — an interned :name or :ns/name."""

    cdef readonly Symbol sym
    cdef readonly int32_t _hashcode
    cdef readonly int32_t _hasheq
    cdef str _str_cache
    cdef object __weakref__         # required for WeakValueDictionary interning

    def __cinit__(self, sym):
        if isinstance(sym, str):
            sym = Symbol.intern(sym)
        elif not isinstance(sym, Symbol):
            raise TypeError(f"Keyword takes Symbol or str, got {type(sym).__name__}")
        if (<Symbol>sym).meta() is not None:
            sym = (<Symbol>sym).with_meta(None)
        self.sym = sym
        # Java: hashCode = sym.hashCode + 0x9e3779b9; same for hasheq.
        cdef uint32_t h = (<uint32_t>(<Symbol>sym)._hashcode + 0x9e3779b9u) & 0xFFFFFFFFu
        cdef uint32_t he = (<uint32_t>(<Symbol>sym)._hasheq + 0x9e3779b9u) & 0xFFFFFFFFu
        self._hashcode = <int32_t>h
        self._hasheq = <int32_t>he
        self._str_cache = None

    @staticmethod
    def intern(*args):
        """Keyword.intern(sym|name) or Keyword.intern(ns, name).

        Returns the canonical Keyword for the underlying Symbol, creating one
        if absent. Synchronized on a module-level lock so it's safe under
        free-threaded CPython."""
        if len(args) == 1:
            arg = args[0]
            sym = arg if isinstance(arg, Symbol) else Symbol.intern(arg)
        elif len(args) == 2:
            sym = Symbol.intern(args[0], args[1])
        else:
            raise TypeError(f"Keyword.intern takes 1 or 2 args, got {len(args)}")

        if sym.meta() is not None:
            sym = sym.with_meta(None)

        with _kw_lock:
            existing = _kw_table.get(sym)
            if existing is not None:
                return existing
            kw = Keyword(sym)
            _kw_table[sym] = kw
            return kw

    @staticmethod
    def find(*args):
        """Lookup an existing interned Keyword without creating one."""
        if len(args) == 1:
            arg = args[0]
            sym = arg if isinstance(arg, Symbol) else Symbol.intern(arg)
        elif len(args) == 2:
            sym = Symbol.intern(args[0], args[1])
        else:
            raise TypeError(f"Keyword.find takes 1 or 2 args, got {len(args)}")
        return _kw_table.get(sym)

    def get_namespace(self):
        return self.sym.ns

    def get_name(self):
        return self.sym.name

    def hasheq(self):
        return self._hasheq

    def __hash__(self):
        return self._hashcode

    def __eq__(self, other):
        if self is other:
            return True
        if not isinstance(other, Keyword):
            return False
        return self.sym == (<Keyword>other).sym

    def __ne__(self, other):
        return not self.__eq__(other)

    def __str__(self):
        if self._str_cache is None:
            self._str_cache = ':' + str(self.sym)
        return self._str_cache

    def __repr__(self):
        return self.__str__()

    def __lt__(self, other):
        if not isinstance(other, Keyword):
            raise TypeError(f"cannot compare Keyword to {type(other).__name__}")
        return self.sym._compare((<Keyword>other).sym) < 0

    def __le__(self, other):
        if not isinstance(other, Keyword):
            raise TypeError(f"cannot compare Keyword to {type(other).__name__}")
        return self.sym._compare((<Keyword>other).sym) <= 0

    def __gt__(self, other):
        if not isinstance(other, Keyword):
            raise TypeError(f"cannot compare Keyword to {type(other).__name__}")
        return self.sym._compare((<Keyword>other).sym) > 0

    def __ge__(self, other):
        if not isinstance(other, Keyword):
            raise TypeError(f"cannot compare Keyword to {type(other).__name__}")
        return self.sym._compare((<Keyword>other).sym) >= 0

    def compare_to(self, other):
        if not isinstance(other, Keyword):
            raise TypeError(f"cannot compare Keyword to {type(other).__name__}")
        return self.sym._compare((<Keyword>other).sym)

    def __call__(self, *args):
        # JVM keywords are callable with 1 or 2 args (key, [not-found]).
        # Any other arity raises IllegalArgumentException with the message
        # `Wrong number of args (N) passed to: :kw`. JVM caps the
        # reported count at 20; > 20 collapses to "(> 20)".
        cdef int n = len(args)
        if n == 1 or n == 2:
            obj = args[0]
            not_found = args[1] if n == 2 else NOT_FOUND
            if obj is None:
                return None if not_found is NOT_FOUND else not_found
            if isinstance(obj, ILookup):
                if not_found is NOT_FOUND:
                    return obj.val_at(self)
                return obj.val_at(self, not_found)
            try:
                return obj[self]
            except (KeyError, TypeError):
                return None if not_found is NOT_FOUND else not_found
        if n > 20:
            count_str = "> 20"
        else:
            count_str = str(n)
        raise ValueError(
            f"Wrong number of args ({count_str}) passed to: {self}")


IFn.register(Keyword)
Named.register(Keyword)
IHashEq.register(Keyword)
