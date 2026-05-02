# Port of clojure.lang.Var, Binding, and the per-thread binding frame stack.
#
# A Var has:
#   - identity: (ns, sym)
#   - a `root` value (volatile)
#   - a `dynamic` flag — only dynamic vars accept thread bindings
#   - a `_thread_bound` flag that's flipped on first thread-binding push
#
# Per-thread state is stored on a `threading.local()` cell. Each thread sees
# a stack of Frame objects; each Frame is an immutable (bindings: Var→TBox,
# prev: Frame|None) snapshot pushed by `push_thread_bindings`.


import threading as _threading


# --- Binding (a tiny linked-list element) -------------------------------

cdef class Binding:
    """JVM Binding<T> — a single mutable val + next pointer."""

    cdef public object val
    cdef readonly Binding rest

    def __cinit__(self, val=None, Binding rest=None):
        self.val = val
        self.rest = rest


# --- Per-thread binding frame stack -------------------------------------

cdef class _TBox:
    """Java Var$TBox — a (thread, val) pair that lives in a thread frame."""

    cdef public object val
    cdef readonly object thread

    def __cinit__(self, thread, val):
        self.thread = thread
        self.val = val


cdef class _Frame:
    """Immutable snapshot of dynamic var bindings for one push level."""

    cdef readonly object bindings   # IPersistentMap (Var → _TBox)
    cdef readonly _Frame prev

    def __cinit__(self, bindings, _Frame prev):
        self.bindings = bindings
        self.prev = prev

    cdef _Frame clone_top(self):
        # Returns a copy with prev=None — used for transferring frames across
        # threads (e.g. for futures / agents).
        return _Frame(self.bindings, None)


cdef _Frame _FRAME_TOP = _Frame(_PHM_EMPTY, None)


cdef object _var_dvals = _threading.local()


cdef _Frame _get_frame():
    f = getattr(_var_dvals, 'frame', None)
    if f is None:
        return _FRAME_TOP
    return <_Frame>f


cdef void _set_frame(_Frame f):
    _var_dvals.frame = f


# --- Unbound sentinel ---------------------------------------------------

cdef class _Unbound(AFn):
    """Sentinel value installed as a Var's root when it's been interned but
    not yet given a value."""

    cdef readonly Var v

    def __cinit__(self, Var v):
        self.v = v

    def __call__(self, *args):
        raise RuntimeError(f"Attempting to call unbound fn: {self.v}")

    def __str__(self):
        return f"Unbound: {self.v}"

    def __repr__(self):
        return self.__str__()


# --- Var ---------------------------------------------------------------

cdef class Var(ARef):
    """Top-level mutable cell with thread-local-binding semantics. Implements
    IFn by delegating to its current value (which must itself be callable)."""

    cdef readonly Symbol sym
    cdef readonly object ns        # Namespace or None for anonymous vars
    cdef public object root
    cdef public bint dynamic
    cdef bint _thread_bound

    # Var is a Python class key in PHMs (uses identity hash/eq); needs
    # __weakref__ for the Namespace mapping to allow GC of unused vars.
    # ARef already declares __weakref__ via AReference.

    def __init__(self, ns=None, Symbol sym=None, root=None, _set_root=False):
        # _set_root distinguishes "no root provided → install Unbound" from
        # "root provided → install given (even if None)". Avoids ambiguity.
        ARef.__init__(self, _PHM_EMPTY if sym is None else _PHM_EMPTY)
        self.sym = sym
        self.ns = ns
        self.dynamic = False
        self._thread_bound = False
        if _set_root:
            self.root = root
        else:
            self.root = _Unbound(self)
        # Seed meta with :name and :ns keys, like Java's setMeta does.
        if sym is not None:
            self._set_meta_basics()

    cdef _set_meta_basics(self):
        m = self._meta if self._meta is not None else _PHM_EMPTY
        m = m.assoc(_KW_NAME, self.sym).assoc(_KW_NS, self.ns)
        self._meta = m

    # --- factories ---

    @staticmethod
    def create(root=None):
        """Anonymous var (no ns/sym)."""
        if root is None:
            return Var(None, None)
        return Var(None, None, root, _set_root=True)

    @staticmethod
    def intern(*args):
        """Var.intern(ns, sym) | Var.intern(ns, sym, root) | Var.intern(ns, sym, root, replace_root)
        | Var.intern(ns_name_symbol, sym): finds-or-creates the namespace, then interns."""
        if len(args) == 2:
            a, b = args
            if isinstance(a, Symbol):
                # ns_name as symbol — find or create the namespace.
                ns = Namespace.find_or_create(a)
                return ns.intern(b)
            return a.intern(b)
        if len(args) == 3:
            ns, sym, root = args
            v = ns.intern(sym)
            if not v.has_root():
                v.bind_root(root)
            else:
                # Default: replace_root = True
                v.bind_root(root)
            return v
        if len(args) == 4:
            ns, sym, root, replace_root = args
            v = ns.intern(sym)
            if not v.has_root() or replace_root:
                v.bind_root(root)
            return v
        raise TypeError(f"Var.intern takes 2-4 args, got {len(args)}")

    @staticmethod
    def find(Symbol ns_qualified_sym):
        if ns_qualified_sym.ns is None:
            raise ValueError("Symbol must be namespace-qualified")
        ns = Namespace.find(Symbol.intern(ns_qualified_sym.ns))
        if ns is None:
            raise ValueError(f"No such namespace: {ns_qualified_sym.ns}")
        return ns.find_interned_var(Symbol.intern(ns_qualified_sym.name))

    # --- dynamic / threadbound ---

    def set_dynamic(self, b=True):
        self.dynamic = b
        return self

    def is_dynamic(self):
        return self.dynamic

    def is_bound(self):
        if self.has_root():
            return True
        if self._thread_bound:
            return _get_frame().bindings.contains_key(self)
        return False

    # --- root ---

    def has_root(self):
        return not isinstance(self.root, _Unbound)

    def get_raw_root(self):
        return self.root

    def bind_root(self, root):
        self._validate(self._validator, root)
        old_root = self.root
        with self._ref_lock:
            self.root = root
        # Clear macro flag (matches Java).
        if isinstance(self._meta, IPersistentMap) and self._meta.contains_key(_KW_MACRO):
            with self._meta_lock:
                self._meta = self._meta.without(_KW_MACRO)
        self.notify_watches(old_root, root)

    def unbind_root(self):
        with self._ref_lock:
            self.root = _Unbound(self)

    def alter_root(self, fn, args):
        with self._ref_lock:
            arg_list = [self.root]
            if args is not None:
                if isinstance(args, ISeq):
                    s = args
                    while s is not None:
                        arg_list.append(s.first())
                        s = s.next()
                else:
                    arg_list.extend(args)
            new_root = fn(*arg_list)
            self._validate(self._validator, new_root)
            old_root = self.root
            self.root = new_root
        self.notify_watches(old_root, new_root)
        return new_root

    # --- deref / set ---

    def deref(self):
        b = self.get_thread_binding()
        if b is not None:
            return (<_TBox>b).val
        return self.root

    def get(self):
        if not self._thread_bound:
            return self.root
        return self.deref()

    def get_thread_binding(self):
        if not self._thread_bound:
            return None
        f = _get_frame()
        e = f.bindings.entry_at(self)
        if e is None:
            return None
        return e.val()

    def set(self, val):
        self._validate(self._validator, val)
        b = self.get_thread_binding()
        if b is not None:
            tb = <_TBox>b
            if _threading.current_thread() is not tb.thread:
                raise RuntimeError(f"Can't set!: {self.sym} from non-binding thread")
            tb.val = val
            return val
        raise RuntimeError(f"Can't change/establish root binding of: {self.sym} with set")

    # --- Settable ---

    def do_set(self, val):
        return self.set(val)

    def do_reset(self, val):
        self.bind_root(val)
        return val

    # --- meta keys: macro, private, tag ---

    def set_meta(self, m):
        """Replace meta, ensuring :name → sym and :ns → ns are still present.
        Mirrors Java's Var.setMeta."""
        if m is None:
            m = _PHM_EMPTY
        if not isinstance(m, IPersistentMap):
            raise TypeError(f"set_meta requires IPersistentMap, got {type(m).__name__}")
        new_meta = m.assoc(_KW_NAME, self.sym).assoc(_KW_NS, self.ns)
        self.reset_meta(new_meta)

    def set_macro(self):
        self.alter_meta(_assoc_fn, [_KW_MACRO, True])

    def is_macro(self):
        m = self._meta
        return bool(m and m.val_at(_KW_MACRO))

    def set_private(self, b=True):
        """Mark this var as private (sets :private → b in meta)."""
        self.alter_meta(_assoc_fn, [_KW_PRIVATE, b])

    def is_public(self):
        m = self._meta
        return not bool(m and m.val_at(_KW_PRIVATE))

    def set_tag(self, tag):
        """Set the :tag meta key. `tag` is typically a Symbol but any value
        is accepted."""
        self.alter_meta(_assoc_fn, [_KW_TAG, tag])

    def get_tag(self):
        m = self._meta
        return None if m is None else m.val_at(_KW_TAG)

    # --- IFn ---

    def __call__(self, *args):
        return self.deref()(*args)

    def apply_to(self, arglist):
        target = self.deref()
        if hasattr(target, 'apply_to'):
            return target.apply_to(arglist)
        # Plain callable: walk arglist into args.
        args = []
        if arglist is not None:
            s = arglist if isinstance(arglist, ISeq) else (arglist.seq() if isinstance(arglist, Seqable) else None)
            if s is not None:
                while s is not None:
                    args.append(s.first())
                    s = s.next()
            else:
                args = list(arglist)
        return target(*args)

    # --- thread-binding stack management ---

    @staticmethod
    def push_thread_bindings(bindings):
        """`bindings` is an IPersistentMap of Var → val."""
        f = _get_frame()
        bmap = f.bindings
        s = bindings.seq() if bindings is not None else None
        cdef Var v
        cdef object current_thread = _threading.current_thread()
        while s is not None:
            entry = s.first()
            v = entry.key()
            if not v.dynamic:
                raise RuntimeError(
                    f"Can't dynamically bind non-dynamic var: {v.ns}/{v.sym}")
            v._validate(v._validator, entry.val())
            v._thread_bound = True
            bmap = bmap.assoc(v, _TBox(current_thread, entry.val()))
            s = s.next()
        _set_frame(_Frame(bmap, f))

    @staticmethod
    def pop_thread_bindings():
        f = _get_frame()
        if f.prev is None:
            raise RuntimeError("Pop without matching push")
        if f.prev is _FRAME_TOP:
            try:
                del _var_dvals.frame
            except AttributeError:
                pass
        else:
            _set_frame(f.prev)

    @staticmethod
    def get_thread_bindings():
        """Returns a snapshot map of Var → current thread-bound value."""
        f = _get_frame()
        ret = _PHM_EMPTY
        s = f.bindings.seq()
        while s is not None:
            entry = s.first()
            v = entry.key()
            tb = entry.val()
            ret = ret.assoc(v, (<_TBox>tb).val)
            s = s.next()
        return ret

    @staticmethod
    def get_thread_binding_frame():
        return _get_frame()

    @staticmethod
    def clone_thread_binding_frame():
        return _get_frame().clone_top()

    @staticmethod
    def reset_thread_binding_frame(frame):
        _set_frame(<_Frame>frame)

    # --- repr ---

    def to_symbol(self):
        ns_name = None if self.ns is None else self.ns.name.name
        return Symbol.intern(ns_name, self.sym.name)

    def __str__(self):
        if self.ns is not None:
            return f"#'{self.ns.name}/{self.sym}"
        if self.sym is not None:
            return f"#<Var: {self.sym}>"
        return "#<Var: --unnamed-->"

    def __repr__(self):
        return self.__str__()


IFn.register(Var)
Settable.register(Var)


# --- module-level kw cache + helper fns -----------------------------------

cdef object _KW_NAME = Keyword.intern(None, "name")
cdef object _KW_NS = Keyword.intern(None, "ns")
cdef object _KW_MACRO = Keyword.intern(None, "macro")
cdef object _KW_PRIVATE = Keyword.intern(None, "private")
cdef object _KW_TAG = Keyword.intern(None, "tag")


cdef object _assoc_fn = lambda m, k, v: (m if m is not None else _PHM_EMPTY).assoc(k, v)


# --- thread-binding convenience ------------------------------------------

def with_bindings(bindings, fn):
    """Push `bindings` (a PHM of Var→val), call fn() with no args, pop on
    exit. Returns fn's result. Mirrors `clojure.core/with-bindings*`."""
    Var.push_thread_bindings(bindings)
    try:
        return fn()
    finally:
        Var.pop_thread_bindings()


def bound_fn(fn):
    """Capture the current dynamic-binding frame and return a wrapped fn
    that always runs under that frame, regardless of which thread invokes
    it. Mirrors `clojure.core/bound-fn*`.

    The captured frame is restored on entry and the caller's frame is
    re-installed on exit."""
    captured = Var.clone_thread_binding_frame()
    def bound(*args, **kwargs):
        prior = Var.get_thread_binding_frame()
        Var.reset_thread_binding_frame(captured)
        try:
            return fn(*args, **kwargs)
        finally:
            Var.reset_thread_binding_frame(prior)
    return bound
