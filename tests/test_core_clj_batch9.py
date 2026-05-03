"""Tests for core.clj batch 9 (lines 2064-2573):

setup-reference (private), agent, set-agent-send-executor!,
set-agent-send-off-executor!, send, send-off, send-via,
release-pending-sends, add-watch, remove-watch,
agent-error, restart-agent, set-error-handler!, error-handler,
set-error-mode!, error-mode, agent-errors, clear-agent-errors,
shutdown-agents,
ref, atom, swap!, swap-vals!, compare-and-set!, reset!, reset-vals!,
set-validator!, get-validator, alter-meta!, reset-meta!,
commute, alter, ref-set, ref-history-count, ref-min-history,
ref-max-history, ensure, sync, io!,
volatile!, vreset!, vswap!, volatile?
"""

import time
import pytest

import clojure.core  # triggers load

from clojure.lang import (
    Compiler,
    read_string,
    Var, Symbol, Keyword, Namespace, RT,
    Atom, Volatile, Ref, Agent,
    PersistentVector, PersistentArrayMap,
)


def E(src):
    return Compiler.eval(read_string(src))


def K(name):
    return Keyword.intern(None, name)


# --- atom ----------------------------------------------------------

def test_atom_create_and_deref():
    a = E("(clojure.core/atom 0)")
    assert isinstance(a, Atom)
    assert a.deref() == 0

def test_swap_inc():
    a = E("(clojure.core/atom 0)")
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb9-sa"), a)
    E("(clojure.core/swap! user/tcb9-sa clojure.core/inc)")
    assert a.deref() == 1

def test_swap_with_args():
    a = E("(clojure.core/atom 0)")
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb9-sa2"), a)
    E("(clojure.core/swap! user/tcb9-sa2 clojure.core/+ 10 20 30)")
    assert a.deref() == 60

def test_swap_vals_returns_pair():
    a = E("(clojure.core/atom 5)")
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb9-sv"), a)
    pair = E("(clojure.core/swap-vals! user/tcb9-sv clojure.core/+ 10)")
    assert list(pair) == [5, 15]

def test_compare_and_set_success():
    a = E("(clojure.core/atom 5)")
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb9-ca"), a)
    assert E("(clojure.core/compare-and-set! user/tcb9-ca 5 99)") is True
    assert a.deref() == 99

def test_compare_and_set_failure():
    a = E("(clojure.core/atom 5)")
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb9-cb"), a)
    assert E("(clojure.core/compare-and-set! user/tcb9-cb 0 99)") is False
    assert a.deref() == 5

def test_reset():
    a = E("(clojure.core/atom 99)")
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb9-r"), a)
    E("(clojure.core/reset! user/tcb9-r 0)")
    assert a.deref() == 0

def test_reset_vals_returns_pair():
    a = E("(clojure.core/atom 7)")
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb9-rv"), a)
    pair = E("(clojure.core/reset-vals! user/tcb9-rv 99)")
    assert list(pair) == [7, 99]


# --- atom with options --------------------------------------------

def test_atom_with_meta():
    a = E("(clojure.core/atom 0 :meta {:x 1})")
    assert a.meta().val_at(K("x")) == 1

def test_atom_with_validator():
    a = E("(clojure.core/atom 0 :validator clojure.core/even?)")
    assert a.deref() == 0
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb9-va"), a)
    with pytest.raises((ValueError, RuntimeError)):
        E("(clojure.core/swap! user/tcb9-va clojure.core/inc)")  # would make it 1 (odd)


# --- watches ------------------------------------------------------

def test_add_remove_watch():
    a = E("(clojure.core/atom 0)")
    events = []
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb9-aw"), a)
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb9-ws"),
               lambda k, r, o, n: events.append((k, o, n)))
    E("(clojure.core/add-watch user/tcb9-aw :wk user/tcb9-ws)")
    E("(clojure.core/swap! user/tcb9-aw clojure.core/inc)")
    E("(clojure.core/swap! user/tcb9-aw clojure.core/inc)")
    assert events == [(K("wk"), 0, 1), (K("wk"), 1, 2)]
    E("(clojure.core/remove-watch user/tcb9-aw :wk)")
    E("(clojure.core/swap! user/tcb9-aw clojure.core/inc)")
    # Same length — watch was removed
    assert len(events) == 2


# --- ref / sync ---------------------------------------------------

def test_ref_create_and_deref():
    r = E("(clojure.core/ref 100)")
    assert isinstance(r, Ref)
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb9-rf"), r)
    assert E("(clojure.core/deref user/tcb9-rf)") == 100

def test_alter_in_sync():
    r = E("(clojure.core/ref 100)")
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb9-rfa"), r)
    E("(clojure.core/sync nil (clojure.core/alter user/tcb9-rfa clojure.core/+ 5))")
    assert r.deref() == 105

def test_alter_with_multiple_args():
    r = E("(clojure.core/ref 0)")
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb9-rfm"), r)
    E("(clojure.core/sync nil (clojure.core/alter user/tcb9-rfm clojure.core/+ 1 2 3))")
    assert r.deref() == 6

def test_commute_in_sync():
    r = E("(clojure.core/ref [])")
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb9-rfc"), r)
    E("(clojure.core/sync nil (clojure.core/commute user/tcb9-rfc clojure.core/conj 1))")
    assert list(r.deref()) == [1]

def test_ref_set_in_sync():
    r = E("(clojure.core/ref 0)")
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb9-rfs"), r)
    E("(clojure.core/sync nil (clojure.core/ref-set user/tcb9-rfs 99))")
    assert r.deref() == 99

def test_ref_outside_sync_raises():
    r = E("(clojure.core/ref 0)")
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb9-rfo"), r)
    with pytest.raises((RuntimeError, Exception)):
        E("(clojure.core/alter user/tcb9-rfo clojure.core/inc)")

def test_ref_history_settings():
    r = E("(clojure.core/ref 0 :max-history 5 :min-history 2)")
    assert r.get_max_history() == 5
    assert r.get_min_history() == 2

def test_ref_history_count():
    r = E("(clojure.core/ref 0)")
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb9-rh"), r)
    initial = E("(clojure.core/ref-history-count user/tcb9-rh)")
    assert isinstance(initial, int)


# --- agent --------------------------------------------------------

def test_agent_create_and_deref():
    a = E("(clojure.core/agent 0)")
    assert isinstance(a, Agent)
    assert a.deref() == 0

def test_agent_send_then_deref_after_dispatch():
    a = E("(clojure.core/agent 0)")
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb9-ag"), a)
    E("(clojure.core/send user/tcb9-ag clojure.core/inc)")
    # Wait briefly for the executor to apply
    for _ in range(100):
        if a.deref() == 1:
            break
        time.sleep(0.01)
    assert a.deref() == 1

def test_agent_send_with_args():
    a = E("(clojure.core/agent 0)")
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb9-ag2"), a)
    E("(clojure.core/send user/tcb9-ag2 clojure.core/+ 10 20 30)")
    for _ in range(100):
        if a.deref() == 60:
            break
        time.sleep(0.01)
    assert a.deref() == 60

def test_agent_default_error_mode_is_fail():
    a = E("(clojure.core/agent 0)")
    assert a.get_error_mode() == K("fail")

def test_set_error_mode():
    a = E("(clojure.core/agent 0)")
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb9-em"), a)
    E("(clojure.core/set-error-mode! user/tcb9-em :continue)")
    assert E("(clojure.core/error-mode user/tcb9-em)") == K("continue")

def test_agent_creation_with_error_handler_changes_default_mode():
    # When :error-handler is provided, default mode becomes :continue
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb9-eh"),
               lambda a, e: None)
    a = E("(clojure.core/agent 0 :error-handler user/tcb9-eh)")
    assert a.get_error_mode() == K("continue")


# --- volatile -----------------------------------------------------

def test_volatile_create():
    v = E("(clojure.core/volatile! 5)")
    assert isinstance(v, Volatile)
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb9-vv"), v)
    assert E("(clojure.core/deref user/tcb9-vv)") == 5

def test_vreset():
    v = E("(clojure.core/volatile! 0)")
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb9-vr"), v)
    E("(clojure.core/vreset! user/tcb9-vr 99)")
    assert v.deref() == 99

def test_vswap():
    v = E("(clojure.core/volatile! 5)")
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb9-vs"), v)
    E("(clojure.core/vswap! user/tcb9-vs clojure.core/+ 10)")
    assert v.deref() == 15

def test_volatile_p():
    v = E("(clojure.core/volatile! 0)")
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb9-vp"), v)
    assert E("(clojure.core/volatile? user/tcb9-vp)") is True
    assert E("(clojure.core/volatile? 99)") is False


# --- meta on ref --------------------------------------------------

def test_alter_meta():
    a = E("(clojure.core/atom 0 :meta {:k 1})")
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb9-am"), a)
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb9-amf"),
               lambda m, k, v: m.assoc(k, v))
    E("(clojure.core/alter-meta! user/tcb9-am user/tcb9-amf :added 99)")
    assert a.meta().val_at(K("added")) == 99

def test_reset_meta():
    a = E("(clojure.core/atom 0 :meta {:k 1})")
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb9-rm"), a)
    E("(clojure.core/reset-meta! user/tcb9-rm {:new :meta})")
    assert a.meta().val_at(K("new")) == K("meta")
    assert a.meta().val_at(K("k")) is None


# --- io! ----------------------------------------------------------

def test_io_outside_transaction_runs():
    """io! body runs normally when not inside a transaction."""
    assert E("(clojure.core/io! :ok)") == K("ok")

def test_io_inside_transaction_raises():
    """io! inside sync raises IllegalStateException (TypeError in our port)."""
    with pytest.raises((Exception,)):
        E("(clojure.core/sync nil (clojure.core/io! :should-fail))")
