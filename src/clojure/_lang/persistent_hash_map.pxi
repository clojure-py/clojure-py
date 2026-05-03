# Port of clojure.lang.PersistentHashMap.
#
# A persistent rendition of Phil Bagwell's Hash Array Mapped Trie. Uses path-
# copying for persistence, hash-collision leaves at full depth, and node
# polymorphism (BitmapIndexedNode → ArrayNode escalation at 16 children;
# ArrayNode → packed BitmapIndexedNode contraction at ≤8 children).
#
# null keys are stored separately on the top-level map (hasNull/nullValue)
# rather than threaded through the trie.
#
# Reuses _Edit from persistent_vector.pxi for transient ownership tokens.


# --- helpers --------------------------------------------------------------

cdef inline int _phm_mask(int32_t hash_val, int shift) noexcept nogil:
    # 5-bit chunk of the hash starting at `shift`. Java: (hash >>> shift) & 0x1f.
    return <int>((<uint32_t>hash_val >> shift) & 0x1fu)


cdef inline uint32_t _phm_bitpos(int32_t hash_val, int shift) noexcept nogil:
    return <uint32_t>1u << _phm_mask(hash_val, shift)


cdef inline int _phm_popcount(uint32_t x) noexcept nogil:
    # Bit-twiddle popcount; portable.
    x = x - ((x >> 1) & 0x55555555u)
    x = (x & 0x33333333u) + ((x >> 2) & 0x33333333u)
    x = (x + (x >> 4)) & 0x0f0f0f0fu
    return <int>((x * 0x01010101u) >> 24)


cdef list _clone_set(list array, int i, object a):
    cdef list r = list(array)
    r[i] = a
    return r


cdef list _clone_set2(list array, int i, object a, int j, object b):
    cdef list r = list(array)
    r[i] = a
    r[j] = b
    return r


cdef list _remove_pair(list array, int i):
    # Remove the (i*2, i*2+1) pair, shifting everything down.
    cdef list r = list(array)
    del r[2 * i]
    del r[2 * i]   # second 'del' shifts; this removes the second slot
    return r


# --- Box: mutable single-cell, used to signal "added"/"removed" leaf -----

cdef class _Box:
    """Mutable one-slot cell. Java's Box. Used to thread an added/removed
    leaf signal back up through the recursive descent."""

    cdef public object val

    def __cinit__(self, val=None):
        self.val = val


# --- _INode base ----------------------------------------------------------

cdef class _INode:
    """Internal HAMT node. Subclasses: _BitmapIndexedNode, _ArrayNode,
    _HashCollisionNode."""

    cdef _INode assoc(self, int shift, int32_t hash_val, object key, object val, _Box added_leaf):
        raise NotImplementedError

    cdef _INode without(self, int shift, int32_t hash_val, object key):
        raise NotImplementedError

    cdef object find(self, int shift, int32_t hash_val, object key, object not_found):
        raise NotImplementedError

    cdef object find_entry(self, int shift, int32_t hash_val, object key):
        # Returns MapEntry or None.
        raise NotImplementedError

    cdef object node_seq(self):
        raise NotImplementedError

    cdef object kv_reduce(self, f, init):
        raise NotImplementedError

    cdef _INode t_assoc(self, _Edit edit, int shift, int32_t hash_val, object key, object val, _Box added_leaf):
        raise NotImplementedError

    cdef _INode t_without(self, _Edit edit, int shift, int32_t hash_val, object key, _Box removed_leaf):
        raise NotImplementedError


# --- HashCollisionNode ----------------------------------------------------

cdef class _HashCollisionNode(_INode):
    """Leaf node for keys whose hashes collide all the way down."""

    cdef int32_t _hash
    cdef int _count
    cdef list _array
    cdef _Edit _edit

    def __cinit__(self, _Edit edit, int32_t hash_val=0, int count=0, list array=None):
        self._edit = edit
        self._hash = hash_val
        self._count = count
        self._array = array if array is not None else []

    cdef int _find_index(self, object key):
        cdef int i
        for i in range(0, 2 * self._count, 2):
            if Util.equiv(key, self._array[i]):
                return i
        return -1

    cdef _INode assoc(self, int shift, int32_t hash_val, object key, object val, _Box added_leaf):
        cdef int idx
        cdef list new_arr
        if hash_val == self._hash:
            idx = self._find_index(key)
            if idx != -1:
                if self._array[idx + 1] is val:
                    return self
                return _HashCollisionNode(_NOEDIT, self._hash, self._count,
                                          _clone_set(self._array, idx + 1, val))
            new_arr = list(self._array)
            new_arr.append(key)
            new_arr.append(val)
            added_leaf.val = added_leaf
            return _HashCollisionNode(_NOEDIT, self._hash, self._count + 1, new_arr)
        # Different hash → wrap in a BitmapIndexedNode and recurse.
        return _BitmapIndexedNode(_NOEDIT,
                                  _phm_bitpos(self._hash, shift),
                                  [None, self]
                                  ).assoc(shift, hash_val, key, val, added_leaf)

    cdef _INode without(self, int shift, int32_t hash_val, object key):
        cdef int idx = self._find_index(key)
        if idx == -1:
            return self
        if self._count == 1:
            return None
        return _HashCollisionNode(_NOEDIT, self._hash, self._count - 1,
                                  _remove_pair(self._array, idx // 2))

    cdef object find(self, int shift, int32_t hash_val, object key, object not_found):
        cdef int idx = self._find_index(key)
        if idx < 0:
            return not_found
        return self._array[idx + 1]

    cdef object find_entry(self, int shift, int32_t hash_val, object key):
        cdef int idx = self._find_index(key)
        if idx < 0:
            return None
        return MapEntry(self._array[idx], self._array[idx + 1])

    cdef object node_seq(self):
        return _NodeSeq.create(self._array)

    cdef object kv_reduce(self, f, init):
        return _node_seq_kv_reduce(self._array, f, init)

    cdef _HashCollisionNode _ensure_editable(self, _Edit edit):
        if self._edit is edit:
            return self
        cdef list new_arr = list(self._array) + [None, None]
        return _HashCollisionNode(edit, self._hash, self._count, new_arr)

    cdef _HashCollisionNode _ensure_editable_with(self, _Edit edit, int count, list array):
        if self._edit is edit:
            self._array = array
            self._count = count
            return self
        return _HashCollisionNode(edit, self._hash, count, array)

    cdef _INode t_assoc(self, _Edit edit, int shift, int32_t hash_val, object key, object val, _Box added_leaf):
        cdef int idx
        cdef _HashCollisionNode editable
        cdef list new_arr
        if hash_val == self._hash:
            idx = self._find_index(key)
            if idx != -1:
                if self._array[idx + 1] is val:
                    return self
                editable = self._ensure_editable(edit)
                editable._array[idx + 1] = val
                return editable
            if len(self._array) > 2 * self._count:
                added_leaf.val = added_leaf
                editable = self._ensure_editable(edit)
                editable._array[2 * self._count] = key
                editable._array[2 * self._count + 1] = val
                editable._count += 1
                return editable
            new_arr = list(self._array)
            new_arr.append(key)
            new_arr.append(val)
            added_leaf.val = added_leaf
            return self._ensure_editable_with(edit, self._count + 1, new_arr)
        return _BitmapIndexedNode(edit,
                                  _phm_bitpos(self._hash, shift),
                                  [None, self, None, None]
                                  ).t_assoc(edit, shift, hash_val, key, val, added_leaf)

    cdef _INode t_without(self, _Edit edit, int shift, int32_t hash_val, object key, _Box removed_leaf):
        cdef int idx = self._find_index(key)
        cdef _HashCollisionNode editable
        if idx == -1:
            return self
        removed_leaf.val = removed_leaf
        if self._count == 1:
            return None
        editable = self._ensure_editable(edit)
        editable._array[idx] = editable._array[2 * self._count - 2]
        editable._array[idx + 1] = editable._array[2 * self._count - 1]
        editable._array[2 * self._count - 2] = None
        editable._array[2 * self._count - 1] = None
        editable._count -= 1
        return editable


# --- BitmapIndexedNode ----------------------------------------------------

cdef class _BitmapIndexedNode(_INode):
    """Sparse hash node. Bit i of `bitmap` set ⇔ child i populated. `array`
    is 2*N elements: pairs of (key_or_null, val_or_node). A null key means
    the slot is a sub-node (stored in val_or_node)."""

    cdef uint32_t _bitmap
    cdef list _array
    cdef _Edit _edit

    def __cinit__(self, _Edit edit, bitmap=0, list array=None):
        self._edit = edit
        self._bitmap = <uint32_t>bitmap
        self._array = array if array is not None else []

    cdef int _idx_for(self, uint32_t bit):
        return _phm_popcount(self._bitmap & (bit - 1u))

    cdef _INode assoc(self, int shift, int32_t hash_val, object key, object val, _Box added_leaf):
        cdef uint32_t bit = _phm_bitpos(hash_val, shift)
        cdef int idx = self._idx_for(bit)
        cdef object key_or_null
        cdef object val_or_node
        cdef _INode n
        cdef int n_pop, jdx
        cdef list new_array, nodes
        cdef int i, j
        if (self._bitmap & bit) != 0:
            key_or_null = self._array[2 * idx]
            val_or_node = self._array[2 * idx + 1]
            if key_or_null is None:
                n = (<_INode>val_or_node).assoc(shift + 5, hash_val, key, val, added_leaf)
                if n is val_or_node:
                    return self
                return _BitmapIndexedNode(_NOEDIT, self._bitmap,
                                          _clone_set(self._array, 2 * idx + 1, n))
            if Util.equiv(key, key_or_null):
                if val is val_or_node:
                    return self
                return _BitmapIndexedNode(_NOEDIT, self._bitmap,
                                          _clone_set(self._array, 2 * idx + 1, val))
            added_leaf.val = added_leaf
            return _BitmapIndexedNode(_NOEDIT, self._bitmap,
                                      _clone_set2(self._array,
                                                  2 * idx, None,
                                                  2 * idx + 1,
                                                  _create_node(shift + 5, key_or_null, val_or_node, hash_val, key, val)))
        # Bit not set yet — grow.
        n_pop = _phm_popcount(self._bitmap)
        if n_pop >= 16:
            # Promote to ArrayNode.
            nodes = [None] * 32
            jdx = _phm_mask(hash_val, shift)
            nodes[jdx] = _BIN_EMPTY.assoc(shift + 5, hash_val, key, val, added_leaf)
            j = 0
            for i in range(32):
                if ((self._bitmap >> i) & 1u) != 0:
                    if self._array[j] is None:
                        nodes[i] = self._array[j + 1]
                    else:
                        nodes[i] = _BIN_EMPTY.assoc(shift + 5, Util.hasheq(self._array[j]),
                                                    self._array[j], self._array[j + 1],
                                                    added_leaf)
                    j += 2
            return _ArrayNode(_NOEDIT, n_pop + 1, nodes)
        # Fits in BIN: insert pair.
        new_array = list(self._array[:2 * idx]) + [key, val] + list(self._array[2 * idx:])
        added_leaf.val = added_leaf
        return _BitmapIndexedNode(_NOEDIT, self._bitmap | bit, new_array)

    cdef _INode without(self, int shift, int32_t hash_val, object key):
        cdef uint32_t bit = _phm_bitpos(hash_val, shift)
        cdef int idx
        cdef object key_or_null, val_or_node
        cdef _INode n
        if (self._bitmap & bit) == 0:
            return self
        idx = self._idx_for(bit)
        key_or_null = self._array[2 * idx]
        val_or_node = self._array[2 * idx + 1]
        if key_or_null is None:
            n = (<_INode>val_or_node).without(shift + 5, hash_val, key)
            if n is val_or_node:
                return self
            if n is not None:
                return _BitmapIndexedNode(_NOEDIT, self._bitmap,
                                          _clone_set(self._array, 2 * idx + 1, n))
            if self._bitmap == bit:
                return None
            return _BitmapIndexedNode(_NOEDIT, self._bitmap ^ bit, _remove_pair(self._array, idx))
        if Util.equiv(key, key_or_null):
            if self._bitmap == bit:
                return None
            return _BitmapIndexedNode(_NOEDIT, self._bitmap ^ bit, _remove_pair(self._array, idx))
        return self

    cdef object find(self, int shift, int32_t hash_val, object key, object not_found):
        cdef uint32_t bit = _phm_bitpos(hash_val, shift)
        cdef int idx
        cdef object key_or_null, val_or_node
        if (self._bitmap & bit) == 0:
            return not_found
        idx = self._idx_for(bit)
        key_or_null = self._array[2 * idx]
        val_or_node = self._array[2 * idx + 1]
        if key_or_null is None:
            return (<_INode>val_or_node).find(shift + 5, hash_val, key, not_found)
        if Util.equiv(key, key_or_null):
            return val_or_node
        return not_found

    cdef object find_entry(self, int shift, int32_t hash_val, object key):
        cdef uint32_t bit = _phm_bitpos(hash_val, shift)
        cdef int idx
        cdef object key_or_null, val_or_node
        if (self._bitmap & bit) == 0:
            return None
        idx = self._idx_for(bit)
        key_or_null = self._array[2 * idx]
        val_or_node = self._array[2 * idx + 1]
        if key_or_null is None:
            return (<_INode>val_or_node).find_entry(shift + 5, hash_val, key)
        if Util.equiv(key, key_or_null):
            return MapEntry(key_or_null, val_or_node)
        return None

    cdef object node_seq(self):
        return _NodeSeq.create(self._array)

    cdef object kv_reduce(self, f, init):
        return _node_seq_kv_reduce(self._array, f, init)

    cdef _BitmapIndexedNode _ensure_editable(self, _Edit edit):
        if self._edit is edit:
            return self
        cdef int n = _phm_popcount(self._bitmap)
        # Java reserves room for next assoc: 2*(n+1).
        cdef list new_array = list(self._array[:2 * n]) + [None] * 2
        # Pad to at least the original length so subsequent in-place writes work.
        while len(new_array) < len(self._array):
            new_array.append(None)
        return _BitmapIndexedNode(edit, self._bitmap, new_array)

    cdef _BitmapIndexedNode _edit_and_set(self, _Edit edit, int i, object a):
        cdef _BitmapIndexedNode editable = self._ensure_editable(edit)
        editable._array[i] = a
        return editable

    cdef _BitmapIndexedNode _edit_and_set2(self, _Edit edit, int i, object a, int j, object b):
        cdef _BitmapIndexedNode editable = self._ensure_editable(edit)
        editable._array[i] = a
        editable._array[j] = b
        return editable

    cdef _BitmapIndexedNode _edit_and_remove_pair(self, _Edit edit, uint32_t bit, int i):
        if self._bitmap == bit:
            return None
        cdef _BitmapIndexedNode editable = self._ensure_editable(edit)
        editable._bitmap ^= bit
        # Shift down: pair at (i+1) overwrites pair at i, etc.
        cdef int last = len(editable._array)
        cdef int k
        for k in range(2 * i, last - 2):
            editable._array[k] = editable._array[k + 2]
        editable._array[last - 2] = None
        editable._array[last - 1] = None
        return editable

    cdef _INode t_assoc(self, _Edit edit, int shift, int32_t hash_val, object key, object val, _Box added_leaf):
        cdef uint32_t bit = _phm_bitpos(hash_val, shift)
        cdef int idx = self._idx_for(bit)
        cdef object key_or_null, val_or_node
        cdef _INode n
        cdef _BitmapIndexedNode editable
        cdef int n_pop, jdx, i, j
        cdef list nodes
        cdef list new_array
        if (self._bitmap & bit) != 0:
            key_or_null = self._array[2 * idx]
            val_or_node = self._array[2 * idx + 1]
            if key_or_null is None:
                n = (<_INode>val_or_node).t_assoc(edit, shift + 5, hash_val, key, val, added_leaf)
                if n is val_or_node:
                    return self
                return self._edit_and_set(edit, 2 * idx + 1, n)
            if Util.equiv(key, key_or_null):
                if val is val_or_node:
                    return self
                return self._edit_and_set(edit, 2 * idx + 1, val)
            added_leaf.val = added_leaf
            return self._edit_and_set2(edit, 2 * idx, None,
                                       2 * idx + 1,
                                       _create_node_t(edit, shift + 5, key_or_null, val_or_node, hash_val, key, val))
        n_pop = _phm_popcount(self._bitmap)
        if n_pop * 2 < len(self._array):
            # Room in the existing array — grow in place.
            added_leaf.val = added_leaf
            editable = self._ensure_editable(edit)
            # Shift up to make room at 2*idx.
            for j in range(2 * n_pop - 1, 2 * idx - 1, -1):
                editable._array[j + 2] = editable._array[j]
            editable._array[2 * idx] = key
            editable._array[2 * idx + 1] = val
            editable._bitmap |= bit
            return editable
        if n_pop >= 16:
            # Promote.
            nodes = [None] * 32
            jdx = _phm_mask(hash_val, shift)
            nodes[jdx] = _BIN_EMPTY.t_assoc(edit, shift + 5, hash_val, key, val, added_leaf)
            j = 0
            for i in range(32):
                if ((self._bitmap >> i) & 1u) != 0:
                    if self._array[j] is None:
                        nodes[i] = self._array[j + 1]
                    else:
                        nodes[i] = _BIN_EMPTY.t_assoc(edit, shift + 5,
                                                      Util.hasheq(self._array[j]),
                                                      self._array[j],
                                                      self._array[j + 1],
                                                      added_leaf)
                    j += 2
            return _ArrayNode(edit, n_pop + 1, nodes)
        # Resize buffer.
        new_array = list(self._array[:2 * idx]) + [key, val] + list(self._array[2 * idx:2 * n_pop]) + [None] * (2 * 4 - 2)
        added_leaf.val = added_leaf
        editable = self._ensure_editable(edit)
        editable._array = new_array
        editable._bitmap |= bit
        return editable

    cdef _INode t_without(self, _Edit edit, int shift, int32_t hash_val, object key, _Box removed_leaf):
        cdef uint32_t bit = _phm_bitpos(hash_val, shift)
        cdef int idx
        cdef object key_or_null, val_or_node
        cdef _INode n
        if (self._bitmap & bit) == 0:
            return self
        idx = self._idx_for(bit)
        key_or_null = self._array[2 * idx]
        val_or_node = self._array[2 * idx + 1]
        if key_or_null is None:
            n = (<_INode>val_or_node).t_without(edit, shift + 5, hash_val, key, removed_leaf)
            if n is val_or_node:
                return self
            if n is not None:
                return self._edit_and_set(edit, 2 * idx + 1, n)
            if self._bitmap == bit:
                return None
            return self._edit_and_remove_pair(edit, bit, idx)
        if Util.equiv(key, key_or_null):
            removed_leaf.val = removed_leaf
            return self._edit_and_remove_pair(edit, bit, idx)
        return self


cdef _BitmapIndexedNode _BIN_EMPTY = _BitmapIndexedNode(_NOEDIT, 0, [])


# --- ArrayNode ------------------------------------------------------------

cdef class _ArrayNode(_INode):
    """Dense node: 32-element array of sub-INodes (None for empty slot).
    Used when ≥16 children populate; compacts back to BitmapIndexedNode at
    ≤8 children."""

    cdef int _count
    cdef list _array
    cdef _Edit _edit

    def __cinit__(self, _Edit edit, int count=0, list array=None):
        self._edit = edit
        self._count = count
        self._array = array if array is not None else [None] * 32

    cdef _INode assoc(self, int shift, int32_t hash_val, object key, object val, _Box added_leaf):
        cdef int idx = _phm_mask(hash_val, shift)
        cdef _INode node = self._array[idx]
        cdef _INode n
        if node is None:
            return _ArrayNode(_NOEDIT, self._count + 1,
                              _clone_set(self._array, idx,
                                         _BIN_EMPTY.assoc(shift + 5, hash_val, key, val, added_leaf)))
        n = node.assoc(shift + 5, hash_val, key, val, added_leaf)
        if n is node:
            return self
        return _ArrayNode(_NOEDIT, self._count, _clone_set(self._array, idx, n))

    cdef _INode without(self, int shift, int32_t hash_val, object key):
        cdef int idx = _phm_mask(hash_val, shift)
        cdef _INode node = self._array[idx]
        cdef _INode n
        if node is None:
            return self
        n = node.without(shift + 5, hash_val, key)
        if n is node:
            return self
        if n is None:
            if self._count <= 8:
                return self._pack(_NOEDIT, idx)
            return _ArrayNode(_NOEDIT, self._count - 1, _clone_set(self._array, idx, None))
        return _ArrayNode(_NOEDIT, self._count, _clone_set(self._array, idx, n))

    cdef object find(self, int shift, int32_t hash_val, object key, object not_found):
        cdef int idx = _phm_mask(hash_val, shift)
        cdef _INode node = self._array[idx]
        if node is None:
            return not_found
        return node.find(shift + 5, hash_val, key, not_found)

    cdef object find_entry(self, int shift, int32_t hash_val, object key):
        cdef int idx = _phm_mask(hash_val, shift)
        cdef _INode node = self._array[idx]
        if node is None:
            return None
        return node.find_entry(shift + 5, hash_val, key)

    cdef object node_seq(self):
        return _ArrayNodeSeq.create(self._array)

    cdef object kv_reduce(self, f, init):
        cdef int i
        for i in range(32):
            node = self._array[i]
            if node is not None:
                init = (<_INode>node).kv_reduce(f, init)
                if isinstance(init, Reduced):
                    return init
        return init

    cdef _ArrayNode _ensure_editable(self, _Edit edit):
        if self._edit is edit:
            return self
        return _ArrayNode(edit, self._count, list(self._array))

    cdef _ArrayNode _edit_and_set(self, _Edit edit, int i, _INode n):
        cdef _ArrayNode editable = self._ensure_editable(edit)
        editable._array[i] = n
        return editable

    cdef _INode _pack(self, _Edit edit, int idx):
        # Compact back to a BitmapIndexedNode, dropping the null slot at idx.
        cdef list new_array = [None] * (2 * (self._count - 1))
        cdef int j = 1
        cdef uint32_t bitmap = 0u
        cdef int i
        for i in range(idx):
            if self._array[i] is not None:
                new_array[j] = self._array[i]
                bitmap |= (<uint32_t>1u << i)
                j += 2
        for i in range(idx + 1, len(self._array)):
            if self._array[i] is not None:
                new_array[j] = self._array[i]
                bitmap |= (<uint32_t>1u << i)
                j += 2
        return _BitmapIndexedNode(edit, bitmap, new_array)

    cdef _INode t_assoc(self, _Edit edit, int shift, int32_t hash_val, object key, object val, _Box added_leaf):
        cdef int idx = _phm_mask(hash_val, shift)
        cdef _INode node = self._array[idx]
        cdef _ArrayNode editable
        cdef _INode n
        if node is None:
            editable = self._edit_and_set(edit, idx,
                                          _BIN_EMPTY.t_assoc(edit, shift + 5, hash_val, key, val, added_leaf))
            editable._count += 1
            return editable
        n = node.t_assoc(edit, shift + 5, hash_val, key, val, added_leaf)
        if n is node:
            return self
        return self._edit_and_set(edit, idx, n)

    cdef _INode t_without(self, _Edit edit, int shift, int32_t hash_val, object key, _Box removed_leaf):
        cdef int idx = _phm_mask(hash_val, shift)
        cdef _INode node = self._array[idx]
        cdef _INode n
        cdef _ArrayNode editable
        if node is None:
            return self
        n = node.t_without(edit, shift + 5, hash_val, key, removed_leaf)
        if n is node:
            return self
        if n is None:
            if self._count <= 8:
                return self._pack(edit, idx)
            editable = self._edit_and_set(edit, idx, None)
            editable._count -= 1
            return editable
        return self._edit_and_set(edit, idx, n)


# --- _create_node helpers (split key1+key2 at given shift) ---------------

cdef _INode _create_node(int shift, object key1, object val1, int32_t key2_hash, object key2, object val2):
    cdef int32_t key1_hash = Util.hasheq(key1)
    cdef _Box added_leaf
    cdef _Edit fresh_edit
    if key1_hash == key2_hash:
        return _HashCollisionNode(_NOEDIT, key1_hash, 2, [key1, val1, key2, val2])
    added_leaf = _Box(None)
    fresh_edit = _Edit(None)
    return (<_INode>_BIN_EMPTY).t_assoc(fresh_edit, shift, key1_hash, key1, val1, added_leaf
                                        ).t_assoc(fresh_edit, shift, key2_hash, key2, val2, added_leaf)


cdef _INode _create_node_t(_Edit edit, int shift, object key1, object val1, int32_t key2_hash, object key2, object val2):
    cdef int32_t key1_hash = Util.hasheq(key1)
    cdef _Box added_leaf
    if key1_hash == key2_hash:
        return _HashCollisionNode(_NOEDIT, key1_hash, 2, [key1, val1, key2, val2])
    added_leaf = _Box(None)
    return (<_INode>_BIN_EMPTY).t_assoc(edit, shift, key1_hash, key1, val1, added_leaf
                                        ).t_assoc(edit, shift, key2_hash, key2, val2, added_leaf)


# --- NodeSeq (walks BitmapIndexedNode / HashCollisionNode arrays) --------

cdef class _NodeSeq(ASeq):
    cdef list _array
    cdef int _i
    cdef object _s         # nested ISeq from a sub-node, or None

    def __cinit__(self, array=None, i=0, s=None):
        if array is None:
            return
        self._array = array
        self._i = i
        self._s = s

    @staticmethod
    cdef object create(list array):
        return _NodeSeq._create_at(array, 0, None)

    @staticmethod
    cdef object _create_at(list array, int i, object s):
        cdef int j
        if s is not None:
            return _NodeSeq(array, i, s)
        for j in range(i, len(array), 2):
            if array[j] is not None:
                return _NodeSeq(array, j, None)
            node = array[j + 1]
            if node is not None:
                node_seq = (<_INode>node).node_seq()
                if node_seq is not None:
                    return _NodeSeq(array, j + 2, node_seq)
        return None

    def first(self):
        if self._s is not None:
            return self._s.first()
        return MapEntry(self._array[self._i], self._array[self._i + 1])

    def next(self):
        if self._s is not None:
            return _NodeSeq._create_at(self._array, self._i, self._s.next())
        return _NodeSeq._create_at(self._array, self._i + 2, None)

    def with_meta(self, meta):
        cdef _NodeSeq s = _NodeSeq(self._array, self._i, self._s)
        s._meta = meta
        return s


cdef object _node_seq_kv_reduce(list array, f, init):
    cdef int i
    cdef object node
    for i in range(0, len(array), 2):
        if array[i] is not None:
            init = f(init, array[i], array[i + 1])
        else:
            node = array[i + 1]
            if node is not None:
                init = (<_INode>node).kv_reduce(f, init)
        if isinstance(init, Reduced):
            return init
    return init


# --- ArrayNodeSeq (walks ArrayNode children) -----------------------------

cdef class _ArrayNodeSeq(ASeq):
    cdef list _nodes
    cdef int _i
    cdef object _s

    def __cinit__(self, nodes=None, i=0, s=None):
        if nodes is None:
            return
        self._nodes = nodes
        self._i = i
        self._s = s

    @staticmethod
    cdef object create(list nodes):
        return _ArrayNodeSeq._create_at(nodes, 0, None)

    @staticmethod
    cdef object _create_at(list nodes, int i, object s):
        cdef int j
        if s is not None:
            return _ArrayNodeSeq(nodes, i, s)
        for j in range(i, len(nodes)):
            if nodes[j] is not None:
                node_seq = (<_INode>nodes[j]).node_seq()
                if node_seq is not None:
                    return _ArrayNodeSeq(nodes, j + 1, node_seq)
        return None

    def first(self):
        return self._s.first()

    def next(self):
        return _ArrayNodeSeq._create_at(self._nodes, self._i, self._s.next())

    def with_meta(self, meta):
        cdef _ArrayNodeSeq s = _ArrayNodeSeq(self._nodes, self._i, self._s)
        s._meta = meta
        return s


# --- PersistentHashMap ---------------------------------------------------

cdef PersistentHashMap _make_phm(object meta, int count, _INode root, bint has_null, object null_value):
    cdef PersistentHashMap m = PersistentHashMap.__new__(PersistentHashMap)
    m._meta = meta
    m._count = count
    m._root = root
    m._has_null = has_null
    m._null_value = null_value
    return m


cdef class PersistentHashMap:
    """A persistent map backed by a HAMT. O(log32 N) lookup/assoc."""

    cdef readonly int _count
    cdef _INode _root
    cdef readonly bint _has_null
    cdef readonly object _null_value
    cdef object _meta
    cdef int32_t _hash_cache
    cdef int32_t _hasheq_cache
    cdef object __weakref__

    def __cinit__(self):
        pass

    @staticmethod
    def create(*args):
        """PersistentHashMap.create(k1, v1, k2, v2, ...) | .create(dict)
        | .create(seq)  — the seq overload matches the JVM
        create(ISeq) signature: a single sequence of alternating
        keys/values."""
        if len(args) == 1:
            arg = args[0]
            if isinstance(arg, dict):
                d = arg
                t = _PHM_EMPTY.as_transient()
                for k, v in d.items():
                    t.assoc(k, v)
                return t.persistent()
            if isinstance(arg, (Seqable, ISeq)) or arg is None:
                # Single ISeq of alternating k/v
                s = RT.seq(arg)
                t = _PHM_EMPTY.as_transient()
                while s is not None:
                    k = s.first()
                    s = s.next()
                    if s is None:
                        raise ValueError(
                            "PersistentHashMap.create: odd number of items in seq")
                    v = s.first()
                    t.assoc(k, v)
                    s = s.next()
                return t.persistent()
        if len(args) % 2 != 0:
            raise ValueError("PersistentHashMap.create requires alternating key/value args")
        t = _PHM_EMPTY.as_transient()
        cdef int i
        for i in range(0, len(args), 2):
            t.assoc(args[i], args[i + 1])
        return t.persistent()

    @staticmethod
    def create_with_check(*args):
        """Like create, but raises on duplicate keys."""
        if len(args) % 2 != 0:
            raise ValueError("createWithCheck: items must come in pairs")
        t = _PHM_EMPTY.as_transient()
        cdef int i
        for i in range(0, len(args), 2):
            t.assoc(args[i], args[i + 1])
            if t.count() != i // 2 + 1:
                raise ValueError(f"Duplicate key: {args[i]!r}")
        return t.persistent()

    @staticmethod
    def from_iterable(iterable):
        """Build from an iterable of (key, val) pairs / MapEntries."""
        t = _PHM_EMPTY.as_transient()
        for entry in iterable:
            if isinstance(entry, MapEntry):
                t.assoc((<MapEntry>entry)._key, (<MapEntry>entry)._val)
            elif isinstance(entry, IMapEntry):
                t.assoc(entry.key(), entry.val())
            elif isinstance(entry, (list, tuple)) and len(entry) == 2:
                t.assoc(entry[0], entry[1])
            else:
                raise TypeError(f"Cannot use as map entry: {type(entry).__name__}")
        return t.persistent()

    # --- count / lookup ---

    def count(self):
        return self._count

    def __len__(self):
        return self._count

    def contains_key(self, key):
        if key is None:
            return self._has_null
        if self._root is None:
            return False
        cdef object _NF = _PHM_NOT_FOUND
        return self._root.find(0, Util.hasheq(key), key, _NF) is not _NF

    def entry_at(self, key):
        if key is None:
            return MapEntry(None, self._null_value) if self._has_null else None
        if self._root is None:
            return None
        return self._root.find_entry(0, Util.hasheq(key), key)

    def val_at(self, key, not_found=NOT_FOUND):
        cdef object miss = None if not_found is NOT_FOUND else not_found
        if key is None:
            return self._null_value if self._has_null else miss
        if self._root is None:
            return miss
        return self._root.find(0, Util.hasheq(key), key, miss)

    # --- assoc / without ---

    def assoc(self, key, val):
        cdef _Box added_leaf
        cdef _INode new_root
        cdef int new_count
        if key is None:
            if self._has_null and val is self._null_value:
                return self
            new_count = self._count if self._has_null else self._count + 1
            return _make_phm(self._meta, new_count, self._root, True, val)
        added_leaf = _Box(None)
        new_root = (_BIN_EMPTY if self._root is None else self._root
                    ).assoc(0, Util.hasheq(key), key, val, added_leaf)
        if new_root is self._root:
            return self
        new_count = self._count if added_leaf.val is None else self._count + 1
        return _make_phm(self._meta, new_count, new_root, self._has_null, self._null_value)

    def assoc_ex(self, key, val):
        if self.contains_key(key):
            raise ValueError(f"Key already present: {key!r}")
        return self.assoc(key, val)

    def without(self, key):
        cdef _INode new_root
        if key is None:
            if not self._has_null:
                return self
            return _make_phm(self._meta, self._count - 1, self._root, False, None)
        if self._root is None:
            return self
        new_root = self._root.without(0, Util.hasheq(key), key)
        if new_root is self._root:
            return self
        return _make_phm(self._meta, self._count - 1, new_root, self._has_null, self._null_value)

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
        if self._meta is None:
            return _PHM_EMPTY
        return _make_phm(self._meta, 0, None, False, None)

    # --- equality / hash ---

    def equiv(self, other):
        if other is self:
            return True
        return _phm_equiv(self, other)

    def __eq__(self, other):
        if other is self:
            return True
        return _phm_equiv(self, other)

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

    # --- IKVReduce ---

    def kv_reduce(self, f, init):
        if self._has_null:
            init = f(init, None, self._null_value)
            if isinstance(init, Reduced):
                return (<Reduced>init).deref()
        if self._root is not None:
            init = self._root.kv_reduce(f, init)
            if isinstance(init, Reduced):
                return (<Reduced>init).deref()
        return init

    # --- seq ---

    def seq(self):
        cdef object root_seq = self._root.node_seq() if self._root is not None else None
        if self._has_null:
            return Cons(MapEntry(None, self._null_value), root_seq)
        return root_seq

    # --- Python protocols ---

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
        cdef object _NF = _PHM_NOT_FOUND
        result = self.val_at(key, _NF)
        if result is _NF:
            raise KeyError(key)
        return result

    def __call__(self, key, not_found=NOT_FOUND):
        return self.val_at(key, not_found)

    def __bool__(self):
        return self._count > 0

    def meta(self):
        return self._meta

    def with_meta(self, meta):
        if self._meta is meta:
            return self
        return _make_phm(meta, self._count, self._root, self._has_null, self._null_value)

    def __str__(self):
        parts = []
        for entry in self:
            parts.append(_print_str(entry.key()) + " " + _print_str(entry.val()))
        return "{" + ", ".join(parts) + "}"

    def __repr__(self):
        return self.__str__()

    # --- IEditableCollection ---

    def as_transient(self):
        return TransientHashMap._from_persistent(self)


cdef object _PHM_NOT_FOUND = object()


cdef bint _phm_equiv(PersistentHashMap a, object other):
    cdef int i
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


IPersistentMap.register(PersistentHashMap)
Associative.register(PersistentHashMap)
ILookup.register(PersistentHashMap)
IPersistentCollection.register(PersistentHashMap)
Counted.register(PersistentHashMap)
IFn.register(PersistentHashMap)
IHashEq.register(PersistentHashMap)
IMeta.register(PersistentHashMap)
IObj.register(PersistentHashMap)
IKVReduce.register(PersistentHashMap)
IEditableCollection.register(PersistentHashMap)


# --- TransientHashMap ----------------------------------------------------

cdef class TransientHashMap:
    """Mutable companion to PersistentHashMap. Single-shot edit poison."""

    cdef _Edit _edit
    cdef _INode _root
    cdef int _count
    cdef bint _has_null
    cdef object _null_value
    cdef _Box _leaf_flag
    cdef object __weakref__

    def __cinit__(self):
        self._leaf_flag = _Box(None)

    @staticmethod
    cdef TransientHashMap _from_persistent(PersistentHashMap m):
        cdef TransientHashMap t = TransientHashMap.__new__(TransientHashMap)
        t._edit = _Edit(_threading.current_thread())
        t._root = m._root
        t._count = m._count
        t._has_null = m._has_null
        t._null_value = m._null_value
        t._leaf_flag = _Box(None)
        return t

    cdef void _ensure_editable(self) except *:
        if self._edit.thread is None:
            raise RuntimeError("Transient used after persistent! call")

    def assoc(self, key, val):
        self._ensure_editable()
        cdef _INode n
        if key is None:
            if self._null_value is not val:
                self._null_value = val
            if not self._has_null:
                self._count += 1
                self._has_null = True
            return self
        self._leaf_flag.val = None
        n = (_BIN_EMPTY if self._root is None else self._root
             ).t_assoc(self._edit, 0, Util.hasheq(key), key, val, self._leaf_flag)
        if n is not self._root:
            self._root = n
        if self._leaf_flag.val is not None:
            self._count += 1
        return self

    def without(self, key):
        self._ensure_editable()
        cdef _INode n
        if key is None:
            if not self._has_null:
                return self
            self._has_null = False
            self._null_value = None
            self._count -= 1
            return self
        if self._root is None:
            return self
        self._leaf_flag.val = None
        n = self._root.t_without(self._edit, 0, Util.hasheq(key), key, self._leaf_flag)
        if n is not self._root:
            self._root = n
        if self._leaf_flag.val is not None:
            self._count -= 1
        return self

    def val_at(self, key, not_found=NOT_FOUND):
        self._ensure_editable()
        cdef object miss = None if not_found is NOT_FOUND else not_found
        if key is None:
            return self._null_value if self._has_null else miss
        if self._root is None:
            return miss
        return self._root.find(0, Util.hasheq(key), key, miss)

    def contains_key(self, key):
        self._ensure_editable()
        if key is None:
            return self._has_null
        if self._root is None:
            return False
        cdef object _NF = _PHM_NOT_FOUND
        return self._root.find(0, Util.hasheq(key), key, _NF) is not _NF

    def entry_at(self, key):
        self._ensure_editable()
        if key is None:
            return MapEntry(None, self._null_value) if self._has_null else None
        if self._root is None:
            return None
        return self._root.find_entry(0, Util.hasheq(key), key)

    def count(self):
        self._ensure_editable()
        return self._count

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
        self._edit.thread = None
        return _make_phm(None, self._count, self._root, self._has_null, self._null_value)

    def __call__(self, key, not_found=NOT_FOUND):
        return self.val_at(key, not_found)


ITransientMap.register(TransientHashMap)
ITransientAssociative.register(TransientHashMap)
ITransientAssociative2.register(TransientHashMap)
ITransientCollection.register(TransientHashMap)
Counted.register(TransientHashMap)
ILookup.register(TransientHashMap)
IFn.register(TransientHashMap)


# --- the singleton EMPTY -------------------------------------------------

cdef PersistentHashMap _PHM_EMPTY = _make_phm(None, 0, None, False, None)
PERSISTENT_HASH_MAP_EMPTY = _PHM_EMPTY
