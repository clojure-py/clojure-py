# Port of clojure.lang.Symbol.
#
# A Symbol is an immutable (ns, name) pair where ns is optional. Symbols are
# Named, IHashEq, IObj (carry metadata), IFn (callable as map lookup), and
# Comparable. They are NOT interned (unlike Keywords) — equality is structural.


cdef class Symbol:
    """clojure.lang.Symbol — a namespaced name."""

    cdef readonly str ns
    cdef readonly str name
    cdef int32_t _hashcode
    cdef int32_t _hasheq
    cdef object _meta            # IPersistentMap or None
    cdef str _str_cache

    def __cinit__(self, ns, name):
        if not isinstance(name, str):
            raise TypeError(f"Symbol name must be str, got {type(name).__name__}")
        if ns is not None and not isinstance(ns, str):
            raise TypeError(f"Symbol ns must be str or None, got {type(ns).__name__}")
        cdef int32_t name_jhc = _java_string_hashcode(name)
        cdef int32_t ns_jhc = 0 if ns is None else _java_string_hashcode(<str>ns)
        cdef int32_t name_m3 = Murmur3._hash_unencoded_chars(name)
        self.ns = ns
        self.name = name
        self._meta = None
        self._str_cache = None
        self._hashcode = Util._hash_combine(name_jhc, ns_jhc)
        self._hasheq = Util._hash_combine(name_m3, ns_jhc)

    @staticmethod
    def intern(*args):
        """Symbol.intern(name) splits 'ns/name' on the first slash (with the
        special case Symbol.intern('/') = Symbol(None, '/')). Symbol.intern(ns,
        name) takes them split."""
        if len(args) == 1:
            nsname = args[0]
            if not isinstance(nsname, str):
                raise TypeError(f"Symbol.intern arg must be str, got {type(nsname).__name__}")
            i = nsname.find('/')
            if i == -1 or nsname == '/':
                return Symbol(None, nsname)
            return Symbol(nsname[:i], nsname[i + 1:])
        if len(args) == 2:
            return Symbol(args[0], args[1])
        raise TypeError(f"Symbol.intern takes 1 or 2 args, got {len(args)}")

    def get_namespace(self):
        return self.ns

    def get_name(self):
        return self.name

    def meta(self):
        return self._meta

    def with_meta(self, meta):
        if self._meta is meta:
            return self
        cdef Symbol s = Symbol(self.ns, self.name)
        s._meta = meta
        return s

    def hasheq(self):
        return self._hasheq

    def __hash__(self):
        return self._hashcode

    def __eq__(self, other):
        if self is other:
            return True
        if not isinstance(other, Symbol):
            return False
        cdef Symbol s = <Symbol>other
        return self.name == s.name and self.ns == s.ns

    def __ne__(self, other):
        return not self.__eq__(other)

    def __str__(self):
        if self._str_cache is None:
            if self.ns is not None:
                self._str_cache = self.ns + '/' + self.name
            else:
                self._str_cache = self.name
        return self._str_cache

    def __repr__(self):
        return self.__str__()

    cdef int _compare(self, other) except -2:
        if not isinstance(other, Symbol):
            raise TypeError(f"cannot compare Symbol to {type(other).__name__}")
        cdef Symbol s = <Symbol>other
        if self.name == s.name and self.ns == s.ns:
            return 0
        if self.ns is None and s.ns is not None:
            return -1
        if self.ns is not None:
            if s.ns is None:
                return 1
            if self.ns < s.ns:
                return -1
            if self.ns > s.ns:
                return 1
        if self.name < s.name:
            return -1
        if self.name > s.name:
            return 1
        return 0

    def compare_to(self, other):
        return self._compare(other)

    def __lt__(self, other):
        return self._compare(other) < 0

    def __le__(self, other):
        return self._compare(other) <= 0

    def __gt__(self, other):
        return self._compare(other) > 0

    def __ge__(self, other):
        return self._compare(other) >= 0

    def __call__(self, obj, not_found=NOT_FOUND):
        # Symbol as IFn: (sym map) → map lookup. Java delegates to RT.get;
        # we approximate by using ILookup directly, then falling back to
        # Python's __getitem__ for plain dicts and similar containers.
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


IFn.register(Symbol)
IObj.register(Symbol)
IMeta.register(Symbol)
Named.register(Symbol)
IHashEq.register(Symbol)
