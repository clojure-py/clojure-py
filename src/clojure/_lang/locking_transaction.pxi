# Port of clojure.lang.LockingTransaction — Software Transactional Memory.
#
# MVCC: each Ref carries a chain of timestamped values (TVal). Transactions
# get a monotonically-increasing readPoint at start; they see the most-recent
# TVal at-or-before their readPoint. On commit, write-locks are taken on
# affected refs, validators run, new TVal nodes are spliced in.
#
# Conflict / contention handling matches the JVM design:
#   - retry-on-conflict (read a ref that's been written after our readPoint
#     -> bump faults, throw RetryEx, retry)
#   - barge: older transactions can kill younger ones to avoid starvation
#   - blockAndBail: if another tx holds a write we want, wait briefly on its
#     latch, then retry
#
# Java uses ReentrantReadWriteLock per Ref; Python's stdlib has no RWLock,
# so a tiny one's defined here. Writer preference (waiting writers block new
# readers) prevents reader starvation.


import time as _time
import itertools as _itertools


# --- RW lock (writer preference) -----------------------------------------

cdef class _RWLock:
    """Reader-writer lock with writer preference. Both `acquire_read` and
    `acquire_write` may block; `try_acquire_write(timeout)` is a timed
    variant used by tryWriteLock / tryReadLock."""

    cdef object _cv
    cdef int _readers
    cdef bint _writer
    cdef int _waiting_writers
    cdef object __weakref__

    def __cinit__(self):
        cdef object cond_lock = Lock()
        self._cv = _threading.Condition(cond_lock)
        self._readers = 0
        self._writer = False
        self._waiting_writers = 0

    def acquire_read(self):
        with self._cv:
            while self._writer or self._waiting_writers > 0:
                self._cv.wait()
            self._readers += 1

    def release_read(self):
        with self._cv:
            self._readers -= 1
            if self._readers == 0:
                self._cv.notify_all()

    def acquire_write(self):
        with self._cv:
            self._waiting_writers += 1
            try:
                while self._writer or self._readers > 0:
                    self._cv.wait()
                self._writer = True
            finally:
                self._waiting_writers -= 1

    def try_acquire_write(self, double timeout):
        cdef double deadline = _time.monotonic() + timeout
        cdef double remaining
        with self._cv:
            self._waiting_writers += 1
            try:
                while self._writer or self._readers > 0:
                    remaining = deadline - _time.monotonic()
                    if remaining <= 0:
                        return False
                    self._cv.wait(timeout=remaining)
                self._writer = True
                return True
            finally:
                self._waiting_writers -= 1

    def release_write(self):
        with self._cv:
            self._writer = False
            self._cv.notify_all()


# --- internal helper objects ---------------------------------------------

class _LTRetryEx(BaseException):
    """Internal signal — caught at the top of the run loop to retry."""


class _LTAbortException(Exception):
    """User-thrown to abort the transaction with no retry."""


# Status codes for _LTInfo.status.
_LT_RUNNING = 0
_LT_COMMITTING = 1
_LT_RETRY = 2
_LT_KILLED = 3
_LT_COMMITTED = 4


cdef class _LTInfo:
    """Per-tx state visible to other transactions for barging / blocking."""

    cdef public int status
    cdef public long start_point
    cdef public object latch       # threading.Event — fires on stop
    cdef public object lock        # synchronizes status updates
    cdef object __weakref__

    def __cinit__(self, int status, long start_point):
        self.status = status
        self.start_point = start_point
        self.latch = _threading.Event()
        self.lock = Lock()

    def running(self):
        s = self.status
        return s == _LT_RUNNING or s == _LT_COMMITTING


cdef class _LTCFn:
    """A queued commute (fn + extra args)."""
    cdef public object fn
    cdef public object args        # ISeq or None

    def __cinit__(self, fn, args):
        self.fn = fn
        self.args = args


cdef class _LTNotify:
    """Pending watcher notification — buffered until after commit."""
    cdef public object ref
    cdef public object oldval
    cdef public object newval

    def __cinit__(self, ref, oldval, newval):
        self.ref = ref
        self.oldval = oldval
        self.newval = newval


# --- module-level state --------------------------------------------------

# Total order on transactions. itertools.count is thread-safe in CPython
# (atomic increment at the C level).
cdef object _LT_POINT_COUNTER = _itertools.count(1)
cdef object _LT_THREAD_LOCAL = _threading.local()


cdef long _next_point() except? -1:
    return next(_LT_POINT_COUNTER)


cdef object _get_current_lt():
    return getattr(_LT_THREAD_LOCAL, "lt", None)


cdef void _set_current_lt(object lt):
    if lt is None:
        try:
            del _LT_THREAD_LOCAL.lt
        except AttributeError:
            pass
    else:
        _LT_THREAD_LOCAL.lt = lt


# --- LockingTransaction --------------------------------------------------

cdef class LockingTransaction:
    """One in-flight STM transaction. See run / dosync."""

    RETRY_LIMIT = 10000
    LOCK_WAIT_SECS = 0.1            # 100ms (Java's LOCK_WAIT_MSECS)
    BARGE_WAIT_SECS = 0.01          # 10ms (Java's BARGE_WAIT_NANOS)

    cdef public object info         # _LTInfo or None
    cdef public long read_point
    cdef public long start_point
    cdef public double start_time
    cdef public object actions      # list of pending Agent actions
    cdef public object vals         # dict: ref -> tx-local val
    cdef public object sets         # set of refs written via doSet
    cdef public object commutes     # dict: ref -> list of _LTCFn
    cdef public object ensures      # set of refs holding readLock
    cdef object __weakref__

    def __cinit__(self):
        self.info = None
        self.read_point = 0
        self.start_point = 0
        self.start_time = 0.0
        self.actions = []
        self.vals = {}
        self.sets = set()
        self.commutes = {}
        self.ensures = set()

    # --- thread-local accessors ---

    @staticmethod
    def get_running():
        t = _get_current_lt()
        if t is None or (<LockingTransaction>t).info is None:
            return None
        return t

    @staticmethod
    def get_ex():
        t = LockingTransaction.get_running()
        if t is None:
            raise RuntimeError("No transaction running")
        return t

    @staticmethod
    def is_running():
        return LockingTransaction.get_running() is not None

    @staticmethod
    def run_in_transaction(fn):
        """Execute `fn` (a no-arg callable) inside a transaction. If a
        transaction is already running on this thread, just call fn."""
        t = _get_current_lt()
        if t is None:
            new_t = LockingTransaction()
            _set_current_lt(new_t)
            try:
                return new_t.run(fn)
            finally:
                _set_current_lt(None)
        if (<LockingTransaction>t).info is not None:
            return fn()
        return (<LockingTransaction>t).run(fn)

    # --- internal helpers ---

    cdef void _stop(self, int status):
        cdef _LTInfo info
        if self.info is not None:
            info = <_LTInfo>self.info
            with info.lock:
                info.status = status
                info.latch.set()
            self.info = None
            self.vals.clear()
            self.sets.clear()
            self.commutes.clear()

    cdef void _try_write_lock(self, ref):
        if not ref._lock.try_acquire_write(LockingTransaction.LOCK_WAIT_SECS):
            raise _LTRetryEx()

    cdef void _release_if_ensured(self, ref):
        if ref in self.ensures:
            self.ensures.discard(ref)
            ref._lock.release_read()

    cdef bint _bargeable(self, _LTInfo refinfo):
        # `True` if we successfully barge (kill) `refinfo`. Only allowed
        # if (a) we've been running long enough (BARGE_WAIT) and (b) we're
        # older than the other transaction.
        if (_time.monotonic() - self.start_time) <= LockingTransaction.BARGE_WAIT_SECS:
            return False
        if self.start_point >= refinfo.start_point:
            return False
        with refinfo.lock:
            if refinfo.status == _LT_RUNNING:
                refinfo.status = _LT_KILLED
                refinfo.latch.set()
                return True
        return False

    cdef object _block_and_bail(self, _LTInfo refinfo):
        # Stop, wait briefly on the other tx's latch, retry.
        self._stop(_LT_RETRY)
        refinfo.latch.wait(timeout=LockingTransaction.LOCK_WAIT_SECS)
        raise _LTRetryEx()

    cdef object _lock(self, ref):
        """Acquire write lock on ref; returns the most-recent committed val
        (or None if unbound). Resolves contention via barge/block."""
        self._release_if_ensured(ref)

        cdef bint unlocked = True
        cdef _LTInfo refinfo
        try:
            self._try_write_lock(ref)
            unlocked = False

            if ref._tvals is not None and (<_TVal>ref._tvals).point > self.read_point:
                raise _LTRetryEx()

            refinfo = ref._tinfo
            # Write-lock conflict.
            if refinfo is not None and refinfo is not self.info and refinfo.running():
                if not self._bargeable(refinfo):
                    ref._lock.release_write()
                    unlocked = True
                    return self._block_and_bail(refinfo)

            ref._tinfo = self.info
            return None if ref._tvals is None else (<_TVal>ref._tvals).val
        finally:
            if not unlocked:
                ref._lock.release_write()

    # --- transaction-scoped operations called by Ref ---

    cdef object _do_get(self, ref):
        if not (<_LTInfo>self.info).running():
            raise _LTRetryEx()
        if ref in self.vals:
            return self.vals[ref]
        ref._lock.acquire_read()
        try:
            if ref._tvals is None:
                raise RuntimeError(f"{ref} is unbound.")
            ver = <_TVal>ref._tvals
            head = ver
            while True:
                if ver.point <= self.read_point:
                    return ver.val
                ver = <_TVal>ver.prior
                if ver is head:
                    break
        finally:
            ref._lock.release_read()
        # No version of val precedes the read point: bump faults and retry.
        ref._faults_lock.acquire()
        try:
            ref._faults += 1
        finally:
            ref._faults_lock.release()
        raise _LTRetryEx()

    cdef object _do_set(self, ref, object val):
        if not (<_LTInfo>self.info).running():
            raise _LTRetryEx()
        if ref in self.commutes:
            raise RuntimeError("Can't set after commute")
        if ref not in self.sets:
            self.sets.add(ref)
            self._lock(ref)
        self.vals[ref] = val
        return val

    cdef void _do_ensure(self, ref):
        if not (<_LTInfo>self.info).running():
            raise _LTRetryEx()
        if ref in self.ensures:
            return
        ref._lock.acquire_read()
        # Someone wrote after our snapshot.
        if ref._tvals is not None and (<_TVal>ref._tvals).point > self.read_point:
            ref._lock.release_read()
            raise _LTRetryEx()
        cdef _LTInfo refinfo = ref._tinfo
        if refinfo is not None and refinfo.running():
            ref._lock.release_read()
            if refinfo is not self.info:
                self._block_and_bail(refinfo)
        else:
            self.ensures.add(ref)

    cdef object _do_commute(self, ref, fn, args):
        if not (<_LTInfo>self.info).running():
            raise _LTRetryEx()
        if ref not in self.vals:
            ref._lock.acquire_read()
            try:
                val = None if ref._tvals is None else (<_TVal>ref._tvals).val
            finally:
                ref._lock.release_read()
            self.vals[ref] = val
        fns = self.commutes.get(ref)
        if fns is None:
            fns = []
            self.commutes[ref] = fns
        fns.append(_LTCFn(fn, args))
        # Compute the commute now (provisional); will be re-applied at commit.
        cur = self.vals[ref]
        new_val = _apply_commute(fn, cur, args)
        self.vals[ref] = new_val
        return new_val

    # --- commit loop ---

    def run(self, fn):
        cdef bint done = False
        cdef object ret = None
        cdef list locked = []
        cdef list notify = []
        cdef int i, hcount
        cdef long commit_point
        cdef _TVal new_tval
        cdef _LTInfo refinfo
        cdef _LTInfo info_obj
        cdef Ref typed_ref
        cdef bint was_ensured

        for i in range(LockingTransaction.RETRY_LIMIT):
            if done:
                break
            try:
                self.read_point = _next_point()
                if i == 0:
                    self.start_point = self.read_point
                    self.start_time = _time.monotonic()
                self.info = _LTInfo(_LT_RUNNING, self.start_point)
                ret = fn()

                # Atomic flip RUNNING -> COMMITTING; if we've been killed, skip.
                info_obj = <_LTInfo>self.info
                with info_obj.lock:
                    if info_obj.status != _LT_RUNNING:
                        raise _LTRetryEx()
                    info_obj.status = _LT_COMMITTING

                # Process commutes (in ref-id order so locking is consistent).
                sorted_commutes = sorted(self.commutes.items(), key=_ref_id_key)
                for ref, cfns in sorted_commutes:
                    if ref in self.sets:
                        continue
                    was_ensured = ref in self.ensures
                    self._release_if_ensured(ref)
                    self._try_write_lock(ref)
                    locked.append(ref)
                    if was_ensured and ref._tvals is not None and (<_TVal>ref._tvals).point > self.read_point:
                        raise _LTRetryEx()
                    refinfo = ref._tinfo
                    if refinfo is not None and refinfo is not self.info and refinfo.running():
                        if not self._bargeable(refinfo):
                            raise _LTRetryEx()
                    val = None if ref._tvals is None else (<_TVal>ref._tvals).val
                    self.vals[ref] = val
                    for cfn in cfns:
                        cur = self.vals[ref]
                        self.vals[ref] = _apply_commute((<_LTCFn>cfn).fn, cur, (<_LTCFn>cfn).args)

                # Lock all refs we wrote to via doSet.
                for ref in self.sets:
                    self._try_write_lock(ref)
                    locked.append(ref)

                # Validate.
                for ref, new_val in self.vals.items():
                    (<Ref>ref)._validate((<Ref>ref)._validator, new_val)

                # Splice new TVals + collect notifications.
                commit_point = _next_point()
                for ref, new_val in self.vals.items():
                    typed_ref = <Ref>ref
                    oldval = None if typed_ref._tvals is None else typed_ref._tvals.val
                    hcount = typed_ref._hist_count()
                    if typed_ref._tvals is None:
                        new_tval = _TVal(new_val, commit_point, None)
                        typed_ref._tvals = new_tval
                    elif (typed_ref._faults > 0 and hcount < typed_ref._max_history) or hcount < typed_ref._min_history:
                        # Grow history.
                        typed_ref._tvals = _TVal(new_val, commit_point, typed_ref._tvals)
                        typed_ref._faults = 0
                    else:
                        # Reuse oldest slot in the cycle.
                        typed_ref._tvals = typed_ref._tvals.next_
                        typed_ref._tvals.val = new_val
                        typed_ref._tvals.point = commit_point
                    if typed_ref.get_watches().count() > 0:
                        notify.append(_LTNotify(typed_ref, oldval, new_val))

                done = True
                info_obj = <_LTInfo>self.info
                with info_obj.lock:
                    info_obj.status = _LT_COMMITTED

            except _LTRetryEx:
                pass
            finally:
                # Release locks in reverse order.
                for ref in reversed(locked):
                    ref._lock.release_write()
                locked.clear()
                for r in self.ensures:
                    r._lock.release_read()
                self.ensures.clear()
                self._stop(_LT_COMMITTED if done else _LT_RETRY)
                if done:
                    try:
                        for n in notify:
                            (<_LTNotify>n).ref.notify_watches((<_LTNotify>n).oldval, (<_LTNotify>n).newval)
                        for action in self.actions:
                            _agent_dispatch_action(action)
                    finally:
                        notify.clear()
                        self.actions = []
                else:
                    notify.clear()

        if not done:
            raise RuntimeError("Transaction failed after reaching retry limit")
        return ret

    def enqueue_action(self, action):
        """Used by Agent.dispatch_action: defer a send until commit."""
        self.actions.append(action)


# --- helpers (module-level cdefs) ----------------------------------------

cdef object _apply_commute(fn, cur, args):
    """Invoke commute fn with (cur, *args) — args is None or an ISeq."""
    if args is None:
        return fn(cur)
    arg_list = [cur]
    s = args
    while s is not None:
        arg_list.append(s.first())
        s = s.next()
    return fn(*arg_list)


def _ref_id_key(item):
    return item[0]._id


def _agent_dispatch_action(action):
    """Forward to Agent.dispatch_action — defined in agent.pxi (loaded
    later). Resolved via Python global lookup at call time."""
    Agent.dispatch_action(action)


def dosync(fn):
    """Convenience: run `fn` (no-arg callable) inside a transaction.

    Equivalent to LockingTransaction.run_in_transaction(fn)."""
    return LockingTransaction.run_in_transaction(fn)
