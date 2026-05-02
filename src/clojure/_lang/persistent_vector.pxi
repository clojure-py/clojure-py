# Port of clojure.lang.PersistentVector — the 32-way trie with a tail
# optimization. Includes the inner Node, the TransientVector mutable variant,
# the chunked seq view (ChunkedSeq), and the reverse seq view (RSeq).
#
# Implementation notes:
#   - The 32-element internal arrays are Python lists. Cython generates good
#     code for indexed list reads/writes, and list.copy()/slicing is fast.
#   - The "edit" identity trick: each transient creation gets a unique _Edit
#     object (a Python class with `is`-identity). Nodes shared with the
#     spawning persistent retain the persistent's _NOEDIT identity; nodes
#     freshly cloned for the transient get the transient's _Edit. Mutate-in-
#     place is allowed only on nodes whose edit IS the transient's edit.
#   - On persistent! the transient's edit.thread is set to None, which
#     poisons further transient operations.
#   - Free-threading: under 3.14t, an _Edit's `thread` attr access is atomic
#     for object refs. The Java version uses AtomicReference; we don't need
#     CAS since the only mutation is "set to None", which is a one-shot
#     transition.


cdef class _Edit:
    """Per-transient ownership token. Identity ('is') determines if a Node is
    owned by the transient that holds this edit."""

    cdef public object thread
    cdef object __weakref__

    def __cinit__(self, thread):
        self.thread = thread


cdef class _PVNode:
    """Internal trie node. Holds a 32-element array and an edit token."""

    cdef _Edit edit
    cdef list array
    cdef object __weakref__

    def __cinit__(self, _Edit edit, list array=None):
        self.edit = edit
        self.array = [None] * 32 if array is None else array


cdef _Edit _NOEDIT = _Edit(None)
cdef _PVNode _EMPTY_NODE = _PVNode(_NOEDIT, [None] * 32)


# ---------- helpers ----------

cdef PersistentVector _make_pv(int cnt, int shift, _PVNode root, list tail, object meta):
    cdef PersistentVector pv = PersistentVector.__new__(PersistentVector)
    pv._cnt = cnt
    pv._shift = shift
    pv._root = root
    pv._tail = tail
    pv._meta = meta
    return pv


cdef int _tailoff_for(int cnt) noexcept:
    if cnt < 32:
        return 0
    return ((cnt - 1) >> 5) << 5


cdef _PVNode _new_path(_Edit edit, int level, _PVNode node):
    if level == 0:
        return node
    cdef _PVNode ret = _PVNode(edit)
    ret.array[0] = _new_path(edit, level - 5, node)
    return ret


cdef _PVNode _push_tail(int cnt, _Edit root_edit, int level, _PVNode parent, _PVNode tailnode):
    cdef int subidx = ((cnt - 1) >> level) & 0x1f
    cdef _PVNode ret = _PVNode(parent.edit, list(parent.array))
    cdef _PVNode child
    cdef _PVNode node_to_insert
    if level == 5:
        node_to_insert = tailnode
    else:
        child = parent.array[subidx]
        if child is not None:
            node_to_insert = _push_tail(cnt, root_edit, level - 5, child, tailnode)
        else:
            node_to_insert = _new_path(root_edit, level - 5, tailnode)
    ret.array[subidx] = node_to_insert
    return ret


cdef _PVNode _do_assoc(int level, _PVNode node, int i, object val):
    cdef _PVNode ret = _PVNode(node.edit, list(node.array))
    cdef int subidx
    if level == 0:
        ret.array[i & 0x1f] = val
    else:
        subidx = (i >> level) & 0x1f
        ret.array[subidx] = _do_assoc(level - 5, <_PVNode>node.array[subidx], i, val)
    return ret


cdef _PVNode _pop_tail(int cnt, _Edit root_edit, int level, _PVNode node):
    cdef int subidx = ((cnt - 2) >> level) & 0x1f
    cdef _PVNode newchild
    cdef _PVNode ret
    if level > 5:
        newchild = _pop_tail(cnt, root_edit, level - 5, <_PVNode>node.array[subidx])
        if newchild is None and subidx == 0:
            return None
        ret = _PVNode(root_edit, list(node.array))
        ret.array[subidx] = newchild
        return ret
    if subidx == 0:
        return None
    ret = _PVNode(root_edit, list(node.array))
    ret.array[subidx] = None
    return ret


# ---------- PersistentVector ----------

cdef class PersistentVector:
    """An immutable, indexed, persistent vector. 32-way trie + tail."""

    cdef readonly int _cnt
    cdef readonly int _shift
    cdef _PVNode _root
    cdef list _tail
    cdef object _meta
    cdef int32_t _hash_cache
    cdef int32_t _hasheq_cache
    cdef object __weakref__

    def __cinit__(self):
        # Default no-op; construction goes through _make_pv / create / etc.
        pass

    @staticmethod
    def create(*items):
        """PersistentVector.create(a, b, c, ...) → vector of those items."""
        return _pv_from_iter(items)

    @staticmethod
    def from_iterable(iterable):
        """PersistentVector.from_iterable(iter) — build from any iterable."""
        return _pv_from_iter(iterable)

    @staticmethod
    def adopt(list items):
        """Wrap an existing 0..32-element list as a vector. Caller MUST NOT
        mutate the list afterwards. Mirrors Java's PersistentVector.adopt."""
        if len(items) > 32:
            return _pv_from_iter(items)
        return _make_pv(len(items), 5, _EMPTY_NODE, list(items), None)

    # --- counts / indexing ---

    def count(self):
        return self._cnt

    def length(self):
        return self._cnt

    def __len__(self):
        return self._cnt

    cdef int _tailoff(self) noexcept:
        return _tailoff_for(self._cnt)

    cdef list _array_for(self, int i):
        cdef _PVNode node
        cdef int level
        if i >= 0 and i < self._cnt:
            if i >= self._tailoff():
                return self._tail
            node = self._root
            level = self._shift
            while level > 0:
                node = <_PVNode>node.array[(i >> level) & 0x1f]
                level -= 5
            return node.array
        raise IndexError(i)

    def array_for(self, int i):
        return self._array_for(i)

    def nth(self, int i, not_found=NOT_FOUND):
        if 0 <= i < self._cnt:
            return self._array_for(i)[i & 0x1f]
        if not_found is NOT_FOUND:
            raise IndexError(i)
        return not_found

    def __getitem__(self, i):
        if isinstance(i, slice):
            # Slice access returns a Python list (concrete, not lazy). Matches
            # what users typically expect from indexable Python sequences.
            return [self.nth(j) for j in range(*i.indices(self._cnt))]
        if isinstance(i, int) and not isinstance(i, bool):
            if i < 0:
                i += self._cnt
            return self.nth(i)
        raise TypeError(f"vector indices must be integers, got {type(i).__name__}")

    def assoc_n(self, int i, val):
        cdef list new_tail
        if 0 <= i < self._cnt:
            if i >= self._tailoff():
                new_tail = list(self._tail)
                new_tail[i & 0x1f] = val
                return _make_pv(self._cnt, self._shift, self._root, new_tail, self._meta)
            return _make_pv(self._cnt, self._shift,
                            _do_assoc(self._shift, self._root, i, val),
                            self._tail, self._meta)
        if i == self._cnt:
            return self.cons(val)
        raise IndexError(i)

    def assoc(self, key, val):
        # Associative.assoc — treats key as int index.
        if isinstance(key, int) and not isinstance(key, bool):
            return self.assoc_n(key, val)
        raise TypeError("Vector key must be integer")

    def val_at(self, key, not_found=NOT_FOUND):
        # ILookup.val_at on a vector treats key as int index. Out-of-range
        # returns not_found (or None when not_found is NOT_FOUND).
        if isinstance(key, int) and not isinstance(key, bool):
            if 0 <= key < self._cnt:
                return self._array_for(key)[key & 0x1f]
        return None if not_found is NOT_FOUND else not_found

    def contains_key(self, key):
        return (isinstance(key, int) and not isinstance(key, bool)
                and 0 <= key < self._cnt)

    def entry_at(self, key):
        if (isinstance(key, int) and not isinstance(key, bool)
                and 0 <= key < self._cnt):
            return (key, self.nth(key))
        return None

    # --- cons / append ---

    def cons(self, val):
        cdef list new_tail
        cdef _PVNode tailnode, newroot
        cdef int newshift
        # Tail has room?
        if self._cnt - self._tailoff() < 32:
            new_tail = list(self._tail)
            new_tail.append(val)
            return _make_pv(self._cnt + 1, self._shift, self._root, new_tail, self._meta)
        # Tail full → push it into the tree, start a new tail with [val].
        tailnode = _PVNode(self._root.edit, list(self._tail))
        newshift = self._shift
        # Overflow root?
        if (self._cnt >> 5) > (1 << self._shift):
            newroot = _PVNode(self._root.edit)
            newroot.array[0] = self._root
            newroot.array[1] = _new_path(self._root.edit, self._shift, tailnode)
            newshift += 5
        else:
            newroot = _push_tail(self._cnt, self._root.edit, self._shift, self._root, tailnode)
        return _make_pv(self._cnt + 1, newshift, newroot, [val], self._meta)

    # --- pop / peek ---

    def pop(self):
        cdef list newtail
        cdef _PVNode newroot
        cdef int newshift
        if self._cnt == 0:
            raise IndexError("Can't pop empty vector")
        if self._cnt == 1:
            return _make_pv(0, 5, _EMPTY_NODE, [], self._meta)
        # Tail has more than one element? Just shrink it.
        if self._cnt - self._tailoff() > 1:
            newtail = list(self._tail)
            newtail.pop()
            return _make_pv(self._cnt - 1, self._shift, self._root, newtail, self._meta)
        # Tail had a single element; promote a leaf out of the tree as new tail.
        newtail = list(self._array_for(self._cnt - 2))
        newroot = _pop_tail(self._cnt, self._root.edit, self._shift, self._root)
        newshift = self._shift
        if newroot is None:
            newroot = _EMPTY_NODE
        if self._shift > 5 and newroot.array[1] is None:
            newroot = <_PVNode>newroot.array[0]
            newshift -= 5
        return _make_pv(self._cnt - 1, newshift, newroot, newtail, self._meta)

    def peek(self):
        if self._cnt > 0:
            return self.nth(self._cnt - 1)
        return None

    # --- empty / meta ---

    def empty(self):
        if self._meta is None:
            return _PV_EMPTY
        return _make_pv(0, 5, _EMPTY_NODE, [], self._meta)

    def meta(self):
        return self._meta

    def with_meta(self, meta):
        if self._meta is meta:
            return self
        return _make_pv(self._cnt, self._shift, self._root, self._tail, meta)

    # --- equality / hash ---

    def equiv(self, other):
        if other is self:
            return True
        if isinstance(other, PersistentVector):
            return _pv_equiv_pv(self, other, Util.equiv)
        if isinstance(other, IPersistentVector):
            return _pv_equiv_ipv(self, other, Util.equiv)
        if isinstance(other, (list, tuple)):
            return _pv_equiv_seqlike(self, other, Util.equiv)
        if isinstance(other, Sequential):
            return _pv_equiv_sequential(self, other, Util.equiv)
        return False

    def __eq__(self, other):
        if other is self:
            return True
        if isinstance(other, PersistentVector):
            return _pv_equiv_pv(self, other, Util.equals)
        if isinstance(other, IPersistentVector):
            return _pv_equiv_ipv(self, other, Util.equals)
        if isinstance(other, (list, tuple)):
            return _pv_equiv_seqlike(self, other, Util.equals)
        if isinstance(other, Sequential):
            return _pv_equiv_sequential(self, other, Util.equals)
        return False

    def __ne__(self, other):
        return not self.__eq__(other)

    def __hash__(self):
        cdef int32_t cached = self._hash_cache
        if cached != 0:
            return cached
        cdef uint32_t h = 1u
        cdef int i
        cdef object x
        for i in range(self._cnt):
            x = self.nth(i)
            h = (31u * h + (0u if x is None else <uint32_t>(<int32_t>hash(x)))) & 0xFFFFFFFFu
        cdef int32_t result = <int32_t>h
        self._hash_cache = result
        return result

    def hasheq(self):
        cdef int32_t cached = self._hasheq_cache
        if cached != 0:
            return cached
        result = Murmur3.hash_ordered(self)
        self._hasheq_cache = result
        return result

    # --- IFn (vectors as functions of their index) ---

    def __call__(self, key):
        if isinstance(key, int) and not isinstance(key, bool):
            if 0 <= key < self._cnt:
                return self.nth(key)
            raise IndexError(key)
        raise TypeError("Vector key must be integer")

    # --- iteration ---

    def __iter__(self):
        cdef int i = 0
        cdef int base = 0
        cdef list array
        if self._cnt == 0:
            return
        array = self._array_for(0)
        while i < self._cnt:
            if i - base == 32:
                array = self._array_for(i)
                base += 32
            yield array[i & 0x1f]
            i += 1

    def __reversed__(self):
        cdef int i
        for i in range(self._cnt - 1, -1, -1):
            yield self.nth(i)

    def __contains__(self, o):
        # Note: vectors implement __contains__ by index range, not value
        # membership. (Clojure: (contains? v 3) checks key-presence, NOT value.)
        # That matches Associative.contains_key for vectors.
        if isinstance(o, int) and not isinstance(o, bool):
            return 0 <= o < self._cnt
        return False

    def __bool__(self):
        return self._cnt > 0

    def __str__(self):
        parts = []
        for x in self:
            parts.append(_print_str(x))
        return "[" + " ".join(parts) + "]"

    def __repr__(self):
        return self.__str__()

    # --- seqs ---

    def seq(self):
        if self._cnt == 0:
            return None
        return _PVChunkedSeq(self, 0, 0)

    def chunked_seq(self):
        return self.seq()

    def rseq(self):
        if self._cnt == 0:
            return None
        return _PVRSeq(self, self._cnt - 1)

    # --- reduce / kvreduce ---

    def reduce(self, f, start=NOT_FOUND):
        cdef int i = 0
        cdef int step = 0
        cdef int j
        cdef list array
        cdef object init
        if start is NOT_FOUND:
            if self._cnt == 0:
                return f()
            init = self._array_for(0)[0]
            i = 0
            step = 0
            # Process first chunk skipping element 0.
            array = self._array_for(0)
            for j in range(1, len(array)):
                init = f(init, array[j])
                if isinstance(init, Reduced):
                    return (<Reduced>init).deref()
            i = len(array)
            step = len(array)
            while i < self._cnt:
                array = self._array_for(i)
                for j in range(len(array)):
                    init = f(init, array[j])
                    if isinstance(init, Reduced):
                        return (<Reduced>init).deref()
                step = len(array)
                i += step
            return init
        else:
            init = start
            i = 0
            while i < self._cnt:
                array = self._array_for(i)
                for j in range(len(array)):
                    init = f(init, array[j])
                    if isinstance(init, Reduced):
                        return (<Reduced>init).deref()
                step = len(array)
                i += step
            return init

    def kv_reduce(self, f, init):
        cdef int i = 0
        cdef int step = 0
        cdef int j
        cdef list array
        while i < self._cnt:
            array = self._array_for(i)
            for j in range(len(array)):
                init = f(init, j + i, array[j])
                if isinstance(init, Reduced):
                    return (<Reduced>init).deref()
            step = len(array)
            i += step
        return init

    # --- IDrop ---

    def drop(self, int n):
        cdef int offset
        if n < self._cnt:
            offset = n % 32
            return _PVChunkedSeq.from_arrays(self, self._array_for(n), n - offset, offset)
        return None

    # --- IEditableCollection ---

    def as_transient(self):
        return TransientVector._from_persistent(self)


IPersistentVector.register(PersistentVector)
IPersistentStack.register(PersistentVector)
Associative.register(PersistentVector)
ILookup.register(PersistentVector)
IPersistentCollection.register(PersistentVector)
Sequential.register(PersistentVector)
Counted.register(PersistentVector)
Indexed.register(PersistentVector)
Reversible.register(PersistentVector)
IFn.register(PersistentVector)
IHashEq.register(PersistentVector)
IMeta.register(PersistentVector)
IObj.register(PersistentVector)
IReduce.register(PersistentVector)
IReduceInit.register(PersistentVector)
IKVReduce.register(PersistentVector)
IDrop.register(PersistentVector)
IEditableCollection.register(PersistentVector)


# ---------- equality helpers (kept module-level so they can be cdef) ----------

cdef bint _pv_equiv_pv(PersistentVector a, PersistentVector b, eq_fn) except *:
    cdef int n = a._cnt
    cdef int i
    if b._cnt != n:
        return False
    for i in range(n):
        if not eq_fn(a.nth(i), b.nth(i)):
            return False
    return True


cdef bint _pv_equiv_ipv(PersistentVector a, object b, eq_fn) except *:
    cdef int n = a._cnt
    cdef int i
    if b.count() != n:
        return False
    for i in range(n):
        if not eq_fn(a.nth(i), b.nth(i)):
            return False
    return True


cdef bint _pv_equiv_seqlike(PersistentVector a, object b, eq_fn) except *:
    cdef int n = a._cnt
    cdef int i
    if len(b) != n:
        return False
    for i in range(n):
        if not eq_fn(a.nth(i), b[i]):
            return False
    return True


cdef bint _pv_equiv_sequential(PersistentVector a, object b, eq_fn) except *:
    # Use Python iter() — both ISeq (via ASeq.__iter__) and Python iterables
    # support it. Avoids juggling two protocols.
    it = iter(b)
    cdef int i
    for i in range(a._cnt):
        try:
            v = next(it)
        except StopIteration:
            return False
        if not eq_fn(a.nth(i), v):
            return False
    try:
        next(it)
        return False
    except StopIteration:
        return True


# ---------- ChunkedSeq view ----------

cdef class _PVChunkedSeq(ASeq):
    """Chunked seq view of a PersistentVector. See PersistentVector.seq()."""

    cdef PersistentVector _vec
    cdef list _node
    cdef int _i
    cdef int _offset

    def __cinit__(self, vec=None, i=0, offset=0):
        # vec=None is the internal __new__ + direct-assign path used by
        # from_arrays. Public construction passes a vec.
        if vec is None:
            return
        self._vec = vec
        self._i = i
        self._offset = offset
        self._node = (<PersistentVector>vec)._array_for(i)

    @staticmethod
    cdef _PVChunkedSeq from_arrays(PersistentVector vec, list node, int i, int offset):
        cdef _PVChunkedSeq cs = _PVChunkedSeq.__new__(_PVChunkedSeq)
        cs._vec = vec
        cs._node = node
        cs._i = i
        cs._offset = offset
        return cs

    def first(self):
        return self._node[self._offset]

    def next(self):
        if self._offset + 1 < len(self._node):
            return _PVChunkedSeq.from_arrays(self._vec, self._node, self._i, self._offset + 1)
        return self.chunked_next()

    def count(self):
        return self._vec._cnt - (self._i + self._offset)

    def chunked_first(self):
        return ArrayChunk(self._node, self._offset)

    def chunked_next(self):
        if self._i + len(self._node) < self._vec._cnt:
            return _PVChunkedSeq(self._vec, self._i + len(self._node), 0)
        return None

    def chunked_more(self):
        s = self.chunked_next()
        if s is None:
            return _empty_list
        return s

    def with_meta(self, meta):
        if self._meta is meta:
            return self
        cdef _PVChunkedSeq cs = _PVChunkedSeq.from_arrays(self._vec, self._node, self._i, self._offset)
        cs._meta = meta
        return cs

    def reduce(self, f, start=NOT_FOUND):
        cdef int j
        cdef int ii
        cdef int step = 0
        cdef list array
        cdef object acc
        if start is NOT_FOUND:
            if self._i + self._offset < self._vec._cnt:
                acc = self._node[self._offset]
            else:
                return f()
            for j in range(self._offset + 1, len(self._node)):
                acc = f(acc, self._node[j])
                if isinstance(acc, Reduced):
                    return (<Reduced>acc).deref()
            ii = self._i + len(self._node)
            while ii < self._vec._cnt:
                array = self._vec._array_for(ii)
                for j in range(len(array)):
                    acc = f(acc, array[j])
                    if isinstance(acc, Reduced):
                        return (<Reduced>acc).deref()
                step = len(array)
                ii += step
            return acc
        else:
            acc = start
            for j in range(self._offset, len(self._node)):
                acc = f(acc, self._node[j])
                if isinstance(acc, Reduced):
                    return (<Reduced>acc).deref()
            ii = self._i + len(self._node)
            while ii < self._vec._cnt:
                array = self._vec._array_for(ii)
                for j in range(len(array)):
                    acc = f(acc, array[j])
                    if isinstance(acc, Reduced):
                        return (<Reduced>acc).deref()
                step = len(array)
                ii += step
            return acc

    def drop(self, int n):
        cdef int o = self._offset + n
        cdef int new_i, new_offset
        if o < len(self._node):
            return _PVChunkedSeq.from_arrays(self._vec, self._node, self._i, o)
        new_i = self._i + o
        if new_i < self._vec._cnt:
            new_offset = new_i % 32
            return _PVChunkedSeq.from_arrays(
                self._vec, self._vec._array_for(new_i),
                new_i - new_offset, new_offset)
        return None


IChunkedSeq.register(_PVChunkedSeq)
Counted.register(_PVChunkedSeq)
IReduce.register(_PVChunkedSeq)
IReduceInit.register(_PVChunkedSeq)
IDrop.register(_PVChunkedSeq)


# ---------- RSeq view (reverse) ----------

cdef class _PVRSeq(ASeq):
    """Reverse seq view of a PersistentVector — yields elements from end to start."""

    cdef object _vec     # IPersistentVector
    cdef int _i

    def __cinit__(self, vec, int i):
        self._vec = vec
        self._i = i

    def first(self):
        return self._vec.nth(self._i)

    def next(self):
        if self._i > 0:
            return _PVRSeq(self._vec, self._i - 1)
        return None

    def count(self):
        return self._i + 1

    def with_meta(self, meta):
        if self._meta is meta:
            return self
        cdef _PVRSeq r = _PVRSeq(self._vec, self._i)
        r._meta = meta
        return r


Counted.register(_PVRSeq)


# ---------- factory ----------

cdef object _pv_from_iter(items):
    # Build via TransientVector for amortized O(1) conj.
    cdef TransientVector t = TransientVector._from_persistent(_PV_EMPTY)
    for x in items:
        t.conj(x)
    return t.persistent()


# ---------- TransientVector ----------

cdef class TransientVector:
    """The mutable companion to PersistentVector. asTransient() forks one off
    a persistent; persistent() seals it back into a fresh PersistentVector."""

    cdef int _cnt
    cdef int _shift
    cdef _PVNode _root
    cdef list _tail
    cdef object __weakref__

    def __cinit__(self):
        pass

    @staticmethod
    cdef TransientVector _from_persistent(PersistentVector v):
        cdef TransientVector t = TransientVector.__new__(TransientVector)
        t._cnt = v._cnt
        t._shift = v._shift
        t._root = _editable_root(v._root)
        t._tail = _editable_tail(v._tail)
        return t

    cdef _ensure_editable(self):
        if self._root.edit.thread is None:
            raise RuntimeError("Transient used after persistent! call")

    cdef _PVNode _ensure_editable_node(self, _PVNode node):
        if node.edit is self._root.edit:
            return node
        return _PVNode(self._root.edit, list(node.array))

    cdef int _tailoff(self) noexcept:
        return _tailoff_for(self._cnt)

    cdef list _array_for_t(self, int i):
        cdef _PVNode node
        cdef int level
        if 0 <= i < self._cnt:
            if i >= self._tailoff():
                return self._tail
            node = self._root
            level = self._shift
            while level > 0:
                node = <_PVNode>node.array[(i >> level) & 0x1f]
                level -= 5
            return node.array
        raise IndexError(i)

    cdef list _editable_array_for(self, int i):
        cdef _PVNode node
        cdef int level
        if 0 <= i < self._cnt:
            if i >= self._tailoff():
                return self._tail
            node = self._root
            level = self._shift
            while level > 0:
                node = self._ensure_editable_node(<_PVNode>node.array[(i >> level) & 0x1f])
                level -= 5
            return node.array
        raise IndexError(i)

    def count(self):
        self._ensure_editable()
        return self._cnt

    def __len__(self):
        return self.count()

    def nth(self, int i, not_found=NOT_FOUND):
        self._ensure_editable()
        if 0 <= i < self._cnt:
            return self._array_for_t(i)[i & 0x1f]
        if not_found is NOT_FOUND:
            raise IndexError(i)
        return not_found

    def val_at(self, key, not_found=NOT_FOUND):
        self._ensure_editable()
        if isinstance(key, int) and not isinstance(key, bool):
            if 0 <= key < self._cnt:
                return self._array_for_t(key)[key & 0x1f]
        return None if not_found is NOT_FOUND else not_found

    def contains_key(self, key):
        self._ensure_editable()
        return (isinstance(key, int) and not isinstance(key, bool)
                and 0 <= key < self._cnt)

    def entry_at(self, key):
        if self.contains_key(key):
            return (key, self.nth(key))
        return None

    def conj(self, val):
        self._ensure_editable()
        cdef int i = self._cnt
        cdef _PVNode tailnode, newroot
        cdef int newshift
        # Tail has room?
        if i - self._tailoff() < 32:
            self._tail[i & 0x1f] = val
            self._cnt += 1
            return self
        # Tail full. Push into trie.
        tailnode = _PVNode(self._root.edit, self._tail)
        self._tail = [None] * 32
        self._tail[0] = val
        newshift = self._shift
        if (self._cnt >> 5) > (1 << self._shift):
            newroot = _PVNode(self._root.edit)
            newroot.array[0] = self._root
            newroot.array[1] = _new_path(self._root.edit, self._shift, tailnode)
            newshift += 5
        else:
            newroot = self._push_tail_t(self._shift, self._root, tailnode)
        self._root = newroot
        self._shift = newshift
        self._cnt += 1
        return self

    cdef _PVNode _push_tail_t(self, int level, _PVNode parent, _PVNode tailnode):
        parent = self._ensure_editable_node(parent)
        cdef int subidx = ((self._cnt - 1) >> level) & 0x1f
        cdef _PVNode ret = parent
        cdef _PVNode child
        cdef _PVNode node_to_insert
        if level == 5:
            node_to_insert = tailnode
        else:
            child = parent.array[subidx]
            if child is not None:
                node_to_insert = self._push_tail_t(level - 5, child, tailnode)
            else:
                node_to_insert = _new_path(self._root.edit, level - 5, tailnode)
        ret.array[subidx] = node_to_insert
        return ret

    def assoc_n(self, int i, val):
        self._ensure_editable()
        cdef list arr
        if 0 <= i < self._cnt:
            if i >= self._tailoff():
                self._tail[i & 0x1f] = val
                return self
            self._root = self._do_assoc_t(self._shift, self._root, i, val)
            return self
        if i == self._cnt:
            return self.conj(val)
        raise IndexError(i)

    cdef _PVNode _do_assoc_t(self, int level, _PVNode node, int i, object val):
        node = self._ensure_editable_node(node)
        cdef _PVNode ret = node
        cdef int subidx
        if level == 0:
            ret.array[i & 0x1f] = val
        else:
            subidx = (i >> level) & 0x1f
            ret.array[subidx] = self._do_assoc_t(level - 5, <_PVNode>node.array[subidx], i, val)
        return ret

    def assoc(self, key, val):
        if isinstance(key, int) and not isinstance(key, bool):
            return self.assoc_n(key, val)
        raise TypeError("Vector key must be integer")

    def pop(self):
        self._ensure_editable()
        if self._cnt == 0:
            raise IndexError("Can't pop empty vector")
        if self._cnt == 1:
            self._cnt = 0
            return self
        cdef int i = self._cnt - 1
        # Tail has more than one element? Shrink in place.
        if (i & 0x1f) > 0:
            self._cnt -= 1
            return self
        # Need to promote a leaf out of the trie as the new tail.
        cdef list newtail = list(self._editable_array_for(self._cnt - 2))
        cdef _PVNode newroot = self._pop_tail_t(self._shift, self._root)
        cdef int newshift = self._shift
        if newroot is None:
            newroot = _PVNode(self._root.edit)
        if self._shift > 5 and newroot.array[1] is None:
            newroot = self._ensure_editable_node(<_PVNode>newroot.array[0])
            newshift -= 5
        self._root = newroot
        self._shift = newshift
        self._cnt -= 1
        self._tail = newtail
        return self

    cdef _PVNode _pop_tail_t(self, int level, _PVNode node):
        node = self._ensure_editable_node(node)
        cdef int subidx = ((self._cnt - 2) >> level) & 0x1f
        cdef _PVNode newchild
        cdef _PVNode ret
        if level > 5:
            newchild = self._pop_tail_t(level - 5, <_PVNode>node.array[subidx])
            if newchild is None and subidx == 0:
                return None
            ret = node
            ret.array[subidx] = newchild
            return ret
        if subidx == 0:
            return None
        ret = node
        ret.array[subidx] = None
        return ret

    def persistent(self):
        self._ensure_editable()
        # Poison further use.
        self._root.edit.thread = None
        # Trim tail to actual size.
        cdef int trimmed_size = self._cnt - self._tailoff()
        cdef list trimmed = list(self._tail[:trimmed_size])
        return _make_pv(self._cnt, self._shift, self._root, trimmed, None)

    def __call__(self, key):
        if isinstance(key, int) and not isinstance(key, bool):
            return self.nth(key)
        raise TypeError("Vector key must be integer")


ITransientVector.register(TransientVector)
ITransientAssociative.register(TransientVector)
ITransientAssociative2.register(TransientVector)
ITransientCollection.register(TransientVector)
Counted.register(TransientVector)
Indexed.register(TransientVector)
ILookup.register(TransientVector)
IFn.register(TransientVector)


# ---------- transient editable helpers ----------

import threading as _threading


cdef _PVNode _editable_root(_PVNode node):
    cdef _Edit edit = _Edit(_threading.current_thread())
    return _PVNode(edit, list(node.array))


cdef list _editable_tail(list tl):
    # Tail is always 32-element internally for transient.
    cdef list ret = [None] * 32
    cdef int i
    for i in range(len(tl)):
        ret[i] = tl[i]
    return ret


# ---------- the singleton EMPTY ----------

cdef PersistentVector _PV_EMPTY = _make_pv(0, 5, _EMPTY_NODE, [], None)
PERSISTENT_VECTOR_EMPTY = _PV_EMPTY
