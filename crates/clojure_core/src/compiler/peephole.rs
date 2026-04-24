//! Post-emit peephole optimizations on the `Op` stream.

use crate::compiler::op::Op;
use crate::compiler::pool::PoolBuilder;

/// Collapses `[Deref(ix), …N single-push ops…, Invoke(N)]` patterns into a
/// single `InvokeVar(ix, N)` op. Runs once per compiled method, after all
/// `emit` calls (so jump-patching has already happened on the original
/// indices). Remaps jump targets via a deletion prefix-sum so control flow
/// is preserved.
///
/// Safety rules for fusing a candidate:
/// - The `N` ops between the `Deref` and the `Invoke` must each be a
///   single-value push (`PushConst` / `LoadLocal` / `LoadCapture` / `LoadSelf`
///   / `Deref` / `LoadVar`). Compound expressions as args aren't fused.
/// - No jump target may land inside the arg-push span (`deref_idx+1
///   ..=invoke_idx`). A target at `deref_idx` itself is fine — in the fused
///   form it remaps to "start emitting args", which has the same effect
///   because `InvokeVar` does the deref internally.
pub(super) fn fuse_deref_invoke_pass(code: &mut Vec<Op>, pool: &mut PoolBuilder) {
    let code_len = code.len();
    if code_len < 3 { return; }

    // Gather every jump target in one pass so the safety check is O(1) per
    // candidate rather than re-scanning the code vector each time.
    let mut jump_targets: std::collections::HashSet<u32> = std::collections::HashSet::new();
    for op in code.iter() {
        match op {
            Op::Jump(t) | Op::JumpIfFalsy(t) => { jump_targets.insert(*t); }
            Op::PushHandler(t, _) => { jump_targets.insert(*t); }
            _ => {}
        }
    }

    // Identify all fuseable (deref_idx, invoke_idx, var_ix, nargs) tuples.
    let mut fusions: Vec<(usize, usize, u16, u8)> = Vec::new();
    for invoke_idx in 0..code_len {
        let nargs = match code[invoke_idx] {
            Op::Invoke(n) => n,
            _ => continue,
        };
        let n = nargs as usize;
        if invoke_idx < n + 1 { continue; }

        let mut ok = true;
        for i in 1..=n {
            match &code[invoke_idx - i] {
                Op::PushConst(_) | Op::LoadLocal(_) | Op::LoadCapture(_)
                | Op::LoadSelf | Op::Deref(_) | Op::LoadVar(_) => {}
                _ => { ok = false; break; }
            }
        }
        if !ok { continue; }

        let deref_idx = invoke_idx - n - 1;
        let var_ix = match &code[deref_idx] {
            Op::Deref(ix) => *ix,
            _ => continue,
        };

        // Block fusion if any jump lands in (deref_idx, invoke_idx]. Mid-arg
        // targets would have mismatched stack state; target == invoke_idx
        // would have counted on the target already being on the stack, which
        // isn't true post-fusion. Targets at deref_idx are safe — see the
        // doc comment above.
        let mut blocked = false;
        for t in (deref_idx + 1)..=invoke_idx {
            if jump_targets.contains(&(t as u32)) { blocked = true; break; }
        }
        if blocked { continue; }

        fusions.push((deref_idx, invoke_idx, var_ix, nargs));
    }

    if fusions.is_empty() { return; }

    // Apply in reverse order so earlier fusions' indices stay valid.
    for (deref_idx, invoke_idx, var_ix, nargs) in fusions.iter().rev() {
        let ic_slot = pool.alloc_ic_slot();
        code[*invoke_idx] = match *nargs {
            0 => Op::InvokeVar0(*var_ix, ic_slot),
            1 => Op::InvokeVar1(*var_ix, ic_slot),
            2 => Op::InvokeVar2(*var_ix, ic_slot),
            _ => Op::InvokeVar(*var_ix, *nargs, ic_slot),
        };
        code.remove(*deref_idx);
    }

    // Remap jump targets: each target shifts down by the number of deletions
    // at strictly-lower positions (prefix sum).
    let mut sorted_deletes: Vec<usize> = fusions.iter().map(|(d, _, _, _)| *d).collect();
    sorted_deletes.sort();
    let mut deletions_before = vec![0u32; code_len + 1];
    let mut cum = 0u32;
    let mut di = 0;
    for i in 0..=code_len {
        while di < sorted_deletes.len() && sorted_deletes[di] < i {
            cum += 1;
            di += 1;
        }
        deletions_before[i] = cum;
    }
    for op in code.iter_mut() {
        match op {
            Op::Jump(t) | Op::JumpIfFalsy(t) => {
                let old = *t as usize;
                *t = (old as u32) - deletions_before[old];
            }
            Op::PushHandler(t, _) => {
                let old = *t as usize;
                *t = (old as u32) - deletions_before[old];
            }
            _ => {}
        }
    }
}
