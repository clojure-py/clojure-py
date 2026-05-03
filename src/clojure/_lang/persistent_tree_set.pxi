# Port of clojure.lang.PersistentTreeSet.
#
# Sorted set backed by a PersistentTreeMap. Entries are stored as (key, key)
# pairs. Iteration yields keys in comparator order.


cdef class PersistentTreeSet:
    """A persistent sorted set backed by a PersistentTreeMap."""

    cdef readonly PersistentTreeMap _impl
    cdef object _meta
    cdef int32_t _hash_cache
    cdef int32_t _hasheq_cache
    cdef object __weakref__

    def __cinit__(self):
        pass

    @staticmethod
    cdef PersistentTreeSet _make(object meta, PersistentTreeMap impl):
        cdef PersistentTreeSet s = PersistentTreeSet.__new__(PersistentTreeSet)
        s._meta = meta
        s._impl = impl
        return s

    @staticmethod
    def create(*items):
        # Mirror JVM create(ISeq): a single seq arg is the contents.
        if len(items) == 1 and (isinstance(items[0], (Seqable, ISeq))
                                or items[0] is None):
            ret = _PTS_EMPTY
            s = RT.seq(items[0])
            while s is not None:
                ret = ret.cons(s.first())
                s = s.next()
            return ret
        ret = _PTS_EMPTY
        for x in items:
            ret = ret.cons(x)
        return ret

    @staticmethod
    def create_with_comparator(comp, *items):
        if len(items) == 1 and (isinstance(items[0], (Seqable, ISeq))
                                or items[0] is None):
            ret = PersistentTreeSet._make(None, _make_ptm(None, comp, None, 0))
            s = RT.seq(items[0])
            while s is not None:
                ret = ret.cons(s.first())
                s = s.next()
            return ret
        ret = PersistentTreeSet._make(None, _make_ptm(None, comp, None, 0))
        for x in items:
            ret = ret.cons(x)
        return ret

    def count(self):
        return self._impl._count

    def __len__(self):
        return self._impl._count

    def contains(self, key):
        return self._impl.contains_key(key)

    def __contains__(self, key):
        return self._impl.contains_key(key)

    def get(self, key, not_found=NOT_FOUND):
        return self._impl.val_at(key, not_found)

    def disjoin(self, key):
        if not self._impl.contains_key(key):
            return self
        return PersistentTreeSet._make(self._meta, self._impl.without(key))

    def cons(self, o):
        if self._impl.contains_key(o):
            return self
        return PersistentTreeSet._make(self._meta, self._impl.assoc(o, o))

    def empty(self):
        return PersistentTreeSet._make(self._meta, self._impl.empty())

    def seq(self):
        impl_seq = self._impl.seq()
        if impl_seq is None:
            return None
        return _SetKeySeq(impl_seq)

    def rseq(self):
        impl_seq = self._impl.rseq()
        if impl_seq is None:
            return None
        return _SetKeySeq(impl_seq)

    # --- Sorted ---

    def comparator(self):
        return self._impl._comp

    def entry_key(self, entry):
        # For sets, the entry IS the key.
        return entry

    def seq_with_comparator(self, ascending):
        impl_seq = self._impl.seq_with_comparator(ascending)
        if impl_seq is None:
            return None
        return _SetKeySeq(impl_seq)

    def seq_from(self, key, ascending):
        impl_seq = self._impl.seq_from(key, ascending)
        if impl_seq is None:
            return None
        return _SetKeySeq(impl_seq)

    # --- equality ---

    def equiv(self, other):
        if other is self:
            return True
        return _tree_set_equiv(self, other)

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
        return self._impl.val_at(key, not_found)

    def meta(self):
        return self._meta

    def with_meta(self, meta):
        if self._meta is meta:
            return self
        return PersistentTreeSet._make(meta, self._impl)

    def __bool__(self):
        return self._impl._count > 0

    def __str__(self):
        parts = []
        for x in self:
            parts.append(_print_str(x))
        return "#{" + " ".join(parts) + "}"

    def __repr__(self):
        return self.__str__()


cdef bint _tree_set_equiv(PersistentTreeSet a, object other):
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


cdef PersistentTreeSet _PTS_EMPTY = PersistentTreeSet._make(None, _PTM_EMPTY)
PERSISTENT_TREE_SET_EMPTY = _PTS_EMPTY


IPersistentSet.register(PersistentTreeSet)
IPersistentCollection.register(PersistentTreeSet)
Counted.register(PersistentTreeSet)
IFn.register(PersistentTreeSet)
IHashEq.register(PersistentTreeSet)
IMeta.register(PersistentTreeSet)
IObj.register(PersistentTreeSet)
Reversible.register(PersistentTreeSet)
Sorted.register(PersistentTreeSet)
