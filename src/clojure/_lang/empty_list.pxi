# Stand-in for clojure.lang.PersistentList$EmptyList until the full
# PersistentList lands. Plays the role of Clojure's `()` — the canonical
# empty seq returned by collection.empty() / seq.more() at end-of-seq.


cdef class EmptyList:
    """Singleton empty list / empty seq."""

    cdef object _meta
    cdef object __weakref__

    def first(self):
        return None

    def next(self):
        return None

    def more(self):
        return self

    def seq(self):
        return None

    def empty(self):
        return self

    def count(self):
        return 0

    def cons(self, o):
        # Resolved at call time — PersistentList is defined later in the
        # include chain, but it is in module scope by the time anyone calls
        # this method.
        return PersistentList(o, None, 1)

    def equiv(self, o):
        if o is self or isinstance(o, EmptyList):
            return True
        if isinstance(o, Sequential) or isinstance(o, (list, tuple)):
            try:
                return len(o) == 0
            except TypeError:
                # No __len__ — walk the seq.
                if isinstance(o, Sequential):
                    return o.seq() is None
                return False
        return False

    def __eq__(self, o):
        return self.equiv(o)

    def __ne__(self, o):
        return not self.equiv(o)

    def __hash__(self):
        # Java: empty AbstractList.hashCode == 1.
        return 1

    def hasheq(self):
        # Java: hashOrdered over an empty seq → mix_coll_hash(1, 0).
        return Murmur3._mix_coll_hash(1, 0)

    def meta(self):
        return self._meta

    def with_meta(self, meta):
        if self._meta is meta:
            return self
        cdef EmptyList el = EmptyList()
        el._meta = meta
        return el

    def __iter__(self):
        return iter(())

    def __len__(self):
        return 0

    def __bool__(self):
        # Empty seqs are truthy as objects but represent empty in Clojure.
        # Python convention — empty container is falsy.
        return False

    def __str__(self):
        return "()"

    def __repr__(self):
        return self.__str__()

    def peek(self):
        return None

    def pop(self):
        raise IndexError("Can't pop empty list")


# Module-level singleton (still register a weakref slot so consumers may hold
# weak references to per-meta variants).
cdef EmptyList _empty_list = EmptyList()


ISeq.register(EmptyList)
IPersistentList.register(EmptyList)
IPersistentStack.register(EmptyList)
IPersistentCollection.register(EmptyList)
Sequential.register(EmptyList)
Counted.register(EmptyList)
IHashEq.register(EmptyList)
IMeta.register(EmptyList)
IObj.register(EmptyList)
