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


cdef str _short_name(str dotted):
    # Last component of a dotted name. Pulled out because str.rsplit('.', 1)
    # crashes under Cython 3.2 + CPython 3.14t (likely a free-threading bug
    # in the optimized rsplit codegen). Manual rfind walks fine.
    cdef Py_ssize_t i = dotted.rfind(".")
    if i < 0:
        return dotted
    return dotted[i + 1:]


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

    # --- Python interop: importing values from modules -----------------
    #
    # Java's importClass binds a Class object under a Symbol; Python's
    # equivalent is more general — we bind any Python value (class,
    # function, module, instance, …). For dotted names (modules like
    # collections.abc) the derived alias is the last component.

    def import_class(self, *args):
        """import_class(sym, value)  — bind `value` under `sym` here.
        import_class(value) — derive the alias from value.__name__ (last
        dotted component for modules / submodules)."""
        if len(args) == 2:
            sym, val = args
            if not isinstance(sym, Symbol):
                raise TypeError(f"first arg must be Symbol, got {type(sym).__name__}")
            return self._reference(sym, val)
        if len(args) == 1:
            val = args[0]
            name = getattr(val, "__name__", None)
            if name is None:
                raise TypeError(
                    f"cannot derive name for {val!r}; pass an explicit Symbol")
            return self._reference(Symbol.intern(_short_name(name)), val)
        raise TypeError(f"import_class takes 1 or 2 args, got {len(args)}")

    def import_module(self, mod_name, alias=None):
        """Import a Python module by name and bind it here.

        `alias` may be a Symbol, a string, or None (derive from the last
        dotted component of `mod_name`)."""
        import importlib
        mod = importlib.import_module(mod_name)
        if alias is None:
            alias_sym = Symbol.intern(_short_name(mod_name))
        elif isinstance(alias, Symbol):
            alias_sym = alias
        elif isinstance(alias, str):
            alias_sym = Symbol.intern(alias)
        else:
            raise TypeError(f"alias must be Symbol or str, got {type(alias).__name__}")
        return self._reference(alias_sym, mod)

    def import_from(self, module_name, *names):
        """`from module_name import name1, name2, ...` — bind each name in
        this namespace.

        To rename a binding pass a 2-tuple: ('original', 'alias').
        Returns self for chaining."""
        import importlib
        mod = importlib.import_module(module_name)
        for entry in names:
            if isinstance(entry, tuple) and len(entry) == 2:
                orig, alias = entry
                if not isinstance(orig, str) or not isinstance(alias, str):
                    raise TypeError(
                        "import_from rename tuple must be (str, str)")
                val = getattr(mod, orig)
                self._reference(Symbol.intern(alias), val)
            elif isinstance(entry, str):
                val = getattr(mod, entry)
                self._reference(Symbol.intern(entry), val)
            else:
                raise TypeError(
                    f"import_from expects str or (str, str), got {type(entry).__name__}")
        return self

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
