//! `LockingTransaction` — MVCC transaction state for `sync` / `dosync`.
//!
//! Lives per-OS-thread via `thread_local!`. `Rc` (not `Arc`) because the txn
//! never leaves its owning thread. `ref-set` / `alter` / `commute` / `ensure`
//! look up the current txn through `current()` and mutate it via `RefCell`s.
//!
//! Conflict detection is MVCC-style: every commit advances a global clock
//! and stamps each touched ref's `last_commit`. At commit time, before
//! installing any writes, we check each write-set ref's `last_commit` against
//! our `read_point`; if the ref was committed by another txn after we started
//! reading, we raise `RetryEx` and the outer loop restarts.
//!
//! Barge (the full JVM priority-inversion break) is not implemented. Lock
//! acquisition is in sorted `Ref.id` order so deadlock is impossible, and
//! `MAX_RETRIES = 10_000` bounds any livelock.
//!
//! `io!` detection: `assert_no_txn` checks `current().is_some()`. Exposed as
//! `clojure.lang.RT/io-bang-check` and called at the top of the `io!` macro.

use crate::exceptions::{IllegalStateException, RetryEx};
use crate::stm::ref_::{Ref, TVal};
use parking_lot::lock_api::RawRwLock as _;
use pyo3::prelude::*;
use pyo3::types::PyAny;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

type PyObject = Py<PyAny>;

pub const MAX_RETRIES: u32 = 10_000;

/// Monotonic clock driving MVCC points. Incremented once at txn start (for
/// the `read_point`) and once at commit (for the `write_point`). Starts at 1
/// so `point = 0` TVals (the initial value installed by `Ref::new`) are
/// visible to every transaction.
pub(crate) static COMMIT_CLOCK: AtomicU64 = AtomicU64::new(1);

pub(crate) struct Commute {
    pub f: PyObject,
    pub args: Vec<PyObject>,
}

/// A pending agent send captured inside a transaction — dispatched only if
/// the transaction commits successfully. `executor_kind` is an opaque tag
/// that the agent module's `dispatch_from_commit` interprets.
pub struct PendingSend {
    pub agent: PyObject,
    pub f: PyObject,
    pub args: Vec<PyObject>,
    pub executor_kind: u8,
    pub custom_executor: Option<PyObject>,
    pub binding_snapshot: crate::binding::Frame,
}

/// In-flight MVCC transaction. Not Send — always lives on its owning
/// OS thread via `CURRENT_TXN`.
pub struct LockingTransaction {
    /// MVCC read point — transactions see TVals with `point <= read_point`.
    pub read_point: u64,

    /// Map from `Ref.id` to the `Py<Ref>`. Every ref touched by the txn
    /// (sets, commutes, ensures) is registered here so commit can iterate.
    refs: RefCell<HashMap<u64, Py<Ref>>>,

    /// In-txn values set by `ref-set` / `alter`. Indexed by `Ref.id`.
    vals: RefCell<HashMap<u64, PyObject>>,

    /// Ref ids in the write-set — set by `ref-set` / `alter`.
    sets: RefCell<HashSet<u64>>,

    /// Queued commutes per ref. Each entry is applied at commit time against
    /// the current in-txn value (seeded from the latest committed head under
    /// the write lock). Order preserved via `Vec`.
    commutes: RefCell<BTreeMap<u64, Vec<Commute>>>,

    /// Ref ids held as read-locks via `ensure`. Read locks are acquired at
    /// `ensure` call-time and released at commit (or retry).
    ensures: RefCell<HashSet<u64>>,

    /// Pending agent sends — flushed only on successful commit.
    pub(crate) agent_sends: RefCell<Vec<PendingSend>>,

    /// Retry counter (bumped by the outer run-in-transaction loop).
    pub retry_count: Cell<u32>,
}

impl LockingTransaction {
    fn fresh() -> Self {
        let read_point = COMMIT_CLOCK.fetch_add(1, Ordering::AcqRel);
        Self {
            read_point,
            refs: RefCell::new(HashMap::new()),
            vals: RefCell::new(HashMap::new()),
            sets: RefCell::new(HashSet::new()),
            commutes: RefCell::new(BTreeMap::new()),
            ensures: RefCell::new(HashSet::new()),
            agent_sends: RefCell::new(Vec::new()),
            retry_count: Cell::new(0),
        }
    }

    /// Register (if absent) a touched ref so commit can find it later.
    fn register(&self, r: &Py<Ref>, py: Python<'_>) {
        let id = r.bind(py).get().id;
        let mut refs = self.refs.borrow_mut();
        refs.entry(id).or_insert_with(|| r.clone_ref(py));
    }

    /// MVCC read — called by `Ref::IDeref::deref` when a transaction is
    /// active. Returns the latest in-txn value if the ref was written this
    /// txn, else walks the history for the newest TVal with
    /// `point <= read_point`. Missing (all history pruned past our point)
    /// bumps the ref's fault counter and raises `RetryEx`.
    pub fn do_get(&self, py: Python<'_>, r: &Py<Ref>) -> PyResult<PyObject> {
        self.register(r, py);
        let this_ref = r.bind(py).get();
        let id = this_ref.id;

        // In-txn write wins.
        if let Some(v) = self.vals.borrow().get(&id) {
            return Ok(v.clone_ref(py));
        }

        // Walk history.
        let hist = this_ref.history.lock();
        if let Some(tv) = hist.find_at(self.read_point) {
            return Ok(tv.val.clone_ref(py));
        }
        drop(hist);

        // Ref has been pruned past our point — bump faults so the ref grows
        // its history on future commits, then retry.
        this_ref.faults.fetch_add(1, Ordering::Relaxed);
        Err(RetryEx::new_err("ref history too short — retry"))
    }

    /// `(ref-set r v)` — install `v` as the in-txn value of `r`.
    pub fn do_set(&self, py: Python<'_>, r: &Py<Ref>, v: PyObject) -> PyResult<PyObject> {
        self.register(r, py);
        let id = r.bind(py).get().id;
        if self.commutes.borrow().contains_key(&id) {
            return Err(IllegalStateException::new_err(
                "Can't set after commute",
            ));
        }
        self.sets.borrow_mut().insert(id);
        self.vals.borrow_mut().insert(id, v.clone_ref(py));
        Ok(v)
    }

    /// `(alter r f & args)` — read in-txn value, apply fn, set result.
    pub fn do_alter(
        &self,
        py: Python<'_>,
        r: &Py<Ref>,
        f: PyObject,
        args: Vec<PyObject>,
    ) -> PyResult<PyObject> {
        let current = self.do_get(py, r)?;
        let mut call_args: Vec<PyObject> = Vec::with_capacity(args.len() + 1);
        call_args.push(current);
        call_args.extend(args);
        let new_val = crate::rt::invoke_n(py, f, &call_args)?;
        self.do_set(py, r, new_val)
    }

    /// `(commute r f & args)` — queue `(f args)` and return a provisional
    /// value. Real value is recomputed against the latest committed state
    /// at commit time.
    pub fn do_commute(
        &self,
        py: Python<'_>,
        r: &Py<Ref>,
        f: PyObject,
        args: Vec<PyObject>,
    ) -> PyResult<PyObject> {
        self.register(r, py);
        let id = r.bind(py).get().id;

        // Compute a provisional value from the current in-txn value (for
        // use during the rest of the txn body); commit will recompute from
        // the latest committed head.
        let current = self.do_get(py, r)?;
        let mut call_args: Vec<PyObject> = Vec::with_capacity(args.len() + 1);
        call_args.push(current);
        for a in &args {
            call_args.push(a.clone_ref(py));
        }
        let provisional = crate::rt::invoke_n(py, f.clone_ref(py), &call_args)?;

        // Queue the commute for commit-time recomputation.
        self.commutes
            .borrow_mut()
            .entry(id)
            .or_default()
            .push(Commute { f, args });
        // Store the provisional so subsequent reads in this txn see it.
        self.vals.borrow_mut().insert(id, provisional.clone_ref(py));
        Ok(provisional)
    }

    /// `(ensure r)` — acquire a read lock on `r`, held until commit.
    /// Blocks other txns from committing writes to `r` until we commit.
    /// Returns the in-txn value of `r`.
    pub fn do_ensure(&self, py: Python<'_>, r: &Py<Ref>) -> PyResult<PyObject> {
        self.register(r, py);
        let id = r.bind(py).get().id;
        if self.ensures.borrow().contains(&id) {
            return self.do_get(py, r);
        }
        // Conflict shortcut: if another txn committed after our read_point,
        // retry now rather than holding a stale read lock.
        let last = r.bind(py).get().last_commit.load(Ordering::Acquire);
        if last > self.read_point {
            return Err(RetryEx::new_err("ensure saw post-read-point commit"));
        }
        // Acquire the read lock. Raw API so we can release by id later.
        unsafe { r.bind(py).get().rw.raw().lock_shared(); }
        // Re-check after acquiring.
        let last2 = r.bind(py).get().last_commit.load(Ordering::Acquire);
        if last2 > self.read_point {
            // Unlock and retry.
            unsafe { r.bind(py).get().rw.raw().unlock_shared(); }
            return Err(RetryEx::new_err("ensure saw post-read-point commit"));
        }
        self.ensures.borrow_mut().insert(id);
        self.do_get(py, r)
    }

    /// Commit the transaction. All lock acquisition / release / history
    /// installation / watch firing lives here.
    pub fn commit(&self, py: Python<'_>) -> PyResult<()> {
        // Collect write-ref ids (sets + commutes) in sorted order.
        let mut write_ids: Vec<u64> = {
            let sets = self.sets.borrow();
            let commutes = self.commutes.borrow();
            let mut v: Vec<u64> = sets.iter().copied().collect();
            for id in commutes.keys() {
                if !sets.contains(id) {
                    v.push(*id);
                }
            }
            v.sort();
            v
        };
        // Commute ids, sorted, for the recompute pass below.
        let commute_ids: Vec<u64> = {
            let commutes = self.commutes.borrow();
            let mut v: Vec<u64> = commutes.keys().copied().collect();
            v.sort();
            v
        };

        // If a write ref is also ensured, release its read lock first so we
        // can take the write lock below. We'll re-validate via last_commit
        // check after taking the write lock.
        {
            let refs = self.refs.borrow();
            let mut ensures = self.ensures.borrow_mut();
            for id in &write_ids {
                if ensures.remove(id) {
                    let r = refs.get(id).expect("registered");
                    unsafe { r.bind(py).get().rw.raw().unlock_shared(); }
                }
            }
        }

        // Acquire write locks in sorted id order; track so we can release.
        let mut locked_write: Vec<u64> = Vec::new();
        let release_writes = |ids: &[u64], refs: &HashMap<u64, Py<Ref>>, py: Python<'_>| {
            for id in ids.iter().rev() {
                let r = refs.get(id).expect("registered");
                unsafe { r.bind(py).get().rw.raw().unlock_exclusive(); }
            }
        };
        let release_ensures = |ensures: &HashSet<u64>, refs: &HashMap<u64, Py<Ref>>, py: Python<'_>| {
            for id in ensures {
                let r = refs.get(id).expect("registered");
                unsafe { r.bind(py).get().rw.raw().unlock_shared(); }
            }
        };

        // Closure-free version — we hold locks via raw API and drop manually.
        for id in &write_ids {
            let refs = self.refs.borrow();
            let r = refs.get(id).expect("registered");
            unsafe { r.bind(py).get().rw.raw().lock_exclusive(); }
            locked_write.push(*id);
            // Conflict check: if this ref was committed after our read_point,
            // another txn beat us to it. Abort + retry.
            let last = r.bind(py).get().last_commit.load(Ordering::Acquire);
            if last > self.read_point {
                // Cleanup all held locks and raise RetryEx.
                release_writes(&locked_write, &refs, py);
                release_ensures(&self.ensures.borrow(), &refs, py);
                return Err(RetryEx::new_err("write conflict — retry"));
            }
        }

        // At this point: all write locks held; pure-ensure read locks held.
        // Recompute commutes against the current committed head.
        {
            let refs = self.refs.borrow();
            let mut vals = self.vals.borrow_mut();
            let commutes = self.commutes.borrow();
            for id in &commute_ids {
                let r = refs.get(id).expect("registered");
                let head_val = {
                    let h = r.bind(py).get().history.lock();
                    h.head()
                        .map(|tv| tv.val.clone_ref(py))
                        .ok_or_else(|| IllegalStateException::new_err("Ref is unbound"))?
                };
                let mut cur = head_val;
                for c in commutes.get(id).into_iter().flatten() {
                    let mut call_args: Vec<PyObject> = Vec::with_capacity(c.args.len() + 1);
                    call_args.push(cur);
                    for a in &c.args {
                        call_args.push(a.clone_ref(py));
                    }
                    cur = crate::rt::invoke_n(py, c.f.clone_ref(py), &call_args).map_err(|e| {
                        // Ensure we release locks even on error before bubbling.
                        release_writes(&locked_write, &refs, py);
                        release_ensures(&self.ensures.borrow(), &refs, py);
                        e
                    })?;
                }
                vals.insert(*id, cur);
            }
        }

        // Validate every proposed new value.
        {
            let refs = self.refs.borrow();
            let vals = self.vals.borrow();
            for id in &write_ids {
                let r = refs.get(id).expect("registered");
                let v = vals.get(id).expect("write ref missing vals entry");
                if let Err(e) = r.bind(py).get().validate(py, v) {
                    release_writes(&locked_write, &refs, py);
                    release_ensures(&self.ensures.borrow(), &refs, py);
                    return Err(e);
                }
            }
        }

        // Allocate a fresh commit point and install each new TVal.
        let write_point = COMMIT_CLOCK.fetch_add(1, Ordering::AcqRel);
        let mut watch_snapshots: Vec<(Py<Ref>, PyObject, PyObject)> = Vec::new();
        {
            let refs = self.refs.borrow();
            let vals = self.vals.borrow();
            for id in &write_ids {
                let r = refs.get(id).expect("registered");
                let new_val = vals.get(id).expect("vals entry").clone_ref(py);
                let this = r.bind(py).get();
                let faults = this.faults.load(Ordering::Relaxed) as usize;
                let min_h = this.min_history.load(Ordering::Relaxed);
                let max_h = this.max_history.load(Ordering::Relaxed);
                let mut h = this.history.lock();
                let old_val = h
                    .head()
                    .map(|tv| tv.val.clone_ref(py))
                    .unwrap_or_else(|| py.None());
                let effective_min = min_h.saturating_add(faults);
                h.push(
                    TVal { point: write_point, val: new_val.clone_ref(py) },
                    effective_min,
                    max_h,
                );
                this.last_commit.store(write_point, Ordering::Release);
                drop(h);
                // Reset faults — history just grew.
                this.faults.store(0, Ordering::Relaxed);
                watch_snapshots.push((r.clone_ref(py), old_val, new_val));
            }
        }

        // Release all locks now — watches and pending sends run outside.
        {
            let refs = self.refs.borrow();
            release_writes(&locked_write, &refs, py);
            release_ensures(&self.ensures.borrow(), &refs, py);
        }
        self.ensures.borrow_mut().clear();

        // Fire watches outside any lock.
        for (r, old, new) in watch_snapshots {
            let this = r.bind(py).get();
            this.fire_watches(py, &r, old, new)?;
        }

        // Drain any pending agent sends captured in this txn. The agent
        // module hooks `dispatch_from_commit` in when it's loaded; if the
        // agent module has not been built yet, this is a no-op because
        // `agent_sends` is only populated via `agent::dispatch`.
        let sends: Vec<PendingSend> =
            std::mem::take(&mut *self.agent_sends.borrow_mut());
        for s in sends {
            crate::agent::dispatch_from_commit(py, s)?;
        }

        Ok(())
    }

    /// Release all held locks. Called on error paths other than the main
    /// commit flow (which cleans up inline).
    pub fn release_all_locks(&self, py: Python<'_>) {
        let refs = self.refs.borrow();
        let mut ensures = self.ensures.borrow_mut();
        for id in ensures.drain() {
            if let Some(r) = refs.get(&id) {
                unsafe { r.bind(py).get().rw.raw().unlock_shared(); }
            }
        }
    }
}

thread_local! {
    pub(crate) static CURRENT_TXN: RefCell<Option<Rc<LockingTransaction>>> = const { RefCell::new(None) };
}

/// Return the current thread's in-flight transaction, if any.
pub fn current() -> Option<Rc<LockingTransaction>> {
    CURRENT_TXN.with(|c| c.borrow().clone())
}

/// `io!` check — raise if a transaction is currently running on this thread.
pub fn assert_no_txn(msg: &str) -> PyResult<()> {
    if current().is_some() {
        return Err(IllegalStateException::new_err(format!(
            "I/O in transaction: {msg}"
        )));
    }
    Ok(())
}

fn require_txn() -> PyResult<Rc<LockingTransaction>> {
    current()
        .ok_or_else(|| IllegalStateException::new_err("No transaction running"))
}

/// Entry point for `(sync flags & body)` / `(dosync & body)`. `body` is a
/// no-argument callable; we invoke it, commit on success, and retry on
/// `RetryEx` up to `MAX_RETRIES`.
pub fn run_in_transaction(py: Python<'_>, body: PyObject) -> PyResult<PyObject> {
    // Nested sync: reuse the current txn, matching vanilla.
    if current().is_some() {
        return crate::rt::invoke_n(py, body, &[]);
    }
    for attempt in 0..MAX_RETRIES {
        let txn = Rc::new(LockingTransaction::fresh());
        txn.retry_count.set(attempt);
        CURRENT_TXN.with(|c| c.replace(Some(txn.clone())));

        // Invoke body and (if Ok) commit. On any error we ensure the txn
        // held locks are released before deciding retry-vs-propagate.
        let body_clone = body.clone_ref(py);
        let result: PyResult<PyObject> = (|| -> PyResult<PyObject> {
            let v = crate::rt::invoke_n(py, body_clone, &[])?;
            txn.commit(py)?;
            Ok(v)
        })();
        // Release any locks still held (ensures on a pre-commit error path).
        txn.release_all_locks(py);
        CURRENT_TXN.with(|c| c.replace(None));

        match result {
            Ok(v) => return Ok(v),
            Err(e) if e.is_instance_of::<RetryEx>(py) => continue,
            Err(e) => return Err(e),
        }
    }
    Err(IllegalStateException::new_err(format!(
        "Transaction failed after reaching retry limit ({MAX_RETRIES})"
    )))
}

// --- Public wrappers called from the RT shims. ---

pub fn ref_set(py: Python<'_>, r: Py<Ref>, v: PyObject) -> PyResult<PyObject> {
    require_txn()?.do_set(py, &r, v)
}

pub fn alter(py: Python<'_>, r: Py<Ref>, f: PyObject, args: Vec<PyObject>) -> PyResult<PyObject> {
    require_txn()?.do_alter(py, &r, f, args)
}

pub fn commute(py: Python<'_>, r: Py<Ref>, f: PyObject, args: Vec<PyObject>) -> PyResult<PyObject> {
    require_txn()?.do_commute(py, &r, f, args)
}

pub fn ensure(py: Python<'_>, r: Py<Ref>) -> PyResult<PyObject> {
    require_txn()?.do_ensure(py, &r)
}
