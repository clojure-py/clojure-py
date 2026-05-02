# Port of clojure.lang.MultiFn and clojure.lang.MethodImplCache, plus the
# default global hierarchy (an Atom holding a {:parents :ancestors
# :descendants} PHM).
#
# JVM MultiFn defers isa?/parents/derive to the same fns from clojure.core
# (looked up via RT.var). We don't have core.clj yet, so the hierarchy
# helpers here implement those operations directly.


cdef object _KW_PARENTS = Keyword.intern(None, "parents")
cdef object _KW_ANCESTORS = Keyword.intern(None, "ancestors")
cdef object _KW_DESCENDANTS = Keyword.intern(None, "descendants")


def make_hierarchy():
    """An empty hierarchy: {:parents {} :ancestors {} :descendants {}}."""
    return PersistentHashMap.create(
        _KW_PARENTS, _PHM_EMPTY,
        _KW_ANCESTORS, _PHM_EMPTY,
        _KW_DESCENDANTS, _PHM_EMPTY)


# Module-level Atom holding the default hierarchy. Mirrors Clojure's
# `clojure.core/global-hierarchy`.
global_hierarchy = Atom(make_hierarchy())


cdef _set_from(o):
    if o is None:
        return set()
    if isinstance(o, PersistentHashSet):
        return set(o)
    if isinstance(o, (set, frozenset)):
        return set(o)
    return set(o)


def parents_of(h, tag):
    """Immediate parents of `tag` in hierarchy `h`. Returns None if none."""
    pmap = h.val_at(_KW_PARENTS)
    if pmap is None:
        return None
    return pmap.val_at(tag)


def ancestors_of(h, tag):
    amap = h.val_at(_KW_ANCESTORS)
    if amap is None:
        return None
    return amap.val_at(tag)


def descendants_of(h, tag):
    dmap = h.val_at(_KW_DESCENDANTS)
    if dmap is None:
        return None
    return dmap.val_at(tag)


def isa_pred(h, child, parent):
    """Clojure-style `isa?` over hierarchy `h`.

    Holds when:
      - child == parent (Util.equiv)
      - child and parent are Python classes and issubclass(child, parent)
      - child and parent are vectors of equal length and each component isa?
      - parent is in (ancestors-of h child)
    """
    cdef int i
    if Util.equiv(child, parent):
        return True
    if isinstance(child, type) and isinstance(parent, type):
        return issubclass(child, parent)
    if isinstance(child, IPersistentVector) and isinstance(parent, IPersistentVector):
        if child.count() != parent.count():
            return False
        for i in range(child.count()):
            if not isa_pred(h, child.nth(i), parent.nth(i)):
                return False
        return True
    ancs = ancestors_of(h, child)
    if ancs is not None and ancs.contains(parent):
        return True
    return False


def derive(h, tag, parent):
    """Return a new hierarchy with `tag` derived from `parent`. Idempotent;
    raises on direct self-derivation or cycles."""
    if Util.equiv(tag, parent):
        raise ValueError(f"Cannot derive {tag!r} from itself")

    parents_map = h.val_at(_KW_PARENTS, _PHM_EMPTY)
    ancestors_map = h.val_at(_KW_ANCESTORS, _PHM_EMPTY)
    descendants_map = h.val_at(_KW_DESCENDANTS, _PHM_EMPTY)

    tag_parents = _set_from(parents_map.val_at(tag))
    if parent in tag_parents:
        return h    # already derived

    tag_ancestors = _set_from(ancestors_map.val_at(tag))
    if parent in tag_ancestors:
        raise ValueError(f"{tag!r} already has {parent!r} as ancestor")
    parent_ancestors = _set_from(ancestors_map.val_at(parent))
    if tag in parent_ancestors:
        raise ValueError(f"Cyclic derivation: {parent!r} has {tag!r} as ancestor")

    descendants_of_tag = _set_from(descendants_map.val_at(tag))

    # parents: tag → tag_parents ∪ {parent}
    new_parents = parents_map.assoc(tag,
        PersistentHashSet.from_iterable(tag_parents | {parent}))

    # ancestors: for each k in {tag} ∪ descendants(tag), add {parent} ∪ ancestors(parent)
    new_ancs_to_add = {parent} | parent_ancestors
    new_ancestors = ancestors_map
    for k in {tag} | descendants_of_tag:
        existing = _set_from(new_ancestors.val_at(k))
        merged = existing | new_ancs_to_add
        new_ancestors = new_ancestors.assoc(k, PersistentHashSet.from_iterable(merged))

    # descendants: for each k in {parent} ∪ ancestors(parent), add {tag} ∪ descendants(tag)
    new_descs_to_add = {tag} | descendants_of_tag
    new_descendants = descendants_map
    for k in {parent} | parent_ancestors:
        existing = _set_from(new_descendants.val_at(k))
        merged = existing | new_descs_to_add
        new_descendants = new_descendants.assoc(k, PersistentHashSet.from_iterable(merged))

    return PersistentHashMap.create(
        _KW_PARENTS, new_parents,
        _KW_ANCESTORS, new_ancestors,
        _KW_DESCENDANTS, new_descendants)


def underive(h, tag, parent):
    """Return a new hierarchy without the (tag → parent) edge. Conservative
    rebuild — recomputes ancestors/descendants from scratch from the
    remaining direct parents."""
    parents_map = h.val_at(_KW_PARENTS, _PHM_EMPTY)
    tag_parents = _set_from(parents_map.val_at(tag))
    if parent not in tag_parents:
        return h
    new_tag_parents = tag_parents - {parent}

    if not new_tag_parents:
        new_parents_map = parents_map.without(tag)
    else:
        new_parents_map = parents_map.assoc(tag,
            PersistentHashSet.from_iterable(new_tag_parents))

    # Rebuild ancestors/descendants from the new parents map.
    return _rebuild_from_parents(new_parents_map)


cdef object _rebuild_from_parents(parents_map):
    """Recompute :ancestors and :descendants from a :parents map via
    transitive closure."""
    # Direct parents per tag → Python dict for the transitive walk.
    direct = {}
    s = parents_map.seq()
    while s is not None:
        e = s.first()
        direct[e.key()] = _set_from(e.val())
        s = s.next()

    # Compute ancestors: closure over `direct`.
    ancestors = {}
    for tag in direct:
        seen = set()
        frontier = list(direct.get(tag, ()))
        while frontier:
            t = frontier.pop()
            if t in seen:
                continue
            seen.add(t)
            for p in direct.get(t, ()):
                if p not in seen:
                    frontier.append(p)
        ancestors[tag] = seen

    # descendants[k] = set of tags whose ancestors include k.
    descendants = {}
    for tag, ancs in ancestors.items():
        for a in ancs:
            descendants.setdefault(a, set()).add(tag)

    new_ancestors = _PHM_EMPTY
    for tag, ancs in ancestors.items():
        new_ancestors = new_ancestors.assoc(tag, PersistentHashSet.from_iterable(ancs))
    new_descendants = _PHM_EMPTY
    for k, descs in descendants.items():
        new_descendants = new_descendants.assoc(k, PersistentHashSet.from_iterable(descs))

    return PersistentHashMap.create(
        _KW_PARENTS, parents_map,
        _KW_ANCESTORS, new_ancestors,
        _KW_DESCENDANTS, new_descendants)


# --- MultiFn ------------------------------------------------------------

cdef class MultiFn(AFn):
    """Multimethod dispatch. dispatch_fn(*args) computes a dispatch value;
    add_method registers an implementation for a given dispatch value;
    invoking the MultiFn dispatches to the most specific matching method,
    using the underlying hierarchy and the prefer table."""

    cdef readonly str name
    cdef readonly object dispatch_fn
    cdef readonly object default_dispatch_val
    cdef readonly object hierarchy_ref          # IDeref → hierarchy map
    cdef object _rw
    cdef object _method_table       # PHM: dispatch_val → fn
    cdef object _prefer_table       # PHM: dispatch_val → PHS of preferred-over vals
    cdef object _method_cache       # PHM: dispatch_val → fn (resolved)
    cdef object _cached_hierarchy

    def __init__(self, name, dispatch_fn, default_dispatch_val=None,
                 hierarchy_ref=None):
        self.name = str(name)
        self.dispatch_fn = dispatch_fn
        self.default_dispatch_val = default_dispatch_val
        self.hierarchy_ref = hierarchy_ref if hierarchy_ref is not None else global_hierarchy
        self._rw = _RWLock()
        self._method_table = _PHM_EMPTY
        self._prefer_table = _PHM_EMPTY
        self._method_cache = self._method_table
        self._cached_hierarchy = None

    def reset(self):
        """Clear all methods, prefers, and the resolution cache."""
        self._rw.acquire_write()
        try:
            self._method_table = _PHM_EMPTY
            self._method_cache = _PHM_EMPTY
            self._prefer_table = _PHM_EMPTY
            self._cached_hierarchy = None
        finally:
            self._rw.release_write()
        return self

    def add_method(self, dispatch_val, method):
        self._rw.acquire_write()
        try:
            self._method_table = self._method_table.assoc(dispatch_val, method)
            self._reset_cache_locked()
        finally:
            self._rw.release_write()
        return self

    def remove_method(self, dispatch_val):
        self._rw.acquire_write()
        try:
            self._method_table = self._method_table.without(dispatch_val)
            self._reset_cache_locked()
        finally:
            self._rw.release_write()
        return self

    def prefer_method(self, x, y):
        self._rw.acquire_write()
        try:
            if self._prefers(self.hierarchy_ref.deref(), y, x):
                raise RuntimeError(
                    f"Preference conflict in multimethod '{self.name}': "
                    f"{y!r} is already preferred to {x!r}")
            existing = self._prefer_table.val_at(x, _PHS_EMPTY)
            self._prefer_table = self._prefer_table.assoc(x, existing.cons(y))
            self._reset_cache_locked()
        finally:
            self._rw.release_write()
        return self

    def get_method_table(self):
        return self._method_table

    def get_prefer_table(self):
        return self._prefer_table

    cdef bint _prefers(self, h, x, y) except *:
        # x preferred over y if directly (x's prefers contains y), or
        # transitively through y's parents or x's parents.
        xprefs = self._prefer_table.val_at(x)
        if xprefs is not None and xprefs.contains(y):
            return True
        ps = parents_of(h, y)
        if ps is not None:
            for p in ps:
                if self._prefers(h, x, p):
                    return True
        ps = parents_of(h, x)
        if ps is not None:
            for p in ps:
                if self._prefers(h, p, y):
                    return True
        return False

    cdef bint _dominates(self, h, x, y) except *:
        return self._prefers(h, x, y) or isa_pred(h, x, y)

    cdef _reset_cache_locked(self):
        # Caller holds the write lock.
        self._method_cache = self._method_table
        self._cached_hierarchy = self.hierarchy_ref.deref()

    def get_method(self, dispatch_val):
        if self._cached_hierarchy is not self.hierarchy_ref.deref():
            self._rw.acquire_write()
            try:
                self._reset_cache_locked()
            finally:
                self._rw.release_write()
        cached = self._method_cache.val_at(dispatch_val)
        if cached is not None:
            return cached
        return self._find_and_cache_best_method(dispatch_val)

    cdef object _find_and_cache_best_method(self, dispatch_val):
        # Snapshot under read lock.
        self._rw.acquire_read()
        try:
            mt = self._method_table
            pt = self._prefer_table
            ch = self._cached_hierarchy
            best_entry = None
            best_key = None
            s = mt.seq()
            while s is not None:
                e = s.first()
                k = e.key()
                if isa_pred(ch, dispatch_val, k):
                    if best_entry is None or self._dominates(ch, k, best_key):
                        best_entry = e
                        best_key = k
                    if not self._dominates(ch, best_key, k):
                        raise RuntimeError(
                            f"Multiple methods in multimethod '{self.name}' "
                            f"match dispatch value: {dispatch_val!r} -> "
                            f"{k!r} and {best_key!r}, and neither is preferred")
                s = s.next()
            if best_entry is None:
                best_value = mt.val_at(self.default_dispatch_val)
                if best_value is None:
                    return None
            else:
                best_value = best_entry.val()
        finally:
            self._rw.release_read()

        # Re-acquire write lock and verify the cache basis hasn't shifted.
        self._rw.acquire_write()
        try:
            if (mt is self._method_table
                    and pt is self._prefer_table
                    and ch is self._cached_hierarchy
                    and self._cached_hierarchy is self.hierarchy_ref.deref()):
                self._method_cache = self._method_cache.assoc(dispatch_val, best_value)
                return best_value
            else:
                self._reset_cache_locked()
        finally:
            self._rw.release_write()
        return self._find_and_cache_best_method(dispatch_val)

    def __call__(self, *args):
        dv = self.dispatch_fn(*args)
        target = self.get_method(dv)
        if target is None:
            raise RuntimeError(
                f"No method in multimethod '{self.name}' for dispatch value: {dv!r}")
        return target(*args)


# --- MethodImplCache ----------------------------------------------------

cdef class MethodImplCache:
    """Per-protocol method dispatch cache. Compiler-internal; ported for
    completeness so future Compiler work has it available."""

    cdef readonly object protocol
    cdef readonly Symbol sym
    cdef readonly object methodk     # Keyword
    cdef readonly int shift
    cdef readonly int mask
    cdef readonly object table       # list (or None) [cls, entry, cls, entry, ...]
    cdef readonly object map_        # dict (or None)
    cdef object _mre                 # most-recently-used Entry

    def __init__(self, sym, protocol, methodk, shift=0, mask=0,
                 table=None, map=None):
        self.sym = sym
        self.protocol = protocol
        self.methodk = methodk
        self.shift = shift
        self.mask = mask
        self.table = table
        self.map_ = map
        self._mre = None

    def fn_for(self, c):
        last = self._mre
        if last is not None and (<_MICEntry>last).c is c:
            return (<_MICEntry>last).fn
        return self._find_fn_for(c)

    cdef object _find_fn_for(self, c):
        cdef int idx
        if self.map_ is not None:
            e = self.map_.get(c)
            self._mre = e
            return e.fn if e is not None else None
        if self.table is not None:
            idx = ((Util.hash(c) >> self.shift) & self.mask) << 1
            if idx < len(self.table) and self.table[idx] is c:
                e = self.table[idx + 1]
                self._mre = e
                return e.fn if e is not None else None
        return None


cdef class _MICEntry:
    """One (class, fn) entry in a MethodImplCache."""
    cdef readonly object c
    cdef readonly object fn

    def __cinit__(self, c, fn):
        self.c = c
        self.fn = fn


# Expose Entry as MethodImplCache.Entry-style alias.
MethodImplCache_Entry = _MICEntry


IFn.register(MultiFn)
