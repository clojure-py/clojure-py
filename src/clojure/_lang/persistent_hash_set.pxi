# Port of clojure.lang.PersistentHashSet (and APersistentSet / ATransientSet).
#
# Sets are persistent maps where the value is the key. Most operations
# delegate to the underlying PersistentHashMap.


cdef object _PHS_NF = object()  # sentinel for transient .contains lookups


cdef class PersistentHashSet:
    """A persistent set backed by a PersistentHashMap."""

    cdef readonly PersistentHashMap _impl
    cdef object _meta
    cdef int32_t _hash_cache
    cdef int32_t _hasheq_cache
    cdef object __weakref__

    def __cinit__(self):
        pass

    @staticmethod
    cdef PersistentHashSet _make(object meta, PersistentHashMap impl):
        cdef PersistentHashSet s = PersistentHashSet.__new__(PersistentHashSet)
        s._meta = meta
        s._impl = impl
        return s

    @staticmethod
    def create(*items):
        t = _PHS_EMPTY.as_transient()
        for x in items:
            t.conj(x)
        return t.persistent()

    @staticmethod
    def from_iterable(iterable):
        t = _PHS_EMPTY.as_transient()
        for x in iterable:
            t.conj(x)
        return t.persistent()

    @staticmethod
    def create_with_check(*items):
        t = _PHS_EMPTY.as_transient()
        cdef int i = 0
        for x in items:
            t.conj(x)
            i += 1
            if t.count() != i:
                raise ValueError(f"Duplicate key: {x!r}")
        return t.persistent()

    def count(self):
        return self._impl.count()

    def __len__(self):
        return self._impl.count()

    def contains(self, key):
        return self._impl.contains_key(key)

    def __contains__(self, key):
        return self._impl.contains_key(key)

    def get(self, key, not_found=NOT_FOUND):
        return self._impl.val_at(key, not_found)

    def disjoin(self, key):
        if not self._impl.contains_key(key):
            return self
        return PersistentHashSet._make(self._meta, self._impl.without(key))

    def cons(self, o):
        if self._impl.contains_key(o):
            return self
        return PersistentHashSet._make(self._meta, self._impl.assoc(o, o))

    def empty(self):
        if self._meta is None:
            return PERSISTENT_HASH_SET_EMPTY
        return PersistentHashSet._make(self._meta, _PHM_EMPTY)

    def seq(self):
        impl_seq = self._impl.seq()
        if impl_seq is None:
            return None
        return _SetKeySeq(impl_seq)

    def equiv(self, other):
        if other is self:
            return True
        return _set_equiv(self, other)

    def __eq__(self, other):
        return self.equiv(other)

    def __ne__(self, other):
        return not self.equiv(other)

    def __hash__(self):
        if self._hash_cache != 0:
            return self._hash_cache
        cdef int32_t h = 0
        for x in self:
            h = _to_int32_mask(<long long>h + (0 if x is None else <long long>hash(x)))
        self._hash_cache = h
        return h

    def hasheq(self):
        if self._hasheq_cache != 0:
            return self._hasheq_cache
        result = Murmur3.hash_unordered(self)
        self._hasheq_cache = result
        return result

    def __iter__(self):
        for entry in self._impl:
            yield entry.key()

    def __call__(self, key, not_found=NOT_FOUND):
        # Sets-as-functions: returns key if present, not_found / nil otherwise.
        return self._impl.val_at(key, not_found)

    def meta(self):
        return self._meta

    def with_meta(self, meta):
        if self._meta is meta:
            return self
        return PersistentHashSet._make(meta, self._impl)

    def as_transient(self):
        return TransientHashSet._from_persistent(self._impl.as_transient())

    def __bool__(self):
        return self._impl.count() > 0

    def __str__(self):
        parts = []
        for x in self:
            parts.append(_print_str(x))
        return "#{" + " ".join(parts) + "}"

    def __repr__(self):
        return self.__str__()


cdef bint _set_equiv(PersistentHashSet a, object other):
    if isinstance(other, PersistentHashSet):
        b = <PersistentHashSet>other
        if a._impl._count != b._impl._count:
            return False
        for x in a:
            if not b._impl.contains_key(x):
                return False
        return True
    if isinstance(other, IPersistentSet):
        if other.count() != a._impl._count:
            return False
        for x in a:
            if not other.contains(x):
                return False
        return True
    if isinstance(other, (set, frozenset)):
        if len(other) != a._impl._count:
            return False
        for x in a:
            if x not in other:
                return False
        return True
    return False


cdef class _SetKeySeq(ASeq):
    """Yields keys from an underlying entry seq."""

    cdef object _entry_seq

    def __cinit__(self, entry_seq=None):
        if entry_seq is None:
            return
        self._entry_seq = entry_seq

    def first(self):
        return self._entry_seq.first().key()

    def next(self):
        nxt = self._entry_seq.next()
        if nxt is None:
            return None
        return _SetKeySeq(nxt)

    def with_meta(self, meta):
        cdef _SetKeySeq s = _SetKeySeq(self._entry_seq)
        s._meta = meta
        return s


cdef class TransientHashSet:
    """Mutable companion to PersistentHashSet."""

    cdef object _impl     # ITransientMap
    cdef object __weakref__

    def __cinit__(self):
        pass

    @staticmethod
    cdef TransientHashSet _from_persistent(impl):
        cdef TransientHashSet t = TransientHashSet.__new__(TransientHashSet)
        t._impl = impl
        return t

    def count(self):
        return self._impl.count()

    def __len__(self):
        return self._impl.count()

    def conj(self, val):
        m = self._impl.assoc(val, val)
        if m is not self._impl:
            self._impl = m
        return self

    def contains(self, key):
        return self._impl.val_at(key, _PHS_NF) is not _PHS_NF

    def disjoin(self, key):
        m = self._impl.without(key)
        if m is not self._impl:
            self._impl = m
        return self

    def get(self, key, not_found=NOT_FOUND):
        return self._impl.val_at(key, not_found)

    def persistent(self):
        return PersistentHashSet._make(None, self._impl.persistent())

    def __call__(self, key, not_found=NOT_FOUND):
        return self._impl.val_at(key, not_found)


cdef PersistentHashSet _PHS_EMPTY = PersistentHashSet._make(None, _PHM_EMPTY)
PERSISTENT_HASH_SET_EMPTY = _PHS_EMPTY


IPersistentSet.register(PersistentHashSet)
IPersistentCollection.register(PersistentHashSet)
Counted.register(PersistentHashSet)
IFn.register(PersistentHashSet)
IHashEq.register(PersistentHashSet)
IMeta.register(PersistentHashSet)
IObj.register(PersistentHashSet)
IEditableCollection.register(PersistentHashSet)


ITransientSet.register(TransientHashSet)
ITransientCollection.register(TransientHashSet)
Counted.register(TransientHashSet)
IFn.register(TransientHashSet)
