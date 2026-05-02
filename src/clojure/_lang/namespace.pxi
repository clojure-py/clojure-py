# Port of clojure.lang.Namespace.
#
# A Namespace maps Symbols (unqualified) to Vars (or imported classes /
# referred vars). Each Namespace has:
#   - a name (Symbol)
#   - a `mappings` map (Symbol → Var or Class)
#   - an `aliases` map (Symbol → Namespace)
#
# Namespaces are interned in a global registry by name; Namespace.find_or_create
# is the single entry point for getting a Namespace for a given Symbol.


cdef object _NAMESPACES = {}        # Symbol → Namespace
cdef object _NAMESPACES_LOCK = Lock()


cdef class Namespace(AReference):
    """Top-level symbol table — maps unqualified Symbols to Vars (or other
    referenced things)."""

    cdef readonly Symbol name
    cdef object _mappings_box       # length-1 list — single-cell holding a PHM
    cdef object _aliases_box
    cdef object _ns_lock

    def __init__(self, Symbol name):
        AReference.__init__(self, name.meta())
        self.name = name
        self._mappings_box = [_PHM_EMPTY]
        self._aliases_box = [_PHM_EMPTY]
        self._ns_lock = Lock()

    # --- registry ---

    @staticmethod
    def find_or_create(Symbol name):
        with _NAMESPACES_LOCK:
            ns = _NAMESPACES.get(name)
            if ns is not None:
                return ns
            new_ns = Namespace(name)
            _NAMESPACES[name] = new_ns
            return new_ns

    @staticmethod
    def find(Symbol name):
        return _NAMESPACES.get(name)

    @staticmethod
    def remove(Symbol name):
        with _NAMESPACES_LOCK:
            return _NAMESPACES.pop(name, None)

    @staticmethod
    def all():
        # Snapshot to avoid concurrent-modification surprises.
        with _NAMESPACES_LOCK:
            return list(_NAMESPACES.values())

    def get_name(self):
        return self.name

    # --- mappings: Symbol → Var / Class / aliased Var ---

    def get_mappings(self):
        return self._mappings_box[0]

    def get_mapping(self, Symbol sym):
        return self._mappings_box[0].val_at(sym)

    cdef bint _is_interned_mapping(self, Symbol sym, object o):
        """True iff `o` is a Var that this Namespace owns and whose own sym matches."""
        if not isinstance(o, Var):
            return False
        cdef Var v = <Var>o
        return v.ns is self and v.sym == sym

    def intern(self, Symbol sym):
        """Returns the Var for sym in this namespace, creating one if absent.
        If a referred (non-owned) mapping currently exists for sym, replaces it."""
        if sym.ns is not None:
            raise ValueError("Can't intern namespace-qualified symbol")
        with self._ns_lock:
            mappings = self._mappings_box[0]
            o = mappings.val_at(sym)
            if o is None:
                v = Var(self, sym)
                self._mappings_box[0] = mappings.assoc(sym, v)
                return v
            if self._is_interned_mapping(sym, o):
                return o
            # An aliased / referred mapping exists. Replace with our own Var.
            v = Var(self, sym)
            self._mappings_box[0] = mappings.assoc(sym, v)
            return v

    def unmap(self, Symbol sym):
        if sym.ns is not None:
            raise ValueError("Can't unintern namespace-qualified symbol")
        with self._ns_lock:
            self._mappings_box[0] = self._mappings_box[0].without(sym)

    def find_interned_var(self, Symbol sym):
        o = self._mappings_box[0].val_at(sym)
        if isinstance(o, Var) and (<Var>o).ns is self:
            return o
        return None

    cdef object _reference(self, Symbol sym, object val):
        """Install a non-interned mapping (refer / import). Replaces an
        existing mapping if different."""
        if sym.ns is not None:
            raise ValueError("Can't intern namespace-qualified symbol")
        with self._ns_lock:
            mappings = self._mappings_box[0]
            o = mappings.val_at(sym)
            if o is None:
                self._mappings_box[0] = mappings.assoc(sym, val)
                return val
            if o is val:
                return o
            self._mappings_box[0] = mappings.assoc(sym, val)
            return val

    def refer(self, Symbol sym, Var var):
        return self._reference(sym, var)

    def import_class(self, *args):
        """import_class(sym, cls) — register cls under sym in this ns.
        import_class(cls) — derive sym from cls.__name__."""
        if len(args) == 2:
            sym, cls = args
            return self._reference(sym, cls)
        if len(args) == 1:
            cls = args[0]
            sym = Symbol.intern(cls.__name__)
            return self._reference(sym, cls)
        raise TypeError(f"import_class takes 1 or 2 args, got {len(args)}")

    # --- aliases ---

    def get_aliases(self):
        return self._aliases_box[0]

    def lookup_alias(self, Symbol alias):
        return self._aliases_box[0].val_at(alias)

    def add_alias(self, Symbol alias, Namespace ns):
        if alias is None or ns is None:
            raise ValueError("Expecting Symbol + Namespace")
        with self._ns_lock:
            aliases = self._aliases_box[0]
            existing = aliases.val_at(alias)
            if existing is None:
                self._aliases_box[0] = aliases.assoc(alias, ns)
                return
            if existing is not ns:
                raise RuntimeError(
                    f"Alias {alias} already exists in namespace {self.name}, "
                    f"aliasing {existing}")

    def remove_alias(self, Symbol alias):
        with self._ns_lock:
            self._aliases_box[0] = self._aliases_box[0].without(alias)

    # --- repr ---

    def __str__(self):
        return str(self.name)

    def __repr__(self):
        return self.__str__()
