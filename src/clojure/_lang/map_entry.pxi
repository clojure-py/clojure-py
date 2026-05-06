# Port of clojure.lang.MapEntry / AMapEntry.
#
# A MapEntry is a 2-element pair (key, val) that also behaves as an indexed
# sequence — `(first {:a 1})` returns a MapEntry that can be used as `[k v]`.
# In Java MapEntry extends APersistentVector and gets all vector behavior;
# we don't claim full IPersistentVector here (that would pull in assoc_n /
# cons / pop / peek with vector semantics that need a real backing vector).
# Instead MapEntry implements the read-side vector protocols (Indexed,
# Counted, Sequential, IFn) and equality with vectors / tuples / lists, which
# covers the practical ergonomics.


cdef class MapEntry:
    """A (key, val) pair. Indexable as [0]=key, [1]=val."""

    cdef readonly object _key
    cdef readonly object _val
    cdef object _meta
    cdef object __weakref__

    def __cinit__(self, key, val):
        self._key = key
        self._val = val

    @staticmethod
    def create(key, val):
        return MapEntry(key, val)

    def key(self):
        return self._key

    def val(self):
        return self._val

    def get_key(self):
        return self._key

    def get_value(self):
        return self._val

    def nth(self, int i, not_found=NOT_FOUND):
        if i == 0:
            return self._key
        if i == 1:
            return self._val
        if not_found is NOT_FOUND:
            raise IndexError(i)
        return not_found

    def count(self):
        return 2

    def length(self):
        return 2

    def __len__(self):
        return 2

    def __iter__(self):
        yield self._key
        yield self._val

    def __getitem__(self, key):
        if isinstance(key, int) and not isinstance(key, bool):
            i = key + 2 if key < 0 else key
            return self.nth(i)
        raise TypeError("MapEntry index must be int")

    def seq(self):
        # Walks as if [key, val].
        return _MapEntrySeq(self, 0)

    def __eq__(self, other):
        if other is self:
            return True
        if isinstance(other, MapEntry):
            return (Util.equals(self._key, (<MapEntry>other)._key)
                    and Util.equals(self._val, (<MapEntry>other)._val))
        if isinstance(other, PersistentVector):
            if (<PersistentVector>other)._cnt != 2:
                return False
            return (Util.equals(self._key, (<PersistentVector>other).nth(0))
                    and Util.equals(self._val, (<PersistentVector>other).nth(1)))
        if isinstance(other, (list, tuple)) and len(other) == 2:
            return (Util.equals(self._key, other[0])
                    and Util.equals(self._val, other[1]))
        return False

    def __ne__(self, other):
        return not self.__eq__(other)

    def __hash__(self):
        # Match APersistentVector.hashCode for 2 elements:
        # 31 * (31 + hash(key)) + hash(val), with None → 0.
        cdef object kh = 0 if self._key is None else hash(self._key)
        cdef object vh = 0 if self._val is None else hash(self._val)
        return _to_int32_mask(31 * (31 + kh) + vh)

    def hasheq(self):
        return Murmur3.hash_ordered([self._key, self._val])

    def equiv(self, other):
        if other is self:
            return True
        if isinstance(other, MapEntry):
            return (Util.equiv(self._key, (<MapEntry>other)._key)
                    and Util.equiv(self._val, (<MapEntry>other)._val))
        if isinstance(other, PersistentVector):
            if (<PersistentVector>other)._cnt != 2:
                return False
            return (Util.equiv(self._key, (<PersistentVector>other).nth(0))
                    and Util.equiv(self._val, (<PersistentVector>other).nth(1)))
        if isinstance(other, (list, tuple)) and len(other) == 2:
            return (Util.equiv(self._key, other[0])
                    and Util.equiv(self._val, other[1]))
        return False

    def __call__(self, int i):
        return self.nth(i)

    # --- Associative / IPersistentVector ergonomics ---
    #
    # JVM AMapEntry extends APersistentVector, so a MapEntry behaves
    # as a 2-element vector for indexed access AND assoc/cons. Our
    # MapEntry deliberately doesn't claim full IPersistentVector (the
    # backing data isn't a vector), but assoc / contains_key /
    # entry_at / cons mirror JVM's vector-of-2 semantics so update-in
    # on a map's entries works.

    def _as_vector(self):
        return PersistentVector.from_iterable([self._key, self._val])

    def assoc(self, key, val):
        if isinstance(key, int) and not isinstance(key, bool):
            return self._as_vector().assoc(key, val)
        raise IndexError("MapEntry assoc key must be 0 or 1")

    def assoc_n(self, int i, val):
        return self._as_vector().assoc_n(i, val)

    def contains_key(self, key):
        return isinstance(key, int) and not isinstance(key, bool) and 0 <= key < 2

    def entry_at(self, key):
        if isinstance(key, int) and not isinstance(key, bool) and 0 <= key < 2:
            return MapEntry(key, self.nth(key))
        return None

    def val_at(self, key, not_found=None):
        if isinstance(key, int) and not isinstance(key, bool) and 0 <= key < 2:
            return self.nth(key)
        return not_found

    def cons(self, x):
        return self._as_vector().cons(x)

    def __contains__(self, x):
        # Vector semantics: contains? checks key range, not value.
        return isinstance(x, int) and not isinstance(x, bool) and 0 <= x < 2

    def __bool__(self):
        return True

    def meta(self):
        return self._meta

    def with_meta(self, meta):
        cdef MapEntry e = MapEntry(self._key, self._val)
        e._meta = meta
        return e

    def __str__(self):
        return "[" + _print_str(self._key) + " " + _print_str(self._val) + "]"

    def __repr__(self):
        return self.__str__()


cdef class _MapEntrySeq(ASeq):
    """Seq view of a MapEntry — yields key, then val."""

    cdef object _entry
    cdef int _i

    def __cinit__(self, entry=None, i=0):
        if entry is None:
            return
        self._entry = entry
        self._i = i

    def first(self):
        return (<MapEntry>self._entry).nth(self._i)

    def next(self):
        if self._i < 1:
            return _MapEntrySeq(self._entry, self._i + 1)
        return None

    def count(self):
        return 2 - self._i

    def with_meta(self, meta):
        cdef _MapEntrySeq s = _MapEntrySeq(self._entry, self._i)
        s._meta = meta
        return s


IMapEntry.register(MapEntry)
Indexed.register(MapEntry)
Counted.register(MapEntry)
Sequential.register(MapEntry)
IFn.register(MapEntry)
IHashEq.register(MapEntry)
IMeta.register(MapEntry)
IObj.register(MapEntry)
ILookup.register(MapEntry)
Associative.register(MapEntry)
