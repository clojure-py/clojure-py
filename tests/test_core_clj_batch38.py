"""Tests for core.clj batch 38: *clojure-version* + clojure-version +
promise + deliver (JVM 7204-7286).

Adaptations from JVM:
  - *clojure-version* is hardcoded in core.clj instead of read from
    a JVM classloader resource (clojure/version.properties).
  - promise uses Python's threading.Event (.wait / .set / .is_set)
    in place of JVM CountDownLatch (.await / .countDown / .getCount).
  - JVM IFn.invoke is exposed as Python __call__ — that's the dunder
    that makes promise instances callable. (deliver p v) compiles to
    (p v), which calls the reify class's __call__ method.
  - reify lists IBlockingDeref as a spec without method bodies; the
    multi-arity .deref under IDeref covers both arities. The
    listing-only entry is just so satisfies?/instance? answer True.
"""

import threading as _t
import time as _time

import pytest

import clojure.core  # bootstrap

from clojure.lang import (
    Compiler,
    read_string,
    Keyword,
)


def E(src):
    return Compiler.eval(read_string(src))


def K(name):
    return Keyword.intern(None, name)


# --- *clojure-version* / clojure-version -----------------------

def test_clojure_version_map_shape():
    val = E("*clojure-version*")
    keys = set(dict(val).keys())
    assert K("major") in keys
    assert K("minor") in keys
    assert K("incremental") in keys
    assert K("qualifier") in keys

def test_clojure_version_major_is_int():
    out = E("(:major *clojure-version*)")
    assert isinstance(out, int)

def test_clojure_version_fn_returns_string():
    out = E("(clojure-version)")
    assert isinstance(out, str)
    # Format: "1.12.0" or with qualifier "1.12.0-RC-1"
    assert "." in out

def test_clojure_version_fn_includes_major_minor():
    major = E("(:major *clojure-version*)")
    minor = E("(:minor *clojure-version*)")
    out = E("(clojure-version)")
    assert out.startswith(f"{major}.{minor}")

def test_clojure_version_is_dynamic():
    """*clojure-version* is dynamic, so it can be rebound for testing."""
    out = E("""
      (binding [*clojure-version* {:major 9 :minor 9 :incremental 9 :qualifier nil}]
        (clojure-version))""")
    assert out == "9.9.9"


# --- promise basic ---------------------------------------------

def test_promise_unrealized_until_delivered():
    E("(def -tcb38-p1 (promise))")
    assert E("(realized? -tcb38-p1)") is False

def test_promise_realized_after_deliver():
    E("(def -tcb38-p2 (promise))")
    E("(deliver -tcb38-p2 42)")
    assert E("(realized? -tcb38-p2)") is True

def test_promise_deref_returns_delivered_value():
    E("(def -tcb38-p3 (promise))")
    E("(deliver -tcb38-p3 :hello)")
    assert E("@-tcb38-p3") == K("hello")
    assert E("(deref -tcb38-p3)") == K("hello")

def test_promise_deref_idempotent():
    E("(def -tcb38-p4 (promise))")
    E("(deliver -tcb38-p4 99)")
    assert E("@-tcb38-p4") == 99
    assert E("@-tcb38-p4") == 99
    assert E("@-tcb38-p4") == 99


# --- deliver semantics -----------------------------------------

def test_deliver_returns_promise_first_time():
    """deliver returns the promise itself when delivery happens."""
    E("(def -tcb38-p5 (promise))")
    out = E("(deliver -tcb38-p5 1)")
    assert out is E("-tcb38-p5")

def test_deliver_returns_nil_after_first_delivery():
    """Subsequent deliveries are no-ops returning nil."""
    E("(def -tcb38-p6 (promise))")
    E("(deliver -tcb38-p6 1)")
    out = E("(deliver -tcb38-p6 2)")
    assert out is None

def test_deliver_only_first_value_visible():
    E("(def -tcb38-p7 (promise))")
    E("(deliver -tcb38-p7 :first)")
    E("(deliver -tcb38-p7 :second)")
    E("(deliver -tcb38-p7 :third)")
    assert E("@-tcb38-p7") == K("first")


# --- timeout deref ---------------------------------------------

def test_deref_timeout_returns_default_when_pending():
    E("(def -tcb38-p8 (promise))")
    out = E("(deref -tcb38-p8 30 :timed-out)")
    assert out == K("timed-out")

def test_deref_timeout_returns_value_when_already_delivered():
    E("(def -tcb38-p9 (promise))")
    E("(deliver -tcb38-p9 :ready)")
    out = E("(deref -tcb38-p9 1000 :nope)")
    assert out == K("ready")

def test_deref_timeout_returns_value_when_delivered_within_window():
    """Deliver in another thread within the timeout window."""
    E("(def -tcb38-p10 (promise))")
    E("""(future (do (py.time/sleep 0.05) (deliver -tcb38-p10 :on-time)))""")
    out = E("(deref -tcb38-p10 1000 :missed)")
    assert out == K("on-time")


# --- blocking semantics ----------------------------------------

def test_promise_deref_blocks_until_delivered():
    """Deref on an undelivered promise blocks; deliver from another
    thread releases."""
    E("(def -tcb38-p11 (promise))")
    E("""(future (do (py.time/sleep 0.05) (deliver -tcb38-p11 :unblocked)))""")
    out = E("@-tcb38-p11")
    assert out == K("unblocked")


# --- ABC registration ------------------------------------------

def test_promise_satisfies_ideref():
    E("(def -tcb38-p12 (promise))")
    assert E("(instance? clojure.lang.IDeref -tcb38-p12)") is True

def test_promise_satisfies_iblocking_deref():
    E("(def -tcb38-p13 (promise))")
    assert E("(instance? clojure.lang.IBlockingDeref -tcb38-p13)") is True

def test_promise_satisfies_ipending():
    E("(def -tcb38-p14 (promise))")
    assert E("(instance? clojure.lang.IPending -tcb38-p14)") is True

def test_promise_satisfies_ifn():
    """The promise IS callable — that's how deliver works."""
    E("(def -tcb38-p15 (promise))")
    assert E("(ifn? -tcb38-p15)") is True


# --- promise as fn ---------------------------------------------

def test_promise_callable_directly():
    """(promise val) on the promise instance delivers."""
    E("(def -tcb38-p16 (promise))")
    out = E("(-tcb38-p16 :delivered-via-call)")
    assert out is E("-tcb38-p16")
    assert E("@-tcb38-p16") == K("delivered-via-call")


# --- nil delivery ----------------------------------------------

def test_deliver_nil():
    """Delivering nil as the value works — promise becomes realized,
    deref returns nil."""
    E("(def -tcb38-p17 (promise))")
    E("(deliver -tcb38-p17 nil)")
    assert E("(realized? -tcb38-p17)") is True
    assert E("@-tcb38-p17") is None
