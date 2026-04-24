//! Agents — Clojure's independent-state reference type, updated asynchronously
//! via action functions dispatched to a pool of worker threads.
//!
//! Each agent serializes its actions: only one action runs at a time per agent,
//! FIFO order. Two pools handle dispatch:
//! - `SEND_POOL` (fixed size, ~cpus + 2) — for CPU-bound actions via `send`.
//! - `SEND_OFF_POOL` (large fixed, cap 1024) — for I/O-bound actions via
//!   `send-off`. Emulates JVM's cached pool with a large fixed pool; keeps
//!   shutdown simple.
//!
//! Binding conveyance: `send`/`send-off` capture the caller's top thread-
//! binding frame at dispatch time. The worker installs it before invoking
//! the action. This matches vanilla's `binding-conveyor-fn` observably (our
//! workers start with empty binding stacks, so push-and-pop is equivalent
//! to the JVM clone/reset primitives).
//!
//! `*agent*` — a dynamic Var in `clojure.core` bound to the current agent
//! during action execution. Enables nested `send` inside an action to
//! detect "we're inside an agent" and queue appropriately (and in future,
//! to look up `release-pending-sends`). The Rust side caches `Py<Var>` in a
//! `OnceCell` populated on first dispatch.
//!
//! `IRef` polymorphism: like `Atom` and `Ref`, this pyclass duck-types
//! `#[pymethods]` (`add_watch`, `set_validator`, etc.) rather than
//! implementing a formal `IRef` protocol.

use crate::binding::{empty_frame, frame_assoc, Frame, BINDING_STACK};
use crate::exceptions::{IllegalArgumentException, IllegalStateException};
use crate::ideref::IDeref;
use crate::imeta::IMeta;
use crate::keyword::Keyword;
use crate::stm::txn::{PendingSend, CURRENT_TXN};
use arc_swap::ArcSwap;
use clojure_core_macros::implements;
use once_cell::sync::OnceCell;
use parking_lot::{Condvar, Mutex, RwLock};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyTuple};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;

type PyObject = Py<PyAny>;

#[derive(Clone, Copy, Debug)]
pub enum Executor {
    Send,
    SendOff,
    Custom, // `send-via` — the actual Python callable is stored separately.
}

/// A single action queued on an agent.
pub struct Action {
    pub f: PyObject,
    pub args: Vec<PyObject>,
    pub executor: Executor,
    /// Custom executor callable (only for `send-via`). Kept optional here so
    /// the Action struct is uniform.
    pub custom_executor: Option<PyObject>,
    pub binding_snapshot: Frame,
}

#[pyclass(module = "clojure._core", name = "Agent", frozen)]
pub struct Agent {
    pub id: u64,

    /// Current state. Lock-free reads via ArcSwap.
    pub state: ArcSwap<PyObject>,

    /// Pending actions (FIFO).
    pub queue: Mutex<VecDeque<Action>>,

    /// True iff a worker is currently draining this agent's queue.
    pub busy: AtomicBool,

    /// Parked error if the agent has failed. Visible via `agent-error`.
    pub error: ArcSwap<Option<PyObject>>,

    /// `:fail` or `:continue`. Stored as a PyObject (Keyword) for cheap
    /// identity-compare.
    pub error_mode: ArcSwap<PyObject>,

    /// User-installed error handler, or None.
    pub error_handler: ArcSwap<Option<PyObject>>,

    /// IRef bits.
    pub meta: ArcSwap<Option<PyObject>>,
    pub validator: ArcSwap<Option<PyObject>>,
    pub watches: RwLock<Py<PyDict>>,

    /// Pending-action counter + condvar, for `await`/`await-for`.
    pub pending: Mutex<u64>,
    pub pending_cv: Condvar,
}

// --- Executor pool ---

pub type Job = Box<dyn FnOnce() + Send + 'static>;

pub struct ExecutorPool {
    tx: mpsc::Sender<Option<Job>>,
    _workers: Vec<JoinHandle<()>>,
    shutdown: Arc<AtomicBool>,
}

impl ExecutorPool {
    pub fn new(n_workers: usize) -> Self {
        let (tx, rx) = mpsc::channel::<Option<Job>>();
        let rx = Arc::new(parking_lot::Mutex::new(rx));
        let shutdown = Arc::new(AtomicBool::new(false));
        let mut workers = Vec::with_capacity(n_workers);
        for _ in 0..n_workers {
            let rx = rx.clone();
            let sd = shutdown.clone();
            workers.push(std::thread::spawn(move || loop {
                if sd.load(Ordering::Acquire) {
                    break;
                }
                let job = {
                    let lock = rx.lock();
                    match lock.recv() {
                        Ok(Some(j)) => j,
                        Ok(None) | Err(_) => break,
                    }
                };
                job();
            }));
        }
        Self { tx, _workers: workers, shutdown }
    }

    pub fn execute(&self, job: Job) {
        let _ = self.tx.send(Some(job));
    }

    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Release);
        // Send a stop sentinel for each worker. Workers may exit on recv
        // error too (if the channel closes), but explicit sentinels are
        // cleaner.
        for _ in 0..self._workers.len() {
            let _ = self.tx.send(None);
        }
    }
}

pub static SEND_POOL: OnceCell<ExecutorPool> = OnceCell::new();
pub static SEND_OFF_POOL: OnceCell<ExecutorPool> = OnceCell::new();

fn send_pool() -> &'static ExecutorPool {
    SEND_POOL.get_or_init(|| {
        let n = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            + 2;
        ExecutorPool::new(n)
    })
}

fn send_off_pool() -> &'static ExecutorPool {
    SEND_OFF_POOL.get_or_init(|| ExecutorPool::new(32))
}

/// Pool used for `send-off` and `future` (matches vanilla which routes
/// both through `clojure.lang.Agent.soloExecutor`).
pub fn future_pool() -> &'static ExecutorPool {
    send_off_pool()
}

// --- *agent* Var cache ---

pub static AGENT_STAR_VAR: OnceCell<Py<crate::var::Var>> = OnceCell::new();

fn lookup_agent_star_var(py: Python<'_>) -> Option<Py<crate::var::Var>> {
    if let Some(v) = AGENT_STAR_VAR.get() {
        return Some(v.clone_ref(py));
    }
    // Resolve (clojure.core/*agent*).
    let sys = py.import("sys").ok()?;
    let modules = sys.getattr("modules").ok()?;
    let ns = modules.get_item("clojure.core").ok()?;
    let var = ns.getattr("*agent*").ok()?.cast::<crate::var::Var>().ok()?.clone().unbind();
    let _ = AGENT_STAR_VAR.set(var.clone_ref(py));
    Some(var)
}

// --- Agent impl ---

fn keyword(py: Python<'_>, name: &str) -> PyResult<PyObject> {
    let kw = crate::keyword::keyword(py, name, None)?;
    Ok(kw.into_any())
}

impl Agent {
    pub fn new(py: Python<'_>, initial: PyObject) -> PyResult<Self> {
        Ok(Self {
            id: crate::rt::next_id(),
            state: ArcSwap::new(Arc::new(initial)),
            queue: Mutex::new(VecDeque::new()),
            busy: AtomicBool::new(false),
            error: ArcSwap::new(Arc::new(None)),
            error_mode: ArcSwap::new(Arc::new(keyword(py, "fail")?)),
            error_handler: ArcSwap::new(Arc::new(None)),
            meta: ArcSwap::new(Arc::new(None)),
            validator: ArcSwap::new(Arc::new(None)),
            watches: RwLock::new(PyDict::new(py).unbind()),
            pending: Mutex::new(0),
            pending_cv: Condvar::new(),
        })
    }

    fn validate(&self, py: Python<'_>, v: &PyObject) -> PyResult<()> {
        let validator = {
            let g = self.validator.load();
            let opt: &Option<PyObject> = &g;
            opt.as_ref().map(|o| o.clone_ref(py))
        };
        if let Some(vf) = validator {
            let r = vf.bind(py).call1((v.clone_ref(py),))?;
            if !r.is_truthy()? {
                return Err(IllegalArgumentException::new_err(
                    "Invalid reference state",
                ));
            }
        }
        Ok(())
    }

    // Rust-side accessors (parallel to the pymethods, since #[pymethods] are
    // Python-only by default). Used by the RT shims in rt_ns.rs.
    pub fn install_validator(&self, py: Python<'_>, validator: Option<PyObject>) -> PyResult<()> {
        if let Some(vf) = validator.as_ref() {
            let cur = {
                let g = self.state.load();
                let v: &PyObject = &g;
                v.clone_ref(py)
            };
            let r = vf.bind(py).call1((cur,))?;
            if !r.is_truthy()? {
                return Err(IllegalArgumentException::new_err(
                    "Invalid reference state",
                ));
            }
        }
        self.validator.store(Arc::new(validator));
        Ok(())
    }

    pub fn install_error_handler(&self, f: Option<PyObject>) {
        self.error_handler.store(Arc::new(f));
    }

    pub fn install_error_mode(&self, mode: PyObject) {
        self.error_mode.store(Arc::new(mode));
    }

    pub fn read_error(&self, py: Python<'_>) -> PyObject {
        let g = self.error.load();
        let opt: &Option<PyObject> = &g;
        opt.as_ref().map(|o| o.clone_ref(py)).unwrap_or_else(|| py.None())
    }

    pub fn read_error_handler(&self, py: Python<'_>) -> PyObject {
        let g = self.error_handler.load();
        let opt: &Option<PyObject> = &g;
        opt.as_ref().map(|o| o.clone_ref(py)).unwrap_or_else(|| py.None())
    }

    pub fn read_error_mode(&self, py: Python<'_>) -> PyObject {
        let g = self.error_mode.load();
        let v: &PyObject = &g;
        v.clone_ref(py)
    }

    fn fire_watches(
        &self,
        py: Python<'_>,
        slf: &Py<Agent>,
        old: PyObject,
        new: PyObject,
    ) -> PyResult<()> {
        let watches_snapshot: Vec<(PyObject, PyObject)> = {
            let guard = self.watches.read();
            guard.bind(py).iter().map(|(k, v)| (k.unbind(), v.unbind())).collect()
        };
        for (k, f) in watches_snapshot {
            f.bind(py).call1((k, slf.clone_ref(py), old.clone_ref(py), new.clone_ref(py)))?;
        }
        Ok(())
    }
}

#[pymethods]
impl Agent {
    fn __repr__(slf: Py<Self>, py: Python<'_>) -> PyResult<String> {
        let this = slf.bind(py).get();
        let g = this.state.load();
        let v: &PyObject = &g;
        let s = v.bind(py).repr()?.extract::<String>()?;
        Ok(format!("#<Agent {}>", s))
    }

    #[getter(meta)]
    fn get_meta(&self, py: Python<'_>) -> PyObject {
        let g = self.meta.load();
        let opt: &Option<PyObject> = &g;
        opt.as_ref().map(|o| o.clone_ref(py)).unwrap_or_else(|| py.None())
    }

    fn set_validator(&self, py: Python<'_>, validator: Option<PyObject>) -> PyResult<()> {
        if let Some(vf) = validator.as_ref() {
            let cur = {
                let g = self.state.load();
                let v: &PyObject = &g;
                v.clone_ref(py)
            };
            let r = vf.bind(py).call1((cur,))?;
            if !r.is_truthy()? {
                return Err(IllegalArgumentException::new_err(
                    "Invalid reference state",
                ));
            }
        }
        self.validator.store(Arc::new(validator));
        Ok(())
    }

    fn get_validator(&self, py: Python<'_>) -> Option<PyObject> {
        let g = self.validator.load();
        let opt: &Option<PyObject> = &g;
        opt.as_ref().map(|o| o.clone_ref(py))
    }

    fn add_watch(&self, py: Python<'_>, key: PyObject, f: PyObject) -> PyResult<()> {
        let guard = self.watches.read();
        guard.bind(py).set_item(key, f)?;
        Ok(())
    }

    fn remove_watch(&self, py: Python<'_>, key: PyObject) -> PyResult<()> {
        let guard = self.watches.read();
        guard.bind(py).del_item(key)?;
        Ok(())
    }

    fn get_error(&self, py: Python<'_>) -> PyObject {
        let g = self.error.load();
        let opt: &Option<PyObject> = &g;
        opt.as_ref().map(|o| o.clone_ref(py)).unwrap_or_else(|| py.None())
    }

    fn set_error_handler(&self, f: Option<PyObject>) {
        self.error_handler.store(Arc::new(f));
    }

    fn get_error_handler(&self, py: Python<'_>) -> PyObject {
        let g = self.error_handler.load();
        let opt: &Option<PyObject> = &g;
        opt.as_ref().map(|o| o.clone_ref(py)).unwrap_or_else(|| py.None())
    }

    fn set_error_mode(&self, mode: PyObject) {
        self.error_mode.store(Arc::new(mode));
    }

    fn get_error_mode(&self, py: Python<'_>) -> PyObject {
        let g = self.error_mode.load();
        let v: &PyObject = &g;
        v.clone_ref(py)
    }
}

#[implements(IDeref)]
impl IDeref for Agent {
    fn deref(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let g = this.bind(py).get().state.load();
        let v: &PyObject = &g;
        Ok(v.clone_ref(py))
    }
}

#[implements(IMeta)]
impl IMeta for Agent {
    fn meta(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let g = this.bind(py).get().meta.load();
        let opt: &Option<PyObject> = &g;
        Ok(opt.as_ref().map(|m| m.clone_ref(py)).unwrap_or_else(|| py.None()))
    }
    fn with_meta(this: Py<Self>, py: Python<'_>, meta: PyObject) -> PyResult<PyObject> {
        let a = this.bind(py).get();
        let m = if meta.is_none(py) { None } else { Some(meta) };
        a.meta.store(Arc::new(m));
        Ok(this.into_any())
    }
}

// --- Dispatch ---

/// Entry point used by `(send a f & args)`, `(send-off a f & args)`, and
/// `(send-via ex a f & args)`. Captures binding frame + routes through
/// the current transaction if one is active.
pub fn dispatch(
    py: Python<'_>,
    agent: Py<Agent>,
    exec: Executor,
    custom: Option<PyObject>,
    f: PyObject,
    args: Vec<PyObject>,
) -> PyResult<Py<Agent>> {
    // 1. Capture binding snapshot.
    let snap: Frame = BINDING_STACK
        .with(|s| s.borrow().last().map(|f| f.clone_ref(py)))
        .unwrap_or_else(|| empty_frame(py).unwrap());

    // 2. If currently in an STM transaction, defer dispatch until commit.
    let in_txn = CURRENT_TXN.with(|c| c.borrow().is_some());
    if in_txn {
        let exec_kind = match exec {
            Executor::Send => 0u8,
            Executor::SendOff => 1,
            Executor::Custom => 2,
        };
        let pending = PendingSend {
            agent: agent.clone_ref(py).into_any(),
            f,
            args,
            executor_kind: exec_kind,
            custom_executor: custom,
            binding_snapshot: snap,
        };
        CURRENT_TXN.with(|c| {
            if let Some(t) = c.borrow().as_ref() {
                t.agent_sends.borrow_mut().push(pending);
            }
        });
        return Ok(agent);
    }

    // 3. Reject if agent is failed.
    {
        let g = agent.bind(py).get().error.load();
        let opt: &Option<PyObject> = &g;
        if opt.is_some() {
            return Err(IllegalStateException::new_err(
                "Agent is failed, needs restart",
            ));
        }
    }

    let action = Action {
        f,
        args,
        executor: exec,
        custom_executor: custom,
        binding_snapshot: snap,
    };
    enqueue(py, &agent, action);
    Ok(agent)
}

/// Called by `stm::txn::commit` to dispatch pending sends from a committed
/// transaction.
pub(crate) fn dispatch_from_commit(py: Python<'_>, s: PendingSend) -> PyResult<()> {
    let agent_any = s.agent;
    let agent = agent_any.bind(py).downcast::<Agent>().map_err(|_| {
        IllegalStateException::new_err(
            "Internal: pending agent-send payload is not an Agent",
        )
    })?.clone().unbind();

    // Error check — if the agent failed between dispatch and commit, drop.
    {
        let g = agent.bind(py).get().error.load();
        let opt: &Option<PyObject> = &g;
        if opt.is_some() {
            return Ok(());
        }
    }

    let executor = match s.executor_kind {
        0 => Executor::Send,
        1 => Executor::SendOff,
        _ => Executor::Custom,
    };
    let action = Action {
        f: s.f,
        args: s.args,
        executor,
        custom_executor: s.custom_executor,
        binding_snapshot: s.binding_snapshot,
    };
    enqueue(py, &agent, action);
    Ok(())
}

fn enqueue(py: Python<'_>, agent: &Py<Agent>, action: Action) {
    let this = agent.bind(py).get();
    {
        let mut p = this.pending.lock();
        *p += 1;
    }
    let executor = action.executor;
    let custom = action.custom_executor.as_ref().map(|e| e.clone_ref(py));
    {
        let mut q = this.queue.lock();
        q.push_back(action);
    }
    // Schedule only if nobody is currently draining.
    if !this.busy.swap(true, Ordering::AcqRel) {
        schedule_drain(executor, custom, py, agent.clone_ref(py));
    }
}

fn schedule_drain(
    exec: Executor,
    custom: Option<PyObject>,
    py: Python<'_>,
    agent: Py<Agent>,
) {
    // Custom executor path: build a zero-arg Python callable that calls
    // `drain_one(agent)` when invoked, and hand it off to the user-supplied
    // executor. The executor decides the thread (synchronous or async);
    // whichever side calls the thunk, GIL reacquire happens inside drain_one.
    //
    // Note: we schedule drain-of-queue per executor-dispatch rather than
    // drain-per-action (which is what vanilla JVM does). All queued actions
    // run on whichever executor triggered the current drain. Users who mix
    // `send` and `send-via` on the same agent will observe all actions
    // served by one of the two — typically not a problem in practice.
    if matches!(exec, Executor::Custom) {
        if let Some(exec_obj) = custom {
            match build_drain_thunk(py, agent) {
                Ok((thunk, _unused_agent)) => {
                    let _ = exec_obj.bind(py).call1((thunk,));
                    return;
                }
                Err(a) => {
                    // Closure build failed (extreme — OOM). Fall through to
                    // the default pool so the action still runs.
                    let job: Job = Box::new(move || {
                        Python::attach(|py| drain_one(py, a));
                    });
                    send_pool().execute(job);
                    return;
                }
            }
        }
        // No custom executor provided → default pool.
    }

    let job: Job = Box::new(move || {
        Python::attach(|py| drain_one(py, agent));
    });
    match exec {
        Executor::Send => send_pool().execute(job),
        Executor::SendOff => send_off_pool().execute(job),
        Executor::Custom => send_pool().execute(job),
    }
}

/// Wrap `drain_one(agent)` in a zero-arg Python callable. On build failure,
/// returns the Py<Agent> back to the caller so it can choose a fallback.
fn build_drain_thunk(
    py: Python<'_>,
    agent: Py<Agent>,
) -> Result<(PyObject, ()), Py<Agent>> {
    // PyCFunction::new_closure requires Fn + Send + Sync + 'static. We need
    // to clone-ref the agent inside the closure (each call), so capture it
    // in a cell we can reborrow safely. parking_lot::Mutex works.
    let agent_holder = Arc::new(parking_lot::Mutex::new(Some(agent)));
    let agent_for_closure = agent_holder.clone();
    let closure_result = pyo3::types::PyCFunction::new_closure(
        py,
        None,
        None,
        move |args: &Bound<'_, PyTuple>, _kw: Option<&Bound<'_, PyDict>>| -> PyResult<PyObject> {
            let py = args.py();
            // Take() so the thunk can only fire once; subsequent invocations
            // (if the user's executor misbehaves and calls it twice) are a
            // no-op.
            let maybe_agent = agent_for_closure.lock().take();
            if let Some(a) = maybe_agent {
                drain_one(py, a);
            }
            Ok(py.None())
        },
    );
    match closure_result {
        Ok(thunk) => Ok((thunk.unbind().into_any(), ())),
        Err(_) => {
            // Closure build failed — reclaim the agent.
            let reclaimed = agent_holder
                .lock()
                .take()
                .expect("agent always present if closure wasn't called");
            Err(reclaimed)
        }
    }
}

fn drain_one(py: Python<'_>, agent: Py<Agent>) {
    loop {
        let action = {
            let this = agent.bind(py).get();
            let mut q = this.queue.lock();
            match q.pop_front() {
                Some(a) => a,
                None => {
                    this.busy.store(false, Ordering::Release);
                    return;
                }
            }
        };
        execute(py, &agent, action);
        // If the agent failed (:fail mode), stop draining.
        {
            let this = agent.bind(py).get();
            let g = this.error.load();
            let opt: &Option<PyObject> = &g;
            if opt.is_some() {
                this.busy.store(false, Ordering::Release);
                return;
            }
        }
    }
}

fn execute(py: Python<'_>, agent: &Py<Agent>, action: Action) {
    // Install the convey snapshot as the current top frame, then push a
    // frame over it that binds *agent* to this agent. The two-step push
    // mirrors JVM's binding-conveyor-fn clone + *agent* binding.
    BINDING_STACK.with(|s| s.borrow_mut().push(action.binding_snapshot));

    let agent_star = lookup_agent_star_var(py);
    if let Some(var) = &agent_star {
        let agent_py: PyObject = agent.clone_ref(py).into_any();
        let top = BINDING_STACK
            .with(|s| s.borrow().last().map(|f| f.clone_ref(py)))
            .unwrap_or_else(|| empty_frame(py).unwrap());
        let var_py: PyObject = var.clone_ref(py).into_any();
        let frame = match frame_assoc(&top, py, var_py, agent_py) {
            Ok(f) => f,
            Err(_) => return,
        };
        BINDING_STACK.with(|s| s.borrow_mut().push(frame));
    }

    let result: PyResult<PyObject> = (|| {
        let old_state = {
            let g = agent.bind(py).get().state.load();
            let v: &PyObject = &g;
            v.clone_ref(py)
        };
        let mut call_args: Vec<PyObject> = Vec::with_capacity(action.args.len() + 1);
        call_args.push(old_state.clone_ref(py));
        call_args.extend(action.args);
        let new_val = crate::rt::invoke_n(py, action.f, &call_args)?;
        agent.bind(py).get().validate(py, &new_val)?;
        agent.bind(py).get().state.store(Arc::new(new_val.clone_ref(py)));
        agent.bind(py).get().fire_watches(py, agent, old_state, new_val.clone_ref(py))?;
        Ok(new_val)
    })();

    // Pop *agent* binding and convey snapshot.
    if agent_star.is_some() {
        BINDING_STACK.with(|s| { s.borrow_mut().pop(); });
    }
    BINDING_STACK.with(|s| { s.borrow_mut().pop(); });

    // Install any error BEFORE decrementing pending so `await` observers see
    // a consistent state (agent-error populated once the action is "done").
    if let Err(err) = result {
        handle_error(py, agent, err);
    }

    // Decrement pending + notify so `await`/`await-for` can unblock.
    {
        let this = agent.bind(py).get();
        let mut p = this.pending.lock();
        if *p > 0 { *p -= 1; }
        if *p == 0 { this.pending_cv.notify_all(); }
    }
}

fn handle_error(py: Python<'_>, agent: &Py<Agent>, err: PyErr) {
    let this = agent.bind(py).get();
    let mode: PyObject = {
        let g = this.error_mode.load();
        let v: &PyObject = &g;
        v.clone_ref(py)
    };
    let handler_opt: Option<PyObject> = {
        let g = this.error_handler.load();
        let opt: &Option<PyObject> = &g;
        opt.as_ref().map(|o| o.clone_ref(py))
    };
    let err_obj: PyObject = err.clone_ref(py).into_value(py).into_any();

    // :continue — call handler (if any), don't park error.
    let is_continue = {
        if let Ok(kw) = mode.bind(py).cast::<Keyword>() {
            kw.get().name.as_ref() == "continue"
        } else {
            false
        }
    };
    if is_continue {
        if let Some(h) = handler_opt {
            let _ = h.bind(py).call1((agent.clone_ref(py), err_obj));
        }
        return;
    }

    // :fail — park the error; optionally also notify handler.
    this.error.store(Arc::new(Some(err_obj.clone_ref(py))));
    if let Some(h) = handler_opt {
        let _ = h.bind(py).call1((agent.clone_ref(py), err_obj));
    }
}

// --- await / await-for ---

/// Wait until each agent's pending-count reaches 0.
pub fn agent_await(py: Python<'_>, agents: Vec<Py<Agent>>) -> PyResult<()> {
    for agent in agents {
        let this = agent.bind(py).get();
        // Drop the GIL while blocking so workers can run.
        py.detach(|| {
            let mut p = this.pending.lock();
            while *p > 0 {
                this.pending_cv.wait(&mut p);
            }
        });
    }
    Ok(())
}

pub fn agent_await_for(
    py: Python<'_>,
    timeout_ms: u64,
    agents: Vec<Py<Agent>>,
) -> PyResult<bool> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    for agent in agents {
        let this = agent.bind(py).get();
        let ok = py.detach(|| {
            let mut p = this.pending.lock();
            while *p > 0 {
                let now = std::time::Instant::now();
                if now >= deadline {
                    return false;
                }
                let remaining = deadline - now;
                let result = this.pending_cv.wait_for(&mut p, remaining);
                if result.timed_out() && *p > 0 {
                    return false;
                }
            }
            true
        });
        if !ok {
            return Ok(false);
        }
    }
    Ok(true)
}

// --- restart-agent / clear-agent-errors / shutdown-agents ---

pub fn restart_agent(
    py: Python<'_>,
    agent: Py<Agent>,
    new_state: PyObject,
    clear_actions: bool,
) -> PyResult<PyObject> {
    let this = agent.bind(py).get();
    // Must be in failed state.
    let was_failed = {
        let g = this.error.load();
        let opt: &Option<PyObject> = &g;
        opt.is_some()
    };
    if !was_failed {
        return Err(IllegalStateException::new_err(
            "Agent does not need a restart",
        ));
    }
    this.validate(py, &new_state)?;
    this.state.store(Arc::new(new_state.clone_ref(py)));
    this.error.store(Arc::new(None));
    if clear_actions {
        let mut q = this.queue.lock();
        let dropped = q.len() as u64;
        q.clear();
        let mut p = this.pending.lock();
        if dropped >= *p {
            *p = 0;
        } else {
            *p -= dropped;
        }
        this.pending_cv.notify_all();
    } else {
        // Resume draining if there are queued actions.
        let has_work = !this.queue.lock().is_empty();
        if has_work && !this.busy.swap(true, Ordering::AcqRel) {
            schedule_drain(Executor::Send, None, py, agent.clone_ref(py));
        }
    }
    Ok(new_state)
}

pub fn clear_agent_errors(py: Python<'_>, agent: Py<Agent>) -> PyResult<PyObject> {
    // Legacy alias — restart without state change, without clearing actions.
    let current = {
        let g = agent.bind(py).get().state.load();
        let v: &PyObject = &g;
        v.clone_ref(py)
    };
    restart_agent(py, agent, current, false)
}

pub fn shutdown_agents() {
    if let Some(p) = SEND_POOL.get() {
        p.shutdown();
    }
    if let Some(p) = SEND_OFF_POOL.get() {
        p.shutdown();
    }
}

// --- Python constructor ---

#[pyfunction]
#[pyo3(name = "agent")]
pub fn py_agent(py: Python<'_>, initial: PyObject) -> PyResult<Agent> {
    Agent::new(py, initial)
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Agent>()?;
    m.add_function(wrap_pyfunction!(py_agent, m)?)?;
    Ok(())
}
