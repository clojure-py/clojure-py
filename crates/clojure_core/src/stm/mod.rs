//! Software Transactional Memory — Clojure's `ref` and the `sync`/`dosync`
//! transaction machinery. Mirrors `clojure.lang.Ref` + `clojure.lang.LockingTransaction`.
//!
//! Layout:
//! - `ref_`  — the `Ref` pyclass and its history ring.
//! - `txn`   — `LockingTransaction`, the thread-local current-txn slot, and
//!             the module-level `ref-set` / `alter` / `commute` / `ensure`
//!             helpers that require a running transaction.
//!
//! Design note — `IRef` on the JVM is a super-interface for Atom/Ref/Agent/Var
//! giving them polymorphic `addWatch` / `setValidator` etc. This port does not
//! introduce an `IRef` protocol: instead, `Ref` (like `Atom`, `Var`,
//! `Namespace`) exposes matching `#[pymethods]` and the `add-watch` /
//! `set-validator!` forms in `core.clj` call them by method name. If a future
//! caller needs polymorphic "give me any IRef", we add the protocol then.

pub mod ref_;
pub mod txn;

use pyo3::prelude::*;

pub(crate) fn register(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    ref_::register(py, m)?;
    Ok(())
}
