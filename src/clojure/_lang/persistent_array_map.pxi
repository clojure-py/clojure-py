# Port of clojure.lang.PersistentArrayMap.
#
# A persistent map backed by a flat list [k0, v0, k1, v1, ...]. Lookup is
# O(n), so it's only appropriate for small maps. JVM Clojure spillovers to
# PersistentHashMap when the array reaches 16 entries (8 KV pairs); we let
# the array map grow without spillover until PersistentHashMap lands.
# Performance degrades but correctness is preserved.

# Keyword keys take a fast identity-comparison path (interned keywords have
# unique identity).


cdef int _ARRAY_MAP_HT_THRESHOLD = 16  # records the JVM spillover threshold; not enforced here


cdef bint _equal_key(k1, k2):
    # Fast-path identity for Keyword keys (which are interned).
    if isinstance(k1, Keyword):
        return k1 is k2
    return Util.equiv(k1, k2)


cdef int _pam_index_of(list array, object key):
    cdef int n = len(array)
    cdef int i
    if isinstance(key, Keyword):
        for i in range(0, n, 2):
            if key is array[i]:
                return i
        return -1
    for i in range(0, n, 2):
        if Util.equiv(key, array[i]):
            return i
    return -1


cdef PersistentArrayMap _make_pam(list array, object meta):
    cdef PersistentArrayMap m = PersistentArrayMap.__new__(PersistentArrayMap)
    m._array = array
    m._meta = meta
    return m


cdef class PersistentArrayMap:
    """A persistent map backed by a flat [k0, v0, k1, v1, ...] array."""

    cdef list _array
    cdef object _meta
    cdef int32_t _hash_cache
    cdef int32_t _hasheq_cache
    cdef object __weakref__

    def __cinit__(self):
        self._array = []

    # --- factories ---

    @staticmethod
    def create(*args):
        """PersistentArrayMap.create(k1, v1, k2, v2, ...) or .create(dict)."""
        if len(args) == 1 and isinstance(args[0], dict):
            d = args[0]
            arr = []
            for k, v in d.items():
                arr.append(k)
                arr.append(v)
            return _make_pam(arr, None)
        if len(args) % 2 != 0:
            raise ValueError("PersistentArrayMap.create requires alternating key/value args")
        return _make_pam(list(args), None)

    @staticmethod
    def create_with_check(items):
        """Build from an iterable of [k1 v1 k2 v2 ...]; raises on duplicate keys."""
        arr = list(items)
        if len(arr) % 2 != 0:
            raise ValueError("createWithCheck: items must come in pairs")
        cdef int i, j
        for i in range(0, len(arr), 2):
            for j in range(i + 2, len(arr), 2):
                if _equal_key(arr[i], arr[j]):
                    raise ValueError(f"Duplicate key: {arr[i]!r}")
        return _make_pam(arr, None)

    @staticmethod
    def create_as_if_by_assoc(items):
        """Like create_with_check, but de-dups by overwriting (mirrors
        Java's createAsIfByAssoc — preserves first occurrence's key, last
        occurrence's value)."""
        arr = list(items)
        if len(arr) % 2 != 0:
            raise ValueError("createAsIfByAssoc: items must come in pairs")
        result = []
        cdef int i, j, found_idx
        for i in range(0, len(arr), 2):
            k = arr[i]
            v = arr[i + 1]
            found_idx = -1
            for j in range(0, len(result), 2):
                if _equal_key(k, result[j]):
                    found_idx = j
                    break
            if found_idx >= 0:
                result[found_idx + 1] = v
            else:
                result.append(k)
                result.append(v)
        return _make_pam(result, None)

    # --- IPersistentMap / Associative / ILookup ---

    def count(self):
        return len(self._array) // 2

    def __len__(self):
        return len(self._array) // 2

    def contains_key(self, key):
        return _pam_index_of(self._array, key) >= 0

    def entry_at(self, key):
        cdef int i = _pam_index_of(self._array, key)
        if i >= 0:
            return MapEntry(self._array[i], self._array[i + 1])
        return None

    def assoc(self, key, val):
        cdef int i = _pam_index_of(self._array, key)
        cdef list new_arr
        if i >= 0:
            if self._array[i + 1] is val or self._array[i + 1] == val:
                return self
            new_arr = list(self._array)
            new_arr[i + 1] = val
            return _make_pam(new_arr, self._meta)
        # Not present — grow. JVM Clojure spillovers to PersistentHashMap when
        # the array reaches 16 entries (8 KV pairs).
        if len(self._array) >= _ARRAY_MAP_HT_THRESHOLD:
            return _phm_from_pam_array(self._array, self._meta).assoc(key, val)
        new_arr = list(self._array)
        new_arr.append(key)
        new_arr.append(val)
        return _make_pam(new_arr, self._meta)

    def assoc_ex(self, key, val):
        cdef int i = _pam_index_of(self._array, key)
        cdef list new_arr
        if i >= 0:
            raise ValueError(f"Key already present: {key!r}")
        if len(self._array) >= _ARRAY_MAP_HT_THRESHOLD:
            return _phm_from_pam_array(self._array, self._meta).assoc_ex(key, val)
        new_arr = list(self._array)
        new_arr.append(key)
        new_arr.append(val)
        return _make_pam(new_arr, self._meta)

    def without(self, key):
        cdef int i = _pam_index_of(self._array, key)
        cdef int new_len
        cdef list new_arr
        if i < 0:
            return self
        new_len = len(self._array) - 2
        if new_len == 0:
            return self.empty()
        new_arr = self._array[:i] + self._array[i + 2:]
        return _make_pam(new_arr, self._meta)

    def cons(self, o):
        # Conj on a map: accepts map entries, [k v] pairs, or seq-of-entries.
        if o is None:
            return self
        if isinstance(o, MapEntry):
            return self.assoc((<MapEntry>o)._key, (<MapEntry>o)._val)
        if isinstance(o, IMapEntry):
            return self.assoc(o.key(), o.val())
        if isinstance(o, IPersistentVector):
            if o.count() != 2:
                raise ValueError("Vector arg to map conj must be a pair")
            return self.assoc(o.nth(0), o.nth(1))
        if isinstance(o, (list, tuple)) and len(o) == 2:
            return self.assoc(o[0], o[1])
        # Sequence of pair-like entries.
        ret = self
        for e in o:
            ret = ret.cons(e)
        return ret

    def empty(self):
        if self._meta is None:
            return _PAM_EMPTY
        return _make_pam([], self._meta)

    def val_at(self, key, not_found=NOT_FOUND):
        cdef int i = _pam_index_of(self._array, key)
        if i >= 0:
            return self._array[i + 1]
        return None if not_found is NOT_FOUND else not_found

    # --- equality / hash ---

    def equiv(self, other):
        if other is self:
            return True
        return _pam_equiv(self, other)

    def __eq__(self, other):
        if other is self:
            return True
        return _pam_equiv(self, other)

    def __ne__(self, other):
        return not self.__eq__(other)

    def __hash__(self):
        if self._hash_cache != 0:
            return self._hash_cache
        # Java AbstractMap.hashCode: sum(entry.hashCode), entry hash = k.hashCode XOR v.hashCode.
        cdef int32_t h = 0
        cdef int i
        for i in range(0, len(self._array), 2):
            k = self._array[i]
            v = self._array[i + 1]
            kh = 0 if k is None else hash(k)
            vh = 0 if v is None else hash(v)
            h = _to_int32_mask(<long long>h + (<long long>kh ^ <long long>vh))
        self._hash_cache = h
        return h

    def hasheq(self):
        if self._hasheq_cache != 0:
            return self._hasheq_cache
        # Maps hash unordered (entry order doesn't affect equality).
        result = Murmur3.hash_unordered(self._iter_entries())
        self._hasheq_cache = result
        return result

    def _iter_entries(self):
        cdef int i
        for i in range(0, len(self._array), 2):
            yield MapEntry(self._array[i], self._array[i + 1])

    # --- IKVReduce / IDrop ---

    def kv_reduce(self, f, init):
        cdef int i
        for i in range(0, len(self._array), 2):
            init = f(init, self._array[i], self._array[i + 1])
            if isinstance(init, Reduced):
                return (<Reduced>init).deref()
        return init

    def drop(self, n):
        if len(self._array) > 0:
            s = self.seq()
            return s.drop(n)
        return None

    # --- seq ---

    def seq(self):
        if len(self._array) > 0:
            return _PAMSeq(self._array, 0)
        return None

    # --- Python protocols ---

    def __iter__(self):
        cdef int i
        for i in range(0, len(self._array), 2):
            yield MapEntry(self._array[i], self._array[i + 1])

    def keys(self):
        cdef int i
        for i in range(0, len(self._array), 2):
            yield self._array[i]

    def values(self):
        cdef int i
        for i in range(0, len(self._array), 2):
            yield self._array[i + 1]

    def __contains__(self, key):
        return self.contains_key(key)

    def __getitem__(self, key):
        cdef int i = _pam_index_of(self._array, key)
        if i >= 0:
            return self._array[i + 1]
        raise KeyError(key)

    def __call__(self, key, not_found=NOT_FOUND):
        return self.val_at(key, not_found)

    def __bool__(self):
        return len(self._array) > 0

    def meta(self):
        return self._meta

    def with_meta(self, meta):
        if self._meta is meta:
            return self
        return _make_pam(self._array, meta)

    def __str__(self):
        parts = []
        cdef int i
        for i in range(0, len(self._array), 2):
            parts.append(_print_str(self._array[i]) + " " + _print_str(self._array[i + 1]))
        return "{" + ", ".join(parts) + "}"

    def __repr__(self):
        return self.__str__()

    # --- IEditableCollection ---

    def as_transient(self):
        return TransientArrayMap._from_persistent(self._array)


cdef bint _pam_equiv(PersistentArrayMap a, object other):
    cdef int i
    if isinstance(other, PersistentArrayMap):
        b = <PersistentArrayMap>other
        if len(a._array) != len(b._array):
            return False
        # Walk a's entries and check each in b.
        for i in range(0, len(a._array), 2):
            k = a._array[i]
            v = a._array[i + 1]
            other_idx = _pam_index_of(b._array, k)
            if other_idx < 0:
                return False
            if not Util.equiv(v, b._array[other_idx + 1]):
                return False
        return True
    if isinstance(other, IPersistentMap):
        if other.count() != len(a._array) // 2:
            return False
        for i in range(0, len(a._array), 2):
            k = a._array[i]
            v = a._array[i + 1]
            if not other.contains_key(k):
                return False
            if not Util.equiv(v, other.val_at(k)):
                return False
        return True
    if isinstance(other, dict):
        if len(other) != len(a._array) // 2:
            return False
        for i in range(0, len(a._array), 2):
            k = a._array[i]
            v = a._array[i + 1]
            if k not in other:
                return False
            if not Util.equiv(v, other[k]):
                return False
        return True
    return False


# --- ABC registration ---
IPersistentMap.register(PersistentArrayMap)
Associative.register(PersistentArrayMap)
ILookup.register(PersistentArrayMap)
IPersistentCollection.register(PersistentArrayMap)
Counted.register(PersistentArrayMap)
IFn.register(PersistentArrayMap)
IHashEq.register(PersistentArrayMap)
IMeta.register(PersistentArrayMap)
IObj.register(PersistentArrayMap)
IKVReduce.register(PersistentArrayMap)
IDrop.register(PersistentArrayMap)
IEditableCollection.register(PersistentArrayMap)


# ---------- internal Seq ----------

cdef class _PAMSeq(ASeq):
    """Seq view of a PersistentArrayMap — yields MapEntries."""

    cdef list _array
    cdef int _i

    def __cinit__(self, array=None, i=0):
        if array is None:
            return
        self._array = array
        self._i = i

    def first(self):
        return MapEntry(self._array[self._i], self._array[self._i + 1])

    def next(self):
        if self._i + 2 < len(self._array):
            return _PAMSeq(self._array, self._i + 2)
        return None

    def count(self):
        return (len(self._array) - self._i) // 2

    def drop(self, int n):
        if n < self.count():
            return _PAMSeq(self._array, self._i + 2 * n)
        return None

    def with_meta(self, meta):
        cdef _PAMSeq s = _PAMSeq(self._array, self._i)
        s._meta = meta
        return s

    def reduce(self, f, start=NOT_FOUND):
        cdef int j
        cdef object acc
        if start is NOT_FOUND:
            if self._i < len(self._array):
                acc = MapEntry(self._array[self._i], self._array[self._i + 1])
                for j in range(self._i + 2, len(self._array), 2):
                    acc = f(acc, MapEntry(self._array[j], self._array[j + 1]))
                    if isinstance(acc, Reduced):
                        return (<Reduced>acc).deref()
                return acc
            return f()
        else:
            acc = start
            for j in range(self._i, len(self._array), 2):
                acc = f(acc, MapEntry(self._array[j], self._array[j + 1]))
                if isinstance(acc, Reduced):
                    return (<Reduced>acc).deref()
            return acc


IReduce.register(_PAMSeq)
IReduceInit.register(_PAMSeq)
IDrop.register(_PAMSeq)
Counted.register(_PAMSeq)


# ---------- TransientArrayMap ----------

cdef class TransientArrayMap:
    """The mutable companion to PersistentArrayMap. JVM uses an explicit
    Thread owner field; we use the same single-shot transition pattern as
    TransientVector."""

    cdef list _array
    cdef int _len
    cdef object _owner
    cdef object __weakref__

    def __cinit__(self):
        self._array = []
        self._len = 0
        self._owner = None

    @staticmethod
    cdef TransientArrayMap _from_persistent(list array):
        cdef TransientArrayMap t = TransientArrayMap.__new__(TransientArrayMap)
        cdef int alloc_size = max(_ARRAY_MAP_HT_THRESHOLD, len(array))
        t._array = list(array) + [None] * (alloc_size - len(array))
        t._len = len(array)
        t._owner = _threading.current_thread()
        return t

    cdef void _ensure_editable(self) except *:
        if self._owner is None:
            raise RuntimeError("Transient used after persistent! call")

    cdef int _index_of(self, key):
        cdef int i
        for i in range(0, self._len, 2):
            if _equal_key(self._array[i], key):
                return i
        return -1

    def assoc(self, key, val):
        self._ensure_editable()
        cdef int i = self._index_of(key)
        if i >= 0:
            if self._array[i + 1] is not val and self._array[i + 1] != val:
                self._array[i + 1] = val
            return self
        # JVM spillovers to TransientHashMap once the array hits the threshold.
        if self._len >= _ARRAY_MAP_HT_THRESHOLD:
            t = _phm_from_pam_array(list(self._array[:self._len]), None).as_transient()
            t.assoc(key, val)
            self._owner = None  # invalidate this transient
            return t
        # Grow buffer if needed.
        if self._len >= len(self._array):
            self._array += [None] * 16
        self._array[self._len] = key
        self._array[self._len + 1] = val
        self._len += 2
        return self

    def without(self, key):
        self._ensure_editable()
        cdef int i = self._index_of(key)
        if i >= 0:
            if self._len >= 2:
                self._array[i] = self._array[self._len - 2]
                self._array[i + 1] = self._array[self._len - 1]
            self._len -= 2
        return self

    def val_at(self, key, not_found=NOT_FOUND):
        self._ensure_editable()
        cdef int i = self._index_of(key)
        if i >= 0:
            return self._array[i + 1]
        return None if not_found is NOT_FOUND else not_found

    def contains_key(self, key):
        self._ensure_editable()
        return self._index_of(key) >= 0

    def entry_at(self, key):
        self._ensure_editable()
        cdef int i = self._index_of(key)
        if i >= 0:
            return MapEntry(self._array[i], self._array[i + 1])
        return None

    def count(self):
        self._ensure_editable()
        return self._len // 2

    def __len__(self):
        return self.count()

    def conj(self, o):
        self._ensure_editable()
        if o is None:
            return self
        if isinstance(o, MapEntry):
            return self.assoc((<MapEntry>o)._key, (<MapEntry>o)._val)
        if isinstance(o, IMapEntry):
            return self.assoc(o.key(), o.val())
        if isinstance(o, IPersistentVector):
            if o.count() != 2:
                raise ValueError("Vector arg to map conj must be a pair")
            return self.assoc(o.nth(0), o.nth(1))
        if isinstance(o, (list, tuple)) and len(o) == 2:
            return self.assoc(o[0], o[1])
        for e in o:
            self.conj(e)
        return self

    def persistent(self):
        self._ensure_editable()
        self._owner = None
        return _make_pam(list(self._array[:self._len]), None)

    def __call__(self, key, not_found=NOT_FOUND):
        return self.val_at(key, not_found)


ITransientMap.register(TransientArrayMap)
ITransientAssociative.register(TransientArrayMap)
ITransientAssociative2.register(TransientArrayMap)
ITransientCollection.register(TransientArrayMap)
Counted.register(TransientArrayMap)
ILookup.register(TransientArrayMap)
IFn.register(TransientArrayMap)


# ---------- the singleton EMPTY ----------

cdef PersistentArrayMap _PAM_EMPTY = _make_pam([], None)
PERSISTENT_ARRAY_MAP_EMPTY = _PAM_EMPTY


# ---------- HashMap spillover helper ----------

cdef object _phm_from_pam_array(list array, object meta):
    """Build a PersistentHashMap from a flat [k,v,k,v,...] array. Used when
    PersistentArrayMap (or its transient) outgrows the array threshold."""
    t = PERSISTENT_HASH_MAP_EMPTY.as_transient()
    cdef int i
    for i in range(0, len(array), 2):
        t.assoc(array[i], array[i + 1])
    cdef PersistentHashMap result = t.persistent()
    if meta is None:
        return result
    return result.with_meta(meta)
