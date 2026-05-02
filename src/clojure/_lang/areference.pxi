# Port of clojure.lang.AReference and ARef.
#
# AReference holds metadata with locked alter / reset operations. ARef
# layers a validator function and a map of watch callbacks on top.


cdef class AReference:
    """Concrete base for any reference type that carries metadata.
    Thread-safe meta access via a per-instance lock."""

    cdef object _meta
    cdef object _meta_lock
    cdef object __weakref__

    def __init__(self, meta=None):
        self._meta = meta
        self._meta_lock = Lock()

    def meta(self):
        with self._meta_lock:
            return self._meta

    def alter_meta(self, alter_fn, args):
        """Compute new_meta = alter_fn(current_meta, *args) and install it."""
        cdef list arg_list
        with self._meta_lock:
            arg_list = [self._meta]
            if args is not None:
                if isinstance(args, ISeq):
                    s = args
                    while s is not None:
                        arg_list.append(s.first())
                        s = s.next()
                elif isinstance(args, Seqable):
                    s = args.seq()
                    while s is not None:
                        arg_list.append(s.first())
                        s = s.next()
                else:
                    arg_list.extend(args)
            self._meta = alter_fn(*arg_list)
            return self._meta

    def reset_meta(self, m):
        with self._meta_lock:
            self._meta = m
            return m


IReference.register(AReference)
IMeta.register(AReference)


cdef class ARef(AReference):
    """Reference with a validator function and watch callbacks."""

    cdef object _validator
    cdef object _watches
    cdef object _ref_lock

    def __init__(self, meta=None):
        AReference.__init__(self, meta)
        self._validator = None
        self._watches = _PHM_EMPTY
        self._ref_lock = Lock()

    def deref(self):
        raise NotImplementedError("ARef subclass must implement deref")

    cdef _validate(self, vf, val):
        if vf is None:
            return
        try:
            ok = vf(val)
        except Exception as e:
            raise RuntimeError("Invalid reference state") from e
        if not ok:
            raise RuntimeError("Invalid reference state")

    def set_validator(self, vf):
        if vf is not None:
            self._validate(vf, self.deref())
        self._validator = vf

    def get_validator(self):
        return self._validator

    def get_watches(self):
        return self._watches

    def add_watch(self, key, callback):
        with self._ref_lock:
            self._watches = self._watches.assoc(key, callback)
        return self

    def remove_watch(self, key):
        with self._ref_lock:
            self._watches = self._watches.without(key)
        return self

    def notify_watches(self, old_val, new_val):
        ws = self._watches
        if ws.count() == 0:
            return
        s = ws.seq()
        while s is not None:
            entry = s.first()
            fn = entry.val()
            if fn is not None:
                fn(entry.key(), self, old_val, new_val)
            s = s.next()


IRef.register(ARef)
IDeref.register(ARef)
