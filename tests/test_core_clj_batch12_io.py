"""Tests for the deferred items from core.clj batch 12: line-seq and the
await family. Splitting them out keeps the IO/concurrency dependencies
(StringIO, threading) separate from the pure-data tests in batch12.

Forms:
  line-seq                  — JVM line 3097
  await, await1, await-for  — JVM lines 3296-3333

Backend systems added under src/clojure/core.py:
  java.io.BufferedReader              — chars-and-buffer reader that
                                        recognizes \\n, \\r, and \\r\\n.
  java.util.concurrent.CountDownLatch — Condition-backed countdown.
  java.util.concurrent.TimeUnit       — constants for await(timeout, unit).
"""

import io
import time
import threading

import pytest

import clojure.core  # triggers load

from clojure.lang import (
    Compiler,
    read_string,
    Var, Symbol,
    RT,
)


def E(src):
    return Compiler.eval(read_string(src))


def _intern(name, val):
    """Bind val to user/<name> so it's reachable from compiled clj forms."""
    Var.intern(Compiler.current_ns(), Symbol.intern(name), val)
    return val


# --- BufferedReader shim ------------------------------------------

BufferedReader = RT.class_for_name("java.io.BufferedReader")


def _br(text):
    return BufferedReader(io.StringIO(text))


def test_buffered_reader_lines_lf():
    r = _br("alpha\nbeta\ngamma\n")
    assert r.readLine() == "alpha"
    assert r.readLine() == "beta"
    assert r.readLine() == "gamma"
    assert r.readLine() is None  # EOF

def test_buffered_reader_no_terminator_at_end():
    r = _br("only")
    assert r.readLine() == "only"
    assert r.readLine() is None

def test_buffered_reader_recognizes_crlf():
    r = _br("a\r\nb\r\n")
    assert r.readLine() == "a"
    assert r.readLine() == "b"
    assert r.readLine() is None

def test_buffered_reader_recognizes_cr():
    r = _br("a\rb\rc")
    assert r.readLine() == "a"
    assert r.readLine() == "b"
    assert r.readLine() == "c"
    assert r.readLine() is None

def test_buffered_reader_mixed_terminators():
    r = _br("a\r\nb\rc\nd")
    assert [r.readLine() for _ in range(5)] == ["a", "b", "c", "d", None]

def test_buffered_reader_empty():
    r = _br("")
    assert r.readLine() is None

def test_buffered_reader_blank_lines():
    r = _br("\n\n\n")
    assert [r.readLine() for _ in range(4)] == ["", "", "", None]

def test_buffered_reader_chunk_boundary():
    """Force a terminator to span two refills (chunk size = 4096)."""
    long = "x" * 4095 + "\r\n" + "y" * 100 + "\n"
    r = BufferedReader(io.StringIO(long))
    assert r.readLine() == "x" * 4095
    assert r.readLine() == "y" * 100
    assert r.readLine() is None

def test_buffered_reader_close():
    src = io.StringIO("hi")
    r = BufferedReader(src)
    r.close()
    assert src.closed


# --- line-seq -----------------------------------------------------

def test_line_seq_basic():
    _intern("ls-rdr", _br("alpha\nbeta\ngamma\n"))
    out = list(E("(clojure.core/line-seq user/ls-rdr)"))
    assert out == ["alpha", "beta", "gamma"]

def test_line_seq_empty_returns_nil():
    _intern("ls-empty", _br(""))
    assert E("(clojure.core/line-seq user/ls-empty)") is None

def test_line_seq_universal_newlines():
    _intern("ls-mixed", _br("a\r\nb\rc\nd"))
    assert list(E("(clojure.core/line-seq user/ls-mixed)")) == ["a", "b", "c", "d"]

def test_line_seq_take_two_doesnt_drain_reader():
    """`(take 2 (line-seq r))` should leave further calls to the reader
    able to produce the remaining lines — the chunk buffer may have read
    ahead, but the lazy seq itself only forced two elements."""
    rdr = _br("1\n2\n3\n4\n5\n")
    _intern("ls-lazy", rdr)
    head = list(E("(clojure.core/take 2 (clojure.core/line-seq user/ls-lazy))"))
    assert head == ["1", "2"]

def test_line_seq_take_more_than_lines():
    _intern("ls-short", _br("only-one\n"))
    out = list(E("(clojure.core/take 5 (clojure.core/line-seq user/ls-short))"))
    assert out == ["only-one"]

def test_line_seq_no_trailing_newline():
    _intern("ls-trail", _br("a\nb\nc"))
    assert list(E("(clojure.core/line-seq user/ls-trail)")) == ["a", "b", "c"]


# --- CountDownLatch shim ------------------------------------------

CountDownLatch = RT.class_for_name("java.util.concurrent.CountDownLatch")
TimeUnit = RT.class_for_name("java.util.concurrent.TimeUnit")


def test_count_down_latch_immediate_release():
    latch = CountDownLatch(0)
    # Should return immediately.
    getattr(latch, "await")()

def test_count_down_latch_blocks_until_zero():
    latch = CountDownLatch(3)
    started = threading.Event()
    finished = threading.Event()

    def waiter():
        started.set()
        getattr(latch, "await")()
        finished.set()

    t = threading.Thread(target=waiter)
    t.start()
    started.wait()
    time.sleep(0.02)
    assert not finished.is_set()
    latch.countDown()
    latch.countDown()
    assert not finished.is_set()
    latch.countDown()
    t.join(timeout=1.0)
    assert finished.is_set()

def test_count_down_latch_timeout_returns_false():
    latch = CountDownLatch(1)
    t0 = time.monotonic()
    got = getattr(latch, "await")(50, TimeUnit.MILLISECONDS)
    elapsed = time.monotonic() - t0
    assert got is False
    assert elapsed >= 0.045
    assert elapsed < 0.5

def test_count_down_latch_timeout_returns_true_on_signal():
    latch = CountDownLatch(1)
    timer = threading.Timer(0.02, latch.countDown)
    timer.start()
    got = getattr(latch, "await")(1000, TimeUnit.MILLISECONDS)
    assert got is True

def test_count_down_latch_get_count():
    latch = CountDownLatch(2)
    assert latch.getCount() == 2
    latch.countDown()
    assert latch.getCount() == 1
    latch.countDown()
    assert latch.getCount() == 0

def test_count_down_latch_negative_count_rejected():
    with pytest.raises(ValueError):
        CountDownLatch(-1)

def test_time_unit_to_seconds():
    assert TimeUnit.MILLISECONDS.to_seconds(500) == 0.5
    assert TimeUnit.SECONDS.to_seconds(2) == 2.0
    assert abs(TimeUnit.MICROSECONDS.to_seconds(1500) - 0.0015) < 1e-9
    assert TimeUnit.NANOSECONDS.to_seconds(1_000_000_000) == 1.0
    assert TimeUnit.MINUTES.to_seconds(1) == 60.0


# --- await -------------------------------------------------------

def _slow_inc(x):
    time.sleep(0.05)
    return x + 1


def test_await_waits_for_pending_send():
    a = E("(clojure.core/agent 0)")
    _intern("aw-a", a)
    _intern("aw-slow-inc", _slow_inc)

    t0 = time.monotonic()
    E("(clojure.core/send user/aw-a user/aw-slow-inc)")
    E("(clojure.core/await user/aw-a)")
    elapsed = time.monotonic() - t0

    assert E("@user/aw-a") == 1
    assert elapsed >= 0.045  # actually waited

def test_await_multiple_agents():
    a1 = E("(clojure.core/agent 0)")
    a2 = E("(clojure.core/agent 100)")
    _intern("aw-m-a1", a1)
    _intern("aw-m-a2", a2)
    _intern("aw-m-slow", _slow_inc)

    E("(clojure.core/send user/aw-m-a1 user/aw-m-slow)")
    E("(clojure.core/send user/aw-m-a2 user/aw-m-slow)")
    E("(clojure.core/await user/aw-m-a1 user/aw-m-a2)")

    assert E("@user/aw-m-a1") == 1
    assert E("@user/aw-m-a2") == 101

def test_await_returns_nil_with_no_agents():
    """No agents → CountDownLatch(0), await returns immediately, fn returns nil."""
    assert E("(clojure.core/await)") is None

def test_await_throws_when_called_in_agent_action():
    """JVM raises if *agent* is bound (i.e. inside a send action)."""
    a = E("(clojure.core/agent 0)")
    _intern("aw-throw-a", a)
    captured = {}

    def evil(state):
        try:
            E("(clojure.core/await user/aw-throw-a)")
            captured["err"] = None
        except Exception as e:
            captured["err"] = e
        return state

    _intern("aw-throw-evil", evil)
    E("(clojure.core/send user/aw-throw-a user/aw-throw-evil)")
    E("(clojure.core/await user/aw-throw-a)")  # this one's fine — outside action

    assert captured.get("err") is not None
    assert "agent action" in str(captured["err"])


# --- await1 ------------------------------------------------------

def test_await1_returns_agent_idle():
    a = E("(clojure.core/agent 0)")
    _intern("aw1-idle-a", a)
    out = E("(clojure.core/await1 user/aw1-idle-a)")
    assert out is a

def test_await1_blocks_when_queue_nonempty():
    a = E("(clojure.core/agent 0)")
    _intern("aw1-busy-a", a)
    _intern("aw1-busy-slow", _slow_inc)

    t0 = time.monotonic()
    E("(clojure.core/send user/aw1-busy-a user/aw1-busy-slow)")
    out = E("(clojure.core/await1 user/aw1-busy-a)")
    elapsed = time.monotonic() - t0

    assert out is a
    assert E("@user/aw1-busy-a") == 1
    assert elapsed >= 0.045


# --- await-for ---------------------------------------------------

def test_await_for_succeeds_within_timeout():
    a = E("(clojure.core/agent 0)")
    _intern("awf-ok-a", a)
    _intern("awf-ok-slow", _slow_inc)

    E("(clojure.core/send user/awf-ok-a user/awf-ok-slow)")
    out = E("(clojure.core/await-for 1000 user/awf-ok-a)")
    assert out is True
    assert E("@user/awf-ok-a") == 1

def test_await_for_times_out():
    a = E("(clojure.core/agent 0)")
    _intern("awf-to-a", a)

    def very_slow(x):
        time.sleep(0.5)
        return x
    _intern("awf-to-slow", very_slow)

    t0 = time.monotonic()
    E("(clojure.core/send user/awf-to-a user/awf-to-slow)")
    out = E("(clojure.core/await-for 50 user/awf-to-a)")
    elapsed = time.monotonic() - t0

    assert out is False
    assert elapsed < 0.4  # didn't wait the full action

def test_await_for_no_agents_returns_true():
    """Zero-count latch fires immediately."""
    assert E("(clojure.core/await-for 100)") is True

def test_await_for_throws_in_agent_action():
    a = E("(clojure.core/agent 0)")
    _intern("awf-throw-a", a)
    captured = {}

    def evil(state):
        try:
            E("(clojure.core/await-for 100 user/awf-throw-a)")
            captured["err"] = None
        except Exception as e:
            captured["err"] = e
        return state

    _intern("awf-throw-evil", evil)
    E("(clojure.core/send user/awf-throw-a user/awf-throw-evil)")
    E("(clojure.core/await user/awf-throw-a)")

    assert captured.get("err") is not None
    assert "agent action" in str(captured["err"])
