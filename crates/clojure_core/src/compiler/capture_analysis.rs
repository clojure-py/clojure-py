//! Static analysis over read forms: how many times an outer-scope local is
//! referenced, and whether any inner `fn*` closes over a given local. The
//! emitter uses these counts to drive locals-clearing liveness (see
//! `Compiler::emit_load_local`).

use crate::collections::parraymap::PersistentArrayMap;
use crate::collections::phashmap::PersistentHashMap;
use crate::collections::phashset::PersistentHashSet;
use crate::collections::plist::{EmptyList, PersistentList};
use crate::collections::pvector::PersistentVector;
use crate::compiler::emit::{collect_seq, is_non_list_seq, list_items, parse_params};
use crate::symbol::Symbol;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBool, PyFloat, PyInt, PyString};
use std::sync::Arc;

type PyObject = Py<PyAny>;

/// Count how many OUTER-scope reads a single form will produce for a given
/// unqualified name. Recurses into subforms. `fn*` forms count as 1 per
/// captured name (that's one `LoadLocal` at `MakeFn` time, regardless of
/// how often the inner body uses the local). Shadowing inside let/loop/fn
/// stops the recursion for that subtree.
pub fn count_outer_refs_in_form(
    py: Python<'_>,
    form: &PyObject,
    name: &str,
) -> PyResult<usize> {
    let b = form.bind(py);

    // Symbol: direct match is 1, miss is 0.
    if let Ok(sym_ref) = b.cast::<Symbol>() {
        let s = sym_ref.get();
        if s.ns.is_none() && s.name.as_ref() == name {
            return Ok(1);
        }
        return Ok(0);
    }

    // Atoms.
    if form.is_none(py)
        || b.cast::<PyBool>().is_ok()
        || b.cast::<PyInt>().is_ok()
        || b.cast::<PyFloat>().is_ok()
        || b.cast::<PyString>().is_ok()
        || b.cast::<crate::keyword::Keyword>().is_ok()
    {
        return Ok(0);
    }

    // Collections (vector / map / set) — count inner references.
    if let Ok(pv) = b.cast::<PersistentVector>() {
        let pv_ref = pv.get();
        let mut total: usize = 0;
        for i in 0..(pv_ref.cnt as usize) {
            let el = pv_ref.nth_internal_pub(py, i)?;
            total = total.saturating_add(count_outer_refs_in_form(py, &el, name)?);
        }
        return Ok(total);
    }
    if b.cast::<PersistentHashMap>().is_ok() || b.cast::<PersistentArrayMap>().is_ok() {
        let mut total: usize = 0;
        for item in b.try_iter()? {
            let k = item?.unbind();
            let v = b.call_method1("val_at", (k.clone_ref(py),))?.unbind();
            total = total.saturating_add(count_outer_refs_in_form(py, &k, name)?);
            total = total.saturating_add(count_outer_refs_in_form(py, &v, name)?);
        }
        return Ok(total);
    }
    if b.cast::<PersistentHashSet>().is_ok() {
        let mut total: usize = 0;
        for item in b.try_iter()? {
            total = total.saturating_add(count_outer_refs_in_form(py, &item?.unbind(), name)?);
        }
        return Ok(total);
    }

    // Lists — check for special scoping forms.
    let items = if let Ok(_pl) = b.cast::<PersistentList>() {
        list_items(py, form)?
    } else if b.cast::<EmptyList>().is_ok() {
        return Ok(0);
    } else if is_non_list_seq(py, form)? {
        collect_seq(py, form)?
    } else {
        return Ok(0);
    };
    if items.is_empty() { return Ok(0); }

    // Head-symbol special cases for shadowing.
    let head = items[0].clone_ref(py);
    let head_b = head.bind(py);
    if let Ok(sym_ref) = head_b.cast::<Symbol>() {
        let s = sym_ref.get();
        if s.ns.is_none() {
            match s.name.as_ref() {
                "quote" => return Ok(0),  // everything inside is data
                "fn*" => {
                    return count_fn_captures(py, &items[1..], name);
                }
                "let*" | "loop*" => {
                    return count_let_refs(py, &items[1..], name);
                }
                "letfn*" => {
                    return count_letfn_refs(py, &items[1..], name);
                }
                _ => {}
            }
        }
        // If the head resolves to a macro Var, macroexpansion will produce
        // forms this pre-scan can't see (e.g. `do-template` emits N copies
        // of the template expression). Undercounting here would cause the
        // compiler's liveness tracker to clear the local too early, so the
        // second macro-generated reference would see nil. Return
        // `usize::MAX` as a "don't use this count" sentinel.
        // Note: qualified heads (ns/name) also need this check — syntax-
        // quote emits fully-qualified forms.
        if head_looks_like_macro(py, s)? {
            return Ok(usize::MAX);
        }
    }

    // Default: count refs in every item (including the head, which may be
    // the symbol we're looking for in the fn-call position).
    let mut total: usize = 0;
    for item in &items {
        let c = count_outer_refs_in_form(py, item, name)?;
        // Saturate at usize::MAX so macro sentinels propagate upward.
        total = total.saturating_add(c);
    }
    Ok(total)
}

fn head_looks_like_macro(py: Python<'_>, s: &Symbol) -> PyResult<bool> {
    let name = s.name.as_ref();
    // Fast-reject ambient names that are known not to be macros. Keeps us
    // from walking a namespace for every `(+ a b)` call.
    if matches!(
        name,
        "+" | "-" | "*" | "/" | "=" | "<" | ">" | "<=" | ">="
            | "inc" | "dec" | "not" | "first" | "rest" | "next" | "cons"
            | "list" | "vector" | "hash-map" | "hash-set"
            | "nth" | "get" | "count" | "conj" | "assoc" | "dissoc"
            | "apply" | "map" | "filter" | "reduce" | "into"
            | "identity" | "seq" | "empty?"
    ) {
        return Ok(false);
    }
    let sys = py.import("sys")?;
    let modules = sys.getattr("modules")?;
    // Resolve to a Var. Search order:
    //   1. ns-qualified → that ns.
    //   2. unqualified → the CURRENT compile ns (to catch refers like
    //      `are`/`do-template` that live in clojure.test or elsewhere).
    //   3. unqualified → clojure.core as a fallback for eval-from-Python
    //      paths where the compile ns is clojure.user or fresh.
    let current_ns_opt: Option<PyObject> =
        crate::eval::load::CURRENT_LOAD_NS.with(|c| c.borrow().as_ref().map(|n| n.clone_ref(py)));
    let resolved: Option<Py<crate::var::Var>> = if let Some(ns_name) = s.ns.as_deref() {
        modules
            .get_item(ns_name)
            .ok()
            .and_then(|m| m.getattr(name).ok())
            .and_then(|v| v.cast::<crate::var::Var>().ok().map(|v| v.clone().unbind()))
    } else {
        let from_current = current_ns_opt
            .as_ref()
            .and_then(|ns| ns.bind(py).getattr(name).ok())
            .and_then(|v| v.cast::<crate::var::Var>().ok().map(|v| v.clone().unbind()));
        from_current.or_else(|| {
            modules
                .get_item("clojure.core")
                .ok()
                .and_then(|m| m.getattr(name).ok())
                .and_then(|v| v.cast::<crate::var::Var>().ok().map(|v| v.clone().unbind()))
        })
    };
    match resolved {
        Some(v) => Ok(v.bind(py).get().is_macro(py)),
        None => Ok(false),
    }
}

/// Sum `count_outer_refs_in_form` over a sequence of body forms.
pub fn count_outer_refs_in_forms(
    py: Python<'_>,
    forms: &[PyObject],
    name: &str,
) -> PyResult<usize> {
    let mut total: usize = 0;
    for f in forms {
        total = total.saturating_add(count_outer_refs_in_form(py, f, name)?);
    }
    Ok(total)
}

/// For a `(fn ...)` — return 1 if `name` is captured by any of its arity
/// bodies (used and not shadowed by that arity's params), else 0.
fn count_fn_captures(
    py: Python<'_>,
    after_fn_head: &[PyObject],
    name: &str,
) -> PyResult<usize> {
    if after_fn_head.is_empty() { return Ok(0); }
    // Skip an optional name symbol.
    let rest: &[PyObject] = if let Ok(_) = after_fn_head[0].bind(py).cast::<Symbol>() {
        &after_fn_head[1..]
    } else {
        after_fn_head
    };
    if rest.is_empty() { return Ok(0); }

    let specs: Vec<(PyObject, Vec<PyObject>)> = {
        let first_b = rest[0].bind(py);
        if first_b.cast::<PersistentVector>().is_ok() {
            vec![(rest[0].clone_ref(py), rest[1..].iter().map(|o| o.clone_ref(py)).collect())]
        } else if first_b.cast::<PersistentList>().is_ok() {
            let mut specs = Vec::new();
            for item in rest {
                let items = list_items(py, item)?;
                if items.is_empty() { continue; }
                let params = items[0].clone_ref(py);
                let body: Vec<PyObject> = items[1..].iter().map(|o| o.clone_ref(py)).collect();
                specs.push((params, body));
            }
            specs
        } else {
            return Ok(0);
        }
    };

    for (params, body) in &specs {
        let (param_names, _) = match parse_params(py, params) {
            Ok(x) => x,
            Err(_) => continue,
        };
        if param_names.iter().any(|n| n.as_ref() == name) {
            continue;  // shadowed in this arity — doesn't capture
        }
        // If the body references `name`, this arity captures it.
        for f in body {
            if count_outer_refs_in_form(py, f, name)? > 0 {
                return Ok(1);  // one LoadLocal at MakeFn time, regardless of inner-body count
            }
        }
    }
    Ok(0)
}

/// Handle `(let [n v ...] body...)` — v exprs are visible with the outer
/// scope; body sees shadowing by any binding name == `name`.
fn count_let_refs(
    py: Python<'_>,
    after_let_head: &[PyObject],
    name: &str,
) -> PyResult<usize> {
    if after_let_head.is_empty() { return Ok(0); }
    let bindings = &after_let_head[0];
    let body = &after_let_head[1..];
    let b = bindings.bind(py);
    let pv = match b.cast::<PersistentVector>() {
        Ok(p) => p,
        Err(_) => return Ok(0),
    };
    let pv_ref = pv.get();
    if pv_ref.cnt % 2 != 0 { return Ok(0); }
    let mut total: usize = 0;
    let mut shadowed = false;
    let n = pv_ref.cnt as usize;
    let mut i = 0;
    while i < n {
        let bind_name = pv_ref.nth_internal_pub(py, i)?;
        let val = pv_ref.nth_internal_pub(py, i + 1)?;
        // Value expr uses the outer binding (unless already shadowed by a
        // prior binding in this same let).
        if !shadowed {
            total = total.saturating_add(count_outer_refs_in_form(py, &val, name)?);
        }
        // Check if this binding shadows `name`.
        let bn_b = bind_name.bind(py);
        if let Ok(sym_ref) = bn_b.cast::<Symbol>() {
            if sym_ref.get().ns.is_none() && sym_ref.get().name.as_ref() == name {
                shadowed = true;
            }
        }
        i += 2;
    }
    if !shadowed {
        for f in body {
            total = total.saturating_add(count_outer_refs_in_form(py, f, name)?);
        }
    }
    Ok(total)
}

/// `letfn*` shadowing: all bound names are visible *throughout* the form
/// (value forms and body). If `name` is one of the bound names, the entire
/// form contributes 0 outer-scope refs. Otherwise sum refs across all
/// value forms and body forms.
fn count_letfn_refs(
    py: Python<'_>,
    after_head: &[PyObject],
    name: &str,
) -> PyResult<usize> {
    if after_head.is_empty() { return Ok(0); }
    let bindings = &after_head[0];
    let body = &after_head[1..];
    let b = bindings.bind(py);
    let pv = match b.cast::<PersistentVector>() {
        Ok(p) => p,
        Err(_) => return Ok(0),
    };
    let pv_ref = pv.get();
    if pv_ref.cnt % 2 != 0 { return Ok(0); }
    let n = pv_ref.cnt as usize;
    // First pass: any binding name == `name` shadows the whole form.
    let mut i = 0;
    while i < n {
        let bind_name = pv_ref.nth_internal_pub(py, i)?;
        let bn_b = bind_name.bind(py);
        if let Ok(sym_ref) = bn_b.cast::<Symbol>() {
            if sym_ref.get().ns.is_none() && sym_ref.get().name.as_ref() == name {
                return Ok(0);
            }
        }
        i += 2;
    }
    // Not shadowed: count refs across value forms and body.
    let mut total: usize = 0;
    let mut i = 1;
    while i < n {
        let val = pv_ref.nth_internal_pub(py, i)?;
        total = total.saturating_add(count_outer_refs_in_form(py, &val, name)?);
        i += 2;
    }
    for f in body {
        total = total.saturating_add(count_outer_refs_in_form(py, f, name)?);
    }
    Ok(total)
}

/// Return the subset of `names` captured by at least one inner fn* in the
/// body. Used to blacklist those locals from mid-body clearing (though
/// scope-end clearing is still safe for them).
pub fn body_has_fn_capturing(
    py: Python<'_>,
    body: &[PyObject],
    name_to_slot: &std::collections::HashMap<Arc<str>, u16>,
) -> PyResult<std::collections::HashSet<String>> {
    let mut out = std::collections::HashSet::new();
    for (name, _) in name_to_slot.iter() {
        let mut captured = false;
        for f in body {
            if form_has_fn_capturing(py, f, name.as_ref())? {
                captured = true;
                break;
            }
        }
        if captured {
            out.insert(name.to_string());
        }
    }
    Ok(out)
}

fn form_has_fn_capturing(
    py: Python<'_>,
    form: &PyObject,
    name: &str,
) -> PyResult<bool> {
    let b = form.bind(py);

    if b.cast::<Symbol>().is_ok()
        || form.is_none(py)
        || b.cast::<PyBool>().is_ok()
        || b.cast::<PyInt>().is_ok()
        || b.cast::<PyFloat>().is_ok()
        || b.cast::<PyString>().is_ok()
        || b.cast::<crate::keyword::Keyword>().is_ok()
    {
        return Ok(false);
    }

    if let Ok(pv) = b.cast::<PersistentVector>() {
        let pv_ref = pv.get();
        for i in 0..(pv_ref.cnt as usize) {
            if form_has_fn_capturing(py, &pv_ref.nth_internal_pub(py, i)?, name)? {
                return Ok(true);
            }
        }
        return Ok(false);
    }
    if b.cast::<PersistentHashMap>().is_ok() || b.cast::<PersistentArrayMap>().is_ok() {
        for item in b.try_iter()? {
            let k = item?.unbind();
            let v = b.call_method1("val_at", (k.clone_ref(py),))?.unbind();
            if form_has_fn_capturing(py, &k, name)? { return Ok(true); }
            if form_has_fn_capturing(py, &v, name)? { return Ok(true); }
        }
        return Ok(false);
    }
    if b.cast::<PersistentHashSet>().is_ok() {
        for item in b.try_iter()? {
            if form_has_fn_capturing(py, &item?.unbind(), name)? {
                return Ok(true);
            }
        }
        return Ok(false);
    }

    let items = if let Ok(_pl) = b.cast::<PersistentList>() {
        list_items(py, form)?
    } else if b.cast::<EmptyList>().is_ok() {
        return Ok(false);
    } else if is_non_list_seq(py, form)? {
        collect_seq(py, form)?
    } else {
        return Ok(false);
    };
    if items.is_empty() { return Ok(false); }

    let head_b = items[0].bind(py);
    if let Ok(sym_ref) = head_b.cast::<Symbol>() {
        let s = sym_ref.get();
        if s.ns.is_none() {
            if s.name.as_ref() == "quote" {
                return Ok(false);
            }
            if s.name.as_ref() == "fn*" {
                return fn_captures_name(py, &items[1..], name);
            }
            if matches!(s.name.as_ref(), "let*" | "loop*") {
                return let_captures_name(py, &items[1..], name);
            }
            if s.name.as_ref() == "letfn*" {
                return letfn_captures_name(py, &items[1..], name);
            }
        }
    }
    for item in &items {
        if form_has_fn_capturing(py, item, name)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn fn_captures_name(
    py: Python<'_>,
    after_fn_head: &[PyObject],
    name: &str,
) -> PyResult<bool> {
    Ok(count_fn_captures(py, after_fn_head, name)? > 0)
}

fn let_captures_name(
    py: Python<'_>,
    after_let_head: &[PyObject],
    name: &str,
) -> PyResult<bool> {
    if after_let_head.is_empty() { return Ok(false); }
    let bindings = &after_let_head[0];
    let body = &after_let_head[1..];
    let b = bindings.bind(py);
    let pv = match b.cast::<PersistentVector>() {
        Ok(p) => p,
        Err(_) => return Ok(false),
    };
    let pv_ref = pv.get();
    if pv_ref.cnt % 2 != 0 { return Ok(false); }
    let mut shadowed = false;
    let n = pv_ref.cnt as usize;
    let mut i = 0;
    while i < n {
        let bind_name = pv_ref.nth_internal_pub(py, i)?;
        let val = pv_ref.nth_internal_pub(py, i + 1)?;
        if !shadowed && form_has_fn_capturing(py, &val, name)? {
            return Ok(true);
        }
        let bn_b = bind_name.bind(py);
        if let Ok(sym_ref) = bn_b.cast::<Symbol>() {
            if sym_ref.get().ns.is_none() && sym_ref.get().name.as_ref() == name {
                shadowed = true;
            }
        }
        i += 2;
    }
    if !shadowed {
        for f in body {
            if form_has_fn_capturing(py, f, name)? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn letfn_captures_name(
    py: Python<'_>,
    after_head: &[PyObject],
    name: &str,
) -> PyResult<bool> {
    if after_head.is_empty() { return Ok(false); }
    let bindings = &after_head[0];
    let body = &after_head[1..];
    let b = bindings.bind(py);
    let pv = match b.cast::<PersistentVector>() {
        Ok(p) => p,
        Err(_) => return Ok(false),
    };
    let pv_ref = pv.get();
    if pv_ref.cnt % 2 != 0 { return Ok(false); }
    let n = pv_ref.cnt as usize;
    // Shadowing: any binding name == `name` blocks the entire form.
    let mut i = 0;
    while i < n {
        let bind_name = pv_ref.nth_internal_pub(py, i)?;
        let bn_b = bind_name.bind(py);
        if let Ok(sym_ref) = bn_b.cast::<Symbol>() {
            if sym_ref.get().ns.is_none() && sym_ref.get().name.as_ref() == name {
                return Ok(false);
            }
        }
        i += 2;
    }
    let mut i = 1;
    while i < n {
        let val = pv_ref.nth_internal_pub(py, i)?;
        if form_has_fn_capturing(py, &val, name)? { return Ok(true); }
        i += 2;
    }
    for f in body {
        if form_has_fn_capturing(py, f, name)? { return Ok(true); }
    }
    Ok(false)
}
