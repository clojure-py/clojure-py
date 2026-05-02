# Port of clojure.lang.Atom and clojure.lang.Volatile.
#
# Atom: validated, watch-able cell with CAS semantics. Java uses
# AtomicReference; we use a per-instance Lock to guard a cdef-class slot.
# `swap` is a spin-loop: read (unlocked), compute, lock + check unchanged +
# write. notifyWatches fires after a successful CAS, outside the lock.
#
# Volatile: a thin holder for a single object ref. No validator, no watches.
# Under CPython 3.14t, object-attribute writes on cdef-class fields are
# atomic stores at the C level, so a plain attribute is sufficient.


cdef class Atom(ARef):
    """Synchronized, validated, watched cell.

    swap(f, *args) → newv  (spin until CAS succeeds; f may be called >1×)
    swap_vals(f, *args)    → IPersistentVector [oldv, newv]
    compare_and_set(o, n)  → bool
    reset(newv)            → newv
    reset_vals(newv)       → IPersistentVector [oldv, newv]
    """

    cdef object _state
    cdef object _cas_lock

    def __init__(self, state, meta=None):
        ARef.__init__(self, meta)
        self._state = state
        self._cas_lock = Lock()

    def deref(self):
        return self._state

    cdef object _try_cas(self, object oldv, object newv):
        """Lock + check unchanged + write. Returns True on success."""
        with self._cas_lock:
            if self._state is oldv:
                self._state = newv
                return True
            return False

    def swap(self, f, *args):
        cdef object oldv, newv
        while True:
            oldv = self._state
            newv = f(oldv, *args)
            self._validate(self._validator, newv)
            if self._try_cas(oldv, newv):
                self.notify_watches(oldv, newv)
                return newv

    def swap_vals(self, f, *args):
        cdef object oldv, newv
        while True:
            oldv = self._state
            newv = f(oldv, *args)
            self._validate(self._validator, newv)
            if self._try_cas(oldv, newv):
                self.notify_watches(oldv, newv)
                return PersistentVector.create(oldv, newv)

    def compare_and_set(self, oldv, newv):
        self._validate(self._validator, newv)
        cdef bint ok = self._try_cas(oldv, newv)
        if ok:
            self.notify_watches(oldv, newv)
        return ok

    def reset(self, newval):
        self._validate(self._validator, newval)
        cdef object oldval
        with self._cas_lock:
            oldval = self._state
            self._state = newval
        self.notify_watches(oldval, newval)
        return newval

    def reset_vals(self, newv):
        self._validate(self._validator, newv)
        cdef object oldv
        # Spin-loop matches Java; the lock makes contention rare but a
        # concurrent writer between read-and-CAS could still happen.
        while True:
            oldv = self._state
            if self._try_cas(oldv, newv):
                self.notify_watches(oldv, newv)
                return PersistentVector.create(oldv, newv)

    def __str__(self):
        return f"#<Atom {self._state!r}>"

    def __repr__(self):
        return self.__str__()


IAtom.register(Atom)
IAtom2.register(Atom)
IRef.register(Atom)
IDeref.register(Atom)


cdef class Volatile:
    """Single mutable cell with volatile semantics. No validator, no watches."""

    cdef object _val
    cdef object __weakref__

    def __cinit__(self, val):
        self._val = val

    def deref(self):
        return self._val

    def reset(self, newval):
        self._val = newval
        return newval

    def __str__(self):
        return f"#<Volatile {self._val!r}>"

    def __repr__(self):
        return self.__str__()


IDeref.register(Volatile)
