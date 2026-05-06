# RecordBase — base class for defrecord-generated types.
#
# Each subclass sets:
#   _record_fields = ('a', 'b', 'c')   # tuple of field name strings
#   __init__ (positional fields → instance attrs of those names)
#
# RecordBase itself implements the IPersistentMap surface so a record
# acts like a map: (get rec :a) returns the field; (assoc rec :a v)
# returns a new instance of the same record class with that field
# updated; (assoc rec :other v) goes to the per-instance _extmap dict;
# (dissoc rec :a) returns a regular map (since records can't have
# nil-valued fields).
#
# Equality is class-and-value based: two records of the same record
# type with the same fields and same extmap are =. Hash combines the
# record class + field values.
#
# JVM stores __meta and __extmap as fields on the record class (with
# special names). We use plain instance attrs `_meta` and `_extmap`.
# Field names beginning with an underscore are reserved.


cdef object _NO_DEFAULT = object()


def _record_make_seq(rec):
    """Build a Clojure list of MapEntry pairs from a record's fields +
    extmap. Returns None when both are empty."""
    cdef list pairs = []
    cdef dict extmap
    for fname in rec._record_fields:
        kw = Keyword.intern(fname)
        pairs.append(MapEntry(kw, getattr(rec, fname)))
    extmap = rec._extmap
    if extmap:
        for k, v in extmap.items():
            pairs.append(MapEntry(k, v))
    if not pairs:
        return None
    # Build a PersistentList from the pairs, last to first.
    cdef object out = None
    for entry in reversed(pairs):
        out = Cons(entry, out)
    return out


def _record_set_state(self, vals):
    """Common __init__ helper: assign field values from an iterable
    of positional args, then initialize _meta to None and _extmap
    to {}.

    Takes `vals` as a single iterable so the Clojure side can pass
    a seq directly without splatting (Clojure interop doesn't have
    a clean way to spread a seq into Python *args)."""
    cdef tuple fields = type(self)._record_fields
    cdef list vals_list = list(vals)
    if len(vals_list) != len(fields):
        raise TypeError(
            f"{type(self).__name__} takes {len(fields)} field args, got {len(vals_list)}")
    for fname, val in zip(fields, vals_list):
        object.__setattr__(self, fname, val)
    object.__setattr__(self, "_meta", None)
    object.__setattr__(self, "_extmap", {})
    object.__setattr__(self, "_hash_cache", 0)


def _record_with_state(cls, fields, meta, extmap):
    """Build a fresh instance of `cls` with the given field values and
    a specific _meta + _extmap. Used by assoc / without / with-meta to
    return a new record sharing the same record-type."""
    inst = cls(*fields)
    object.__setattr__(inst, "_meta", meta)
    object.__setattr__(inst, "_extmap", dict(extmap))
    object.__setattr__(inst, "_hash_cache", 0)
    return inst


class RecordBase:
    """Base class for defrecord types. See module docstring."""

    _record_fields = ()  # subclass overrides

    # --- construction helpers ---

    def __record_init__(self, vals):
        """Called from a subclass's __init__ to wire up fields / meta /
        extmap. `vals` is an iterable of positional field values."""
        _record_set_state(self, vals)

    # --- ILookup ---

    def val_at(self, k, not_found=None):
        if isinstance(k, Keyword) and k.get_name() in self._record_fields:
            return getattr(self, k.get_name())
        if self._extmap:
            return self._extmap.get(k, not_found)
        return not_found

    # --- Associative ---

    def contains_key(self, k):
        if isinstance(k, Keyword) and k.get_name() in self._record_fields:
            return True
        return k in self._extmap

    def entry_at(self, k):
        if isinstance(k, Keyword) and k.get_name() in self._record_fields:
            return MapEntry(k, getattr(self, k.get_name()))
        if k in self._extmap:
            return MapEntry(k, self._extmap[k])
        return None

    def assoc(self, k, v):
        cls = type(self)
        if isinstance(k, Keyword) and k.get_name() in self._record_fields:
            name = k.get_name()
            new_vals = tuple(
                v if f == name else getattr(self, f)
                for f in self._record_fields)
            return _record_with_state(cls, new_vals, self._meta, self._extmap)
        new_extmap = dict(self._extmap)
        new_extmap[k] = v
        same_vals = tuple(getattr(self, f) for f in self._record_fields)
        return _record_with_state(cls, same_vals, self._meta, new_extmap)

    def assoc_ex(self, k, v):
        if self.contains_key(k):
            raise ValueError(f"Key already present: {k!r}")
        return self.assoc(k, v)

    # --- IPersistentMap ---

    def without(self, k):
        cls = type(self)
        if isinstance(k, Keyword) and k.get_name() in self._record_fields:
            # Records can't have missing fields; convert to plain map.
            from clojure.lang import PersistentArrayMap
            kvs = []
            for fname in self._record_fields:
                fkw = Keyword.intern(fname)
                if fkw is k:
                    continue
                kvs.append(fkw)
                kvs.append(getattr(self, fname))
            for kk, vv in self._extmap.items():
                kvs.append(kk)
                kvs.append(vv)
            return (PersistentArrayMap.create(*kvs)
                    .with_meta(self._meta))
        if k in self._extmap:
            new_extmap = {kk: vv for kk, vv in self._extmap.items() if kk != k}
            same_vals = tuple(getattr(self, f) for f in self._record_fields)
            return _record_with_state(cls, same_vals, self._meta, new_extmap)
        return self

    # --- Counted ---

    def count(self):
        return len(self._record_fields) + len(self._extmap)

    # --- Seqable ---

    def seq(self):
        return _record_make_seq(self)

    # --- IPersistentCollection ---

    def cons(self, o):
        if isinstance(o, MapEntry):
            return self.assoc(o.key(), o.val())
        if isinstance(o, IPersistentVector):
            if o.count() != 2:
                raise ValueError(
                    "Vector arg to record conj must be a pair")
            return self.assoc(o.nth(0), o.nth(1))
        # Iterate as a seq of pairs.
        result = self
        s = o.seq() if isinstance(o, Seqable) else (o if o is None else iter(o))
        if isinstance(s, ISeq):
            cur = s
            while cur is not None:
                pair = cur.first()
                if isinstance(pair, MapEntry):
                    result = result.assoc(pair.key(), pair.val())
                else:
                    result = result.assoc(pair.nth(0), pair.nth(1))
                cur = cur.next()
        elif s is not None:
            for pair in s:
                if isinstance(pair, MapEntry):
                    result = result.assoc(pair.key(), pair.val())
                elif isinstance(pair, IPersistentVector):
                    result = result.assoc(pair.nth(0), pair.nth(1))
                else:
                    result = result.assoc(pair[0], pair[1])
        return result

    def empty(self):
        raise NotImplementedError(
            f"Can't create empty: {type(self).__name__}")

    def equiv(self, other):
        return self.__eq__(other)

    # --- IObj / IMeta ---

    def meta(self):
        return self._meta

    def with_meta(self, meta):
        if meta is self._meta:
            return self
        cls = type(self)
        same_vals = tuple(getattr(self, f) for f in self._record_fields)
        return _record_with_state(cls, same_vals, meta, self._extmap)

    # --- IHashEq ---

    def hasheq(self):
        return self.__hash__()

    # --- Python equality / hash / iter ---

    def __eq__(self, other):
        if self is other:
            return True
        if type(self) is not type(other):
            return False
        for f in self._record_fields:
            if not Util.equiv(getattr(self, f), getattr(other, f)):
                return False
        return self._extmap == other._extmap

    def __ne__(self, other):
        return not self.__eq__(other)

    def __hash__(self):
        cdef object cached = self._hash_cache
        if cached:
            return cached
        h = hash(type(self).__name__)
        for f in self._record_fields:
            h = (h * 31) ^ Util.hasheq(getattr(self, f))
        for k, v in self._extmap.items():
            h ^= Util.hasheq(k) ^ Util.hasheq(v)
        h = _to_int32_mask(h)
        object.__setattr__(self, "_hash_cache", h)
        return h

    def __iter__(self):
        s = self.seq()
        while s is not None:
            yield s.first()
            s = s.next()

    def __len__(self):
        return self.count()

    def __contains__(self, k):
        return self.contains_key(k)

    def __getitem__(self, k):
        # Python `rec[k]` — raise KeyError on missing.
        v = self.val_at(k, _NO_DEFAULT)
        if v is _NO_DEFAULT:
            raise KeyError(k)
        return v

    def __repr__(self):
        cls = type(self).__name__
        parts = [f"{f}={getattr(self, f)!r}" for f in self._record_fields]
        if self._extmap:
            parts.append(f"_extmap={self._extmap!r}")
        return f"#{cls}{{{', '.join(parts)}}}"


# ABC registrations — record instances satisfy all of these.
IRecord.register(RecordBase)
ILookup.register(RecordBase)
Associative.register(RecordBase)
IPersistentMap.register(RecordBase)
IPersistentCollection.register(RecordBase)
Counted.register(RecordBase)
Seqable.register(RecordBase)
IObj.register(RecordBase)
IMeta.register(RecordBase)
IHashEq.register(RecordBase)
MapEquivalence.register(RecordBase)
