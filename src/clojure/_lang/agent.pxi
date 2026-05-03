# Port of clojure.lang.Agent.
#
# An Agent holds an asynchronously-updated value. send / send-off enqueue
# an action; the action's fn runs on a thread pool, replaces the agent's
# state with its return value. Send dispatch in a running transaction is
# deferred until commit. Errors set the agent into a failure state which
# blocks subsequent sends until restart.
#
# Java uses two ExecutorServices: a fixed-size pool (`pooledExecutor`) for
# CPU-bound sends, and a cached pool (`soloExecutor`) for blocking-IO
# send-off calls. Python's concurrent.futures.ThreadPoolExecutor is fixed-
# size; "cached" maps to a generously-sized pool.

import os as _os
from concurrent.futures import ThreadPoolExecutor as _ThreadPoolExecutor


cdef object _AGENT_NESTED = _threading.local()
cdef object _CONTINUE_KW = Keyword.intern(None, "continue")
cdef object _FAIL_KW = Keyword.intern(None, "fail")


cdef object _get_nested():
    return getattr(_AGENT_NESTED, "vec", None)


cdef void _set_nested(v):
    if v is None:
        try:
            del _AGENT_NESTED.vec
        except AttributeError:
            pass
    else:
        _AGENT_NESTED.vec = v


# --- _ActionQueue -------------------------------------------------------

cdef class _ActionQueue:
    """Persistent (queue, error) pair."""
    cdef public object q       # PersistentQueue
    cdef public object error   # Throwable / Exception or None

    def __cinit__(self, q, error):
        self.q = q
        self.error = error


cdef _ActionQueue _AQ_EMPTY = _ActionQueue(_PQ_EMPTY, None)


# --- Action -------------------------------------------------------------

cdef class _Action:
    """One pending or running action against an agent."""
    cdef public Agent agent
    cdef public object fn
    cdef public object args        # ISeq or None
    cdef public object exec_       # the executor to run on

    def __cinit__(self, Agent agent, fn, args, exec_):
        self.agent = agent
        self.fn = fn
        self.args = args
        self.exec_ = exec_

    def execute(self):
        try:
            self.exec_.submit(_run_action, self)
        except Exception as error:
            handler = self.agent._error_handler
            if handler is not None:
                try:
                    handler(self.agent, error)
                except Exception:
                    pass


def _run_action(_Action action):
    """Executor target. Threads through nested-send tracking and error mode."""
    cdef _ActionQueue prior, nxt
    cdef bint popped
    _set_nested(_PV_EMPTY)
    try:
        error = None
        try:
            oldval = action.agent._state
            arg_list = [oldval]
            s = action.args
            while s is not None:
                arg_list.append(s.first())
                s = s.next()
            newval = action.fn(*arg_list)
            action.agent._set_state(newval)
            action.agent.notify_watches(oldval, newval)
        except Exception as e:
            error = e

        if error is None:
            release_pending_sends()
        else:
            _set_nested(None)   # let errorHandler send freely
            handler = action.agent._error_handler
            if handler is not None:
                try:
                    handler(action.agent, error)
                except Exception:
                    pass
            if action.agent._error_mode is _CONTINUE_KW:
                error = None

        # Pop self from the queue, install the error (if any) atomically.
        popped = False
        while not popped:
            with action.agent._aq_lock:
                prior = action.agent._aq
                nxt = _ActionQueue(prior.q.pop(), error)
                action.agent._aq = nxt
                popped = True

        if error is None and nxt.q.count() > 0:
            (<_Action>nxt.q.peek()).execute()
    finally:
        _set_nested(None)


# --- Agent --------------------------------------------------------------

# Module-level executors; created lazily and cached.
cdef object _AGENT_POOLED = None
cdef object _AGENT_SOLO = None
cdef object _AGENT_EXEC_LOCK = Lock()


def _get_pooled_executor():
    global _AGENT_POOLED
    if _AGENT_POOLED is None:
        with _AGENT_EXEC_LOCK:
            if _AGENT_POOLED is None:
                n = 2 + (_os.cpu_count() or 1)
                _AGENT_POOLED = _ThreadPoolExecutor(
                    max_workers=n,
                    thread_name_prefix="clojure-agent-send-pool")
    return _AGENT_POOLED


def _get_solo_executor():
    global _AGENT_SOLO
    if _AGENT_SOLO is None:
        with _AGENT_EXEC_LOCK:
            if _AGENT_SOLO is None:
                _AGENT_SOLO = _ThreadPoolExecutor(
                    max_workers=256,
                    thread_name_prefix="clojure-agent-send-off-pool")
    return _AGENT_SOLO


cdef class Agent(ARef):
    """Asynchronous, validated, watched cell. send / send_off dispatch
    actions onto a thread pool."""

    cdef public object _state
    cdef public _ActionQueue _aq
    cdef public object _aq_lock        # guards _aq updates
    cdef public object _error_mode     # :continue | :fail
    cdef public object _error_handler  # IFn or None

    def __init__(self, state, meta=None):
        ARef.__init__(self, meta)
        self._aq_lock = Lock()
        self._aq = _AQ_EMPTY
        self._error_mode = _CONTINUE_KW
        self._error_handler = None
        self._set_state(state)

    cdef bint _set_state(self, new_state) except *:
        self._validate(self._validator, new_state)
        cdef bint changed = self._state is not new_state
        self._state = new_state
        return changed

    def deref(self):
        return self._state

    def get_error(self):
        return self._aq.error

    def get_error_mode(self):
        return self._error_mode

    def set_error_mode(self, mode):
        if mode is not _CONTINUE_KW and mode is not _FAIL_KW:
            raise ValueError("error mode must be :continue or :fail")
        self._error_mode = mode

    def get_error_handler(self):
        return self._error_handler

    def set_error_handler(self, fn):
        self._error_handler = fn

    def get_queue_count(self):
        return self._aq.q.count()

    def restart(self, new_state, clear_actions=False):
        if self.get_error() is None:
            raise RuntimeError("Agent does not need a restart")
        self._validate(self._validator, new_state)
        self._state = new_state

        if clear_actions:
            with self._aq_lock:
                self._aq = _AQ_EMPTY
        else:
            with self._aq_lock:
                prior = self._aq
                self._aq = _ActionQueue(prior.q, None)
            if prior.q.count() > 0:
                (<_Action>prior.q.peek()).execute()
        return new_state

    # --- dispatch ---

    def dispatch(self, fn, args, exec_):
        error = self.get_error()
        if error is not None:
            raise RuntimeError("Agent is failed, needs restart") from error
        action = _Action(self, fn, args, exec_)
        Agent.dispatch_action(action)
        return self

    def send(self, fn, *args):
        arg_seq = IteratorSeq.from_iterable(args) if args else None
        return self.dispatch(fn, arg_seq, _get_pooled_executor())

    def send_off(self, fn, *args):
        arg_seq = IteratorSeq.from_iterable(args) if args else None
        return self.dispatch(fn, arg_seq, _get_solo_executor())

    @staticmethod
    def dispatch_action(action):
        """Route an Action to the right place: a running tx (deferred to
        commit), an in-flight action's nested vector (released on completion),
        or directly onto the agent's queue."""
        trans = LockingTransaction.get_running()
        if trans is not None:
            (<LockingTransaction>trans).enqueue_action(action)
            return
        nested = _get_nested()
        if nested is not None:
            _set_nested(nested.cons(action))
            return
        (<_Action>action).agent._enqueue(action)

    cdef void _enqueue(self, action):
        cdef _ActionQueue prior, nxt
        cdef bint queued = False
        while not queued:
            with self._aq_lock:
                prior = self._aq
                nxt = _ActionQueue(prior.q.cons(action), prior.error)
                self._aq = nxt
                queued = True
        if prior.q.count() == 0 and prior.error is None:
            (<_Action>action).execute()

    # --- shutdown ---

    @staticmethod
    def shutdown_executors():
        """Shut down both thread pools. Safe to call multiple times."""
        global _AGENT_POOLED, _AGENT_SOLO
        with _AGENT_EXEC_LOCK:
            if _AGENT_POOLED is not None:
                _AGENT_POOLED.shutdown(wait=True)
                _AGENT_POOLED = None
            if _AGENT_SOLO is not None:
                _AGENT_SOLO.shutdown(wait=True)
                _AGENT_SOLO = None

    # JVM-style static accessors. clojure.core uses these as
    # `Agent/pooledExecutor` etc. — on the JVM those are public static
    # fields that user code can `set!`. We don't have settable cdef
    # class fields, so the get/set pair stands in.

    @staticmethod
    def get_pooled_executor():
        return _get_pooled_executor()

    @staticmethod
    def set_pooled_executor(executor):
        global _AGENT_POOLED
        with _AGENT_EXEC_LOCK:
            _AGENT_POOLED = executor

    @staticmethod
    def get_solo_executor():
        return _get_solo_executor()

    @staticmethod
    def set_solo_executor(executor):
        global _AGENT_SOLO
        with _AGENT_EXEC_LOCK:
            _AGENT_SOLO = executor

    @staticmethod
    def release_pending_sends():
        """Static-method alias of the module-level release_pending_sends
        so clojure.core's `Agent/release_pending_sends` resolves."""
        return release_pending_sends()

    def __str__(self):
        return f"#<Agent {self._state!r}>"

    def __repr__(self):
        return self.__str__()


def release_pending_sends():
    """Called at the end of an action's body — drains the nested vector
    of sends performed during the action and enqueues each on its agent."""
    sends = _get_nested()
    if sends is None:
        return 0
    for i in range(sends.count()):
        a = sends.nth(i)
        (<_Action>a).agent._enqueue(a)
    _set_nested(_PV_EMPTY)
    return sends.count()


IRef.register(Agent)
IDeref.register(Agent)
