# Port of clojure.lang.PersistentTreeMap.
#
# Persistent Red-Black tree (Okasaki / Kahrs / Larsen). Supports a custom
# Comparator (or a default one that handles nil + cross-type numbers + Python
# `<`).
#
# Java models the node hierarchy with 8 concrete subclasses (Black / BlackVal
# / BlackBranch / BlackBranchVal × Red variants) for memory efficiency. Our
# port collapses to a single _RBNode with an `is_red` flag and slots for
# left/right/key/val. Slightly more memory per node, much less code.


def _default_compare(a, b):
    """Default comparator: nil-aware, numeric cross-type via Numbers.compare,
    then Python ordering."""
    if a is None:
        return 0 if b is None else -1
    if b is None:
        return 1
    if Numbers._is_number(a) and Numbers._is_number(b):
        return Numbers.compare(a, b)
    if a == b:
        return 0
    if a < b:
        return -1
    return 1


cdef class _RBNode:
    """Single node — color carried as a flag rather than a class hierarchy."""

    cdef bint _is_red
    cdef object _key
    cdef object _val
    cdef _RBNode _left
    cdef _RBNode _right

    def __cinit__(self, bint is_red=False, object key=None, object val=None,
                  _RBNode left=None, _RBNode right=None):
        self._is_red = is_red
        self._key = key
        self._val = val
        self._left = left
        self._right = right

    cdef _RBNode add_left(self, _RBNode ins):
        if not self._is_red:
            return ins.balance_left(self)
        return _make_red(self._key, self._val, ins, self._right)

    cdef _RBNode add_right(self, _RBNode ins):
        if not self._is_red:
            return ins.balance_right(self)
        return _make_red(self._key, self._val, self._left, ins)

    cdef _RBNode balance_left(self, _RBNode parent):
        # Black variant: just return black at parent.
        if not self._is_red:
            return _make_black(parent._key, parent._val, self, parent._right)
        # Red variant — check for red-red violation.
        if self._left is not None and self._left._is_red:
            return _make_red(self._key, self._val,
                             self._left.blacken(),
                             _make_black(parent._key, parent._val, self._right, parent._right))
        if self._right is not None and self._right._is_red:
            return _make_red(self._right._key, self._right._val,
                             _make_black(self._key, self._val, self._left, self._right._left),
                             _make_black(parent._key, parent._val, self._right._right, parent._right))
        return _make_black(parent._key, parent._val, self, parent._right)

    cdef _RBNode balance_right(self, _RBNode parent):
        if not self._is_red:
            return _make_black(parent._key, parent._val, parent._left, self)
        if self._right is not None and self._right._is_red:
            return _make_red(self._key, self._val,
                             _make_black(parent._key, parent._val, parent._left, self._left),
                             self._right.blacken())
        if self._left is not None and self._left._is_red:
            return _make_red(self._left._key, self._left._val,
                             _make_black(parent._key, parent._val, parent._left, self._left._left),
                             _make_black(self._key, self._val, self._left._right, self._right))
        return _make_black(parent._key, parent._val, parent._left, self)

    cdef _RBNode blacken(self):
        if not self._is_red:
            return self
        return _RBNode(False, self._key, self._val, self._left, self._right)

    cdef _RBNode redden(self):
        if self._is_red:
            raise RuntimeError("Invariant violation: redden of red node")
        return _RBNode(True, self._key, self._val, self._left, self._right)

    cdef _RBNode replace(self, object key, object val, _RBNode left, _RBNode right):
        return _RBNode(self._is_red, key, val, left, right)

    cdef object kv_reduce(self, f, init):
        if self._left is not None:
            init = self._left.kv_reduce(f, init)
            if isinstance(init, Reduced):
                return init
        init = f(init, self._key, self._val)
        if isinstance(init, Reduced):
            return init
        if self._right is not None:
            init = self._right.kv_reduce(f, init)
        return init


cdef inline _RBNode _make_red(object key, object val, _RBNode left, _RBNode right):
    return _RBNode(True, key, val, left, right)


cdef inline _RBNode _make_black(object key, object val, _RBNode left, _RBNode right):
    return _RBNode(False, key, val, left, right)


# --- balanced-deletion helpers ------------------------------------------

cdef _RBNode _balance_left_del(object key, object val, _RBNode del_, _RBNode right):
    if del_ is not None and del_._is_red:
        return _make_red(key, val, del_.blacken(), right)
    if right is not None and not right._is_red:
        return _right_balance(key, val, del_, right.redden())
    if (right is not None and right._is_red and right._left is not None
            and not right._left._is_red):
        return _make_red(right._left._key, right._left._val,
                         _make_black(key, val, del_, right._left._left),
                         _right_balance(right._key, right._val,
                                        right._left._right, right._right.redden()))
    raise RuntimeError("Invariant violation in balance_left_del")


cdef _RBNode _balance_right_del(object key, object val, _RBNode left, _RBNode del_):
    if del_ is not None and del_._is_red:
        return _make_red(key, val, left, del_.blacken())
    if left is not None and not left._is_red:
        return _left_balance(key, val, left.redden(), del_)
    if (left is not None and left._is_red and left._right is not None
            and not left._right._is_red):
        return _make_red(left._right._key, left._right._val,
                         _left_balance(left._key, left._val,
                                       left._left.redden(), left._right._left),
                         _make_black(key, val, left._right._right, del_))
    raise RuntimeError("Invariant violation in balance_right_del")


cdef _RBNode _left_balance(object key, object val, _RBNode ins, _RBNode right):
    if (ins is not None and ins._is_red and ins._left is not None and ins._left._is_red):
        return _make_red(ins._key, ins._val,
                         ins._left.blacken(),
                         _make_black(key, val, ins._right, right))
    if (ins is not None and ins._is_red and ins._right is not None and ins._right._is_red):
        return _make_red(ins._right._key, ins._right._val,
                         _make_black(ins._key, ins._val, ins._left, ins._right._left),
                         _make_black(key, val, ins._right._right, right))
    return _make_black(key, val, ins, right)


cdef _RBNode _right_balance(object key, object val, _RBNode left, _RBNode ins):
    if (ins is not None and ins._is_red and ins._right is not None and ins._right._is_red):
        return _make_red(ins._key, ins._val,
                         _make_black(key, val, left, ins._left),
                         ins._right.blacken())
    if (ins is not None and ins._is_red and ins._left is not None and ins._left._is_red):
        return _make_red(ins._left._key, ins._left._val,
                         _make_black(key, val, left, ins._left._left),
                         _make_black(ins._key, ins._val, ins._left._right, ins._right))
    return _make_black(key, val, left, ins)


cdef _RBNode _append(_RBNode left, _RBNode right):
    if left is None:
        return right
    if right is None:
        return left
    if left._is_red:
        if right._is_red:
            app = _append(left._right, right._left)
            if app is not None and app._is_red:
                return _make_red(app._key, app._val,
                                 _make_red(left._key, left._val, left._left, app._left),
                                 _make_red(right._key, right._val, app._right, right._right))
            return _make_red(left._key, left._val, left._left,
                             _make_red(right._key, right._val, app, right._right))
        return _make_red(left._key, left._val, left._left, _append(left._right, right))
    if right._is_red:
        return _make_red(right._key, right._val, _append(left, right._left), right._right)
    # both black
    app = _append(left._right, right._left)
    if app is not None and app._is_red:
        return _make_red(app._key, app._val,
                         _make_black(left._key, left._val, left._left, app._left),
                         _make_black(right._key, right._val, app._right, right._right))
    return _balance_left_del(left._key, left._val, left._left,
                             _make_black(right._key, right._val, app, right._right))


# --- PersistentTreeMap --------------------------------------------------

cdef PersistentTreeMap _make_ptm(object meta, object comp, _RBNode tree, int cnt):
    cdef PersistentTreeMap m = PersistentTreeMap.__new__(PersistentTreeMap)
    m._meta = meta
    m._comp = comp
    m._tree = tree
    m._count = cnt
    return m


cdef class PersistentTreeMap:
    """A persistent sorted map. O(log n) ops via a red-black tree."""

    cdef readonly int _count
    cdef readonly object _comp
    cdef _RBNode _tree
    cdef object _meta
    cdef int32_t _hash_cache
    cdef int32_t _hasheq_cache
    cdef object __weakref__

    def __cinit__(self):
        pass

    @staticmethod
    def create(*args):
        """PersistentTreeMap.create(k1, v1, k2, v2, ...) — uses default comparator."""
        if len(args) % 2 != 0:
            raise ValueError("PersistentTreeMap.create requires alternating key/value args")
        ret = _PTM_EMPTY
        cdef int i
        for i in range(0, len(args), 2):
            ret = ret.assoc(args[i], args[i + 1])
        return ret

    @staticmethod
    def create_with_comparator(comp, *args):
        """PersistentTreeMap.create_with_comparator(cmp, k1, v1, k2, v2, ...)."""
        if len(args) % 2 != 0:
            raise ValueError("create_with_comparator requires alternating key/value args after comparator")
        ret = _make_ptm(None, comp, None, 0)
        cdef int i
        for i in range(0, len(args), 2):
            ret = ret.assoc(args[i], args[i + 1])
        return ret

    @staticmethod
    def from_iterable(iterable, comp=None):
        if comp is None:
            comp = _default_compare
        ret = _make_ptm(None, comp, None, 0)
        for entry in iterable:
            if isinstance(entry, MapEntry):
                ret = ret.assoc((<MapEntry>entry)._key, (<MapEntry>entry)._val)
            elif isinstance(entry, IMapEntry):
                ret = ret.assoc(entry.key(), entry.val())
            elif isinstance(entry, (list, tuple)) and len(entry) == 2:
                ret = ret.assoc(entry[0], entry[1])
            else:
                raise TypeError(f"Cannot use as map entry: {type(entry).__name__}")
        return ret

    def count(self):
        return self._count

    def __len__(self):
        return self._count

    cdef int _do_compare(self, a, b) except? -2:
        return self._comp(a, b)

    def comparator(self):
        return self._comp

    def entry_key(self, entry):
        if isinstance(entry, IMapEntry):
            return entry.key()
        return entry[0]

    def contains_key(self, key):
        return self._entry_at_node(key) is not None

    def entry_at(self, key):
        cdef _RBNode n = self._entry_at_node(key)
        if n is None:
            return None
        return MapEntry(n._key, n._val)

    cdef _RBNode _entry_at_node(self, object key):
        cdef _RBNode t = self._tree
        cdef int c
        while t is not None:
            c = self._do_compare(key, t._key)
            if c == 0:
                return t
            t = t._left if c < 0 else t._right
        return None

    def val_at(self, key, not_found=NOT_FOUND):
        cdef _RBNode n = self._entry_at_node(key)
        if n is not None:
            return n._val
        return None if not_found is NOT_FOUND else not_found

    def assoc(self, key, val):
        cdef _Box found = _Box(None)
        cdef _RBNode t = self._add(self._tree, key, val, found)
        cdef _RBNode found_node
        if t is None:
            # Already-present key.
            found_node = <_RBNode>found.val
            if found_node._val is val:
                return self
            return _make_ptm(self._meta, self._comp, self._replace(self._tree, key, val), self._count)
        return _make_ptm(self._meta, self._comp, t.blacken(), self._count + 1)

    def assoc_ex(self, key, val):
        cdef _Box found = _Box(None)
        cdef _RBNode t = self._add(self._tree, key, val, found)
        if t is None:
            raise ValueError(f"Key already present: {key!r}")
        return _make_ptm(self._meta, self._comp, t.blacken(), self._count + 1)

    def without(self, key):
        cdef _Box found = _Box(None)
        cdef _RBNode t = self._remove(self._tree, key, found)
        if t is None:
            if found.val is None:
                return self
            # Now-empty.
            return _make_ptm(self._meta, self._comp, None, 0)
        return _make_ptm(self._meta, self._comp, t.blacken(), self._count - 1)

    cdef _RBNode _add(self, _RBNode t, object key, object val, _Box found):
        cdef int c
        cdef _RBNode ins
        if t is None:
            return _RBNode(True, key, val, None, None)
        c = self._do_compare(key, t._key)
        if c == 0:
            found.val = t
            return None
        ins = self._add(t._left if c < 0 else t._right, key, val, found)
        if ins is None:
            return None
        if c < 0:
            return t.add_left(ins)
        return t.add_right(ins)

    cdef _RBNode _remove(self, _RBNode t, object key, _Box found):
        cdef int c
        cdef _RBNode del_
        if t is None:
            return None
        c = self._do_compare(key, t._key)
        if c == 0:
            found.val = t
            return _append(t._left, t._right)
        del_ = self._remove(t._left if c < 0 else t._right, key, found)
        if del_ is None and found.val is None:
            return None
        if c < 0:
            if t._left is not None and not t._left._is_red:
                return _balance_left_del(t._key, t._val, del_, t._right)
            return _make_red(t._key, t._val, del_, t._right)
        if t._right is not None and not t._right._is_red:
            return _balance_right_del(t._key, t._val, t._left, del_)
        return _make_red(t._key, t._val, t._left, del_)

    cdef _RBNode _replace(self, _RBNode t, object key, object val):
        cdef int c = self._do_compare(key, t._key)
        return t.replace(t._key,
                         val if c == 0 else t._val,
                         self._replace(t._left, key, val) if c < 0 else t._left,
                         self._replace(t._right, key, val) if c > 0 else t._right)

    def cons(self, o):
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
        ret = self
        for e in o:
            ret = ret.cons(e)
        return ret

    def empty(self):
        return _make_ptm(self._meta, self._comp, None, 0)

    def seq(self):
        if self._count == 0:
            return None
        return _PTMSeq.create(self._tree, True, self._count)

    def rseq(self):
        if self._count == 0:
            return None
        return _PTMSeq.create(self._tree, False, self._count)

    def seq_with_comparator(self, ascending):
        if self._count == 0:
            return None
        return _PTMSeq.create(self._tree, ascending, self._count)

    def seq_from(self, key, ascending):
        if self._count == 0:
            return None
        cdef object stack = None
        cdef _RBNode t = self._tree
        cdef int c
        while t is not None:
            c = self._do_compare(key, t._key)
            if c == 0:
                stack = Cons(t, stack)
                return _PTMSeq(stack, ascending, -1)
            elif ascending:
                if c < 0:
                    stack = Cons(t, stack)
                    t = t._left
                else:
                    t = t._right
            else:
                if c > 0:
                    stack = Cons(t, stack)
                    t = t._right
                else:
                    t = t._left
        if stack is not None:
            return _PTMSeq(stack, ascending, -1)
        return None

    def min_key(self):
        cdef _RBNode t = self._tree
        if t is None:
            return None
        while t._left is not None:
            t = t._left
        return t._key

    def max_key(self):
        cdef _RBNode t = self._tree
        if t is None:
            return None
        while t._right is not None:
            t = t._right
        return t._key

    def kv_reduce(self, f, init):
        if self._tree is not None:
            init = self._tree.kv_reduce(f, init)
        if isinstance(init, Reduced):
            return (<Reduced>init).deref()
        return init

    def equiv(self, other):
        if other is self:
            return True
        return _ptm_equiv(self, other)

    def __eq__(self, other):
        if other is self:
            return True
        return _ptm_equiv(self, other)

    def __ne__(self, other):
        return not self.__eq__(other)

    def __hash__(self):
        if self._hash_cache != 0:
            return self._hash_cache
        cdef int32_t h = 0
        for entry in self:
            k = entry.key()
            v = entry.val()
            kh = 0 if k is None else hash(k)
            vh = 0 if v is None else hash(v)
            h = _to_int32_mask(<long long>h + (<long long>kh ^ <long long>vh))
        self._hash_cache = h
        return h

    def hasheq(self):
        if self._hasheq_cache != 0:
            return self._hasheq_cache
        result = Murmur3.hash_unordered(self)
        self._hasheq_cache = result
        return result

    def __iter__(self):
        s = self.seq()
        while s is not None:
            yield s.first()
            s = s.next()

    def keys(self):
        for entry in self:
            yield entry.key()

    def values(self):
        for entry in self:
            yield entry.val()

    def __contains__(self, key):
        return self.contains_key(key)

    def __getitem__(self, key):
        cdef _RBNode n = self._entry_at_node(key)
        if n is None:
            raise KeyError(key)
        return n._val

    def __call__(self, key, not_found=NOT_FOUND):
        return self.val_at(key, not_found)

    def __bool__(self):
        return self._count > 0

    def meta(self):
        return self._meta

    def with_meta(self, meta):
        if self._meta is meta:
            return self
        return _make_ptm(meta, self._comp, self._tree, self._count)

    def __str__(self):
        parts = []
        for entry in self:
            parts.append(_print_str(entry.key()) + " " + _print_str(entry.val()))
        return "{" + ", ".join(parts) + "}"

    def __repr__(self):
        return self.__str__()


cdef bint _ptm_equiv(PersistentTreeMap a, object other):
    if isinstance(other, IPersistentMap):
        if other.count() != a._count:
            return False
        for entry in a:
            k = entry.key()
            v = entry.val()
            if not other.contains_key(k):
                return False
            if not Util.equiv(v, other.val_at(k)):
                return False
        return True
    if isinstance(other, dict):
        if len(other) != a._count:
            return False
        for entry in a:
            k = entry.key()
            v = entry.val()
            if k not in other:
                return False
            if not Util.equiv(v, other[k]):
                return False
        return True
    return False


cdef class _PTMSeq(ASeq):
    """In-order traversal seq over a red-black tree, using an ISeq stack."""

    cdef object _stack       # ISeq of _RBNode
    cdef bint _asc
    cdef int _cnt            # -1 if not pre-computed

    def __cinit__(self, stack=None, asc=True, cnt=-1):
        if stack is None and cnt == -1:
            return
        self._stack = stack
        self._asc = asc
        self._cnt = cnt

    @staticmethod
    cdef object create(_RBNode t, bint asc, int cnt):
        return _PTMSeq(_PTMSeq._push(t, None, asc), asc, cnt)

    @staticmethod
    cdef object _push(_RBNode t, object stack, bint asc):
        while t is not None:
            stack = Cons(t, stack)
            t = t._left if asc else t._right
        return stack

    def first(self):
        cdef _RBNode n = self._stack.first()
        return MapEntry(n._key, n._val)

    def next(self):
        cdef _RBNode t = self._stack.first()
        cdef object next_stack = _PTMSeq._push(
            t._right if self._asc else t._left,
            self._stack.next(), self._asc)
        if next_stack is not None:
            return _PTMSeq(next_stack, self._asc, self._cnt - 1 if self._cnt > 0 else -1)
        return None

    def count(self):
        if self._cnt >= 0:
            return self._cnt
        return ASeq.count(self)

    def with_meta(self, meta):
        cdef _PTMSeq s = _PTMSeq(self._stack, self._asc, self._cnt)
        s._meta = meta
        return s


cdef PersistentTreeMap _PTM_EMPTY = _make_ptm(None, _default_compare, None, 0)
PERSISTENT_TREE_MAP_EMPTY = _PTM_EMPTY


IPersistentMap.register(PersistentTreeMap)
Associative.register(PersistentTreeMap)
ILookup.register(PersistentTreeMap)
IPersistentCollection.register(PersistentTreeMap)
Counted.register(PersistentTreeMap)
IFn.register(PersistentTreeMap)
IHashEq.register(PersistentTreeMap)
IMeta.register(PersistentTreeMap)
IObj.register(PersistentTreeMap)
IKVReduce.register(PersistentTreeMap)
Reversible.register(PersistentTreeMap)
Sorted.register(PersistentTreeMap)
