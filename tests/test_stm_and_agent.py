"""Tests for Ref / LockingTransaction (STM) and Agent."""
import threading
import time
import pytest

from clojure.lang import (
    Ref, LockingTransaction, dosync,
    Agent, Keyword,
    IRef, IDeref, IFn,
)


# =========================================================================
# Ref basics
# =========================================================================

class TestRefBasics:
    def test_initial_deref_outside_tx(self):
        r = Ref(42)
        assert r.deref() == 42

    def test_set_outside_tx_raises(self):
        r = Ref(0)
        with pytest.raises(RuntimeError):
            r.set(99)

    def test_alter_outside_tx_raises(self):
        r = Ref(0)
        with pytest.raises(RuntimeError):
            r.alter(lambda x: x + 1)

    def test_commute_outside_tx_raises(self):
        r = Ref(0)
        with pytest.raises(RuntimeError):
            r.commute(lambda x: x + 1)

    def test_touch_outside_tx_raises(self):
        r = Ref(0)
        with pytest.raises(RuntimeError):
            r.touch()


# =========================================================================
# Transactions
# =========================================================================

class TestTransaction:
    def test_set_in_dosync(self):
        r = Ref(0)
        result = dosync(lambda: r.set(99))
        assert result == 99
        assert r.deref() == 99

    def test_alter_returns_new_value(self):
        r = Ref(10)
        result = dosync(lambda: r.alter(lambda x: x * 2))
        assert result == 20
        assert r.deref() == 20

    def test_alter_with_args(self):
        r = Ref(10)
        result = dosync(lambda: r.alter(lambda x, n: x + n, 5))
        assert result == 15

    def test_commute_returns_new_value(self):
        r = Ref(10)
        result = dosync(lambda: r.commute(lambda x, n: x + n, 5))
        assert result == 15
        assert r.deref() == 15

    def test_set_after_commute_raises(self):
        r = Ref(0)
        def tx():
            r.commute(lambda x: x + 1)
            r.set(99)
        with pytest.raises(Exception):  # commute-then-set is illegal
            dosync(tx)

    def test_two_refs_in_one_tx(self):
        a = Ref(100)
        b = Ref(0)
        def transfer():
            a.set(a.deref() - 50)
            b.set(b.deref() + 50)
        dosync(transfer)
        assert a.deref() == 50
        assert b.deref() == 50

    def test_tx_is_atomic_either_all_or_nothing(self):
        # If tx fn raises, NO writes commit.
        r = Ref(0)
        def failing_tx():
            r.set(99)
            raise RuntimeError("boom")
        with pytest.raises(RuntimeError):
            dosync(failing_tx)
        assert r.deref() == 0   # rollback

    def test_tx_returns_fn_result(self):
        assert dosync(lambda: 42) == 42
        assert dosync(lambda: "hello") == "hello"

    def test_nested_dosync_passes_through(self):
        r = Ref(0)
        def inner():
            r.set(99)
            return r.deref()
        def outer():
            inner_result = dosync(inner)  # nested — same tx
            return inner_result
        assert dosync(outer) == 99
        assert r.deref() == 99


# =========================================================================
# Touch / ensure
# =========================================================================

class TestTouch:
    def test_touch_in_tx(self):
        r = Ref(42)
        # touch doesn't modify; just pins the read-point.
        result = dosync(lambda: (r.touch(), r.deref())[1])
        assert result == 42


# =========================================================================
# Concurrent transactions
# =========================================================================

class TestConcurrentSTM:
    def test_concurrent_increments(self):
        # 4 threads × 100 increments via alter → should see exactly 400.
        r = Ref(0)
        def worker():
            for _ in range(100):
                dosync(lambda: r.alter(lambda x: x + 1))
        threads = [threading.Thread(target=worker) for _ in range(4)]
        for t in threads: t.start()
        for t in threads: t.join()
        assert r.deref() == 400

    def test_concurrent_commute_no_lost_updates(self):
        # Commute is order-independent (unlike alter): all writes commit.
        r = Ref(0)
        def worker():
            for _ in range(100):
                dosync(lambda: r.commute(lambda x: x + 1))
        threads = [threading.Thread(target=worker) for _ in range(4)]
        for t in threads: t.start()
        for t in threads: t.join()
        assert r.deref() == 400

    def test_bank_transfer_invariant(self):
        # Classic STM smoke-test: transfer money between accounts; total
        # should remain constant.
        accounts = [Ref(100) for _ in range(5)]
        n_transfers = 200

        def transfer(from_acct, to_acct, amount):
            def tx():
                from_acct.set(from_acct.deref() - amount)
                to_acct.set(to_acct.deref() + amount)
            dosync(tx)

        def worker(idx):
            import random
            rng = random.Random(idx)
            for _ in range(n_transfers):
                a, b = rng.sample(range(len(accounts)), 2)
                transfer(accounts[a], accounts[b], rng.randint(1, 10))

        threads = [threading.Thread(target=worker, args=(i,)) for i in range(4)]
        for t in threads: t.start()
        for t in threads: t.join()

        total = sum(r.deref() for r in accounts)
        assert total == 100 * len(accounts)


class TestRefInterfaces:
    def test_isinstance(self):
        r = Ref(0)
        assert isinstance(r, IRef)
        assert isinstance(r, IDeref)
        assert isinstance(r, IFn)

    def test_refs_are_ordered_by_id(self):
        a = Ref(0)
        b = Ref(0)
        # The newer ref has a higher id.
        assert a < b or b < a
        # Ordering is consistent.
        assert (a < b) == (a._id < b._id)


# =========================================================================
# History bounds
# =========================================================================

class TestRefHistory:
    def test_default_history_bounds(self):
        r = Ref(0)
        assert r.get_min_history() == 0
        assert r.get_max_history() == 10

    def test_set_history_bounds_chainable(self):
        r = Ref(0)
        assert r.set_min_history(2) is r
        assert r.set_max_history(5) is r
        assert r.get_min_history() == 2
        assert r.get_max_history() == 5


# =========================================================================
# Validators / watches (inherited from ARef)
# =========================================================================

class TestRefValidator:
    def test_validator_rejects_invalid_set(self):
        r = Ref(0)
        r.set_validator(lambda v: v >= 0)
        with pytest.raises(RuntimeError):
            dosync(lambda: r.set(-1))


class TestRefWatches:
    def test_watch_called_after_commit(self):
        r = Ref(0)
        seen = []
        r.add_watch("w", lambda k, ref, old, new: seen.append((old, new)))
        dosync(lambda: r.set(99))
        # Watches notify after commit, may take a tick on free-threaded — but
        # they're synchronous in our impl.
        assert seen == [(0, 99)]


# =========================================================================
# Agent
# =========================================================================

class TestAgentBasics:
    def test_initial_deref(self):
        ag = Agent(42)
        assert ag.deref() == 42

    def test_send_updates_state(self):
        ag = Agent(0)
        ag.send(lambda x: x + 1)
        # send is async — wait briefly
        time.sleep(0.1)
        assert ag.deref() == 1

    def test_send_with_args(self):
        ag = Agent(10)
        ag.send(lambda x, n: x * n, 3)
        time.sleep(0.1)
        assert ag.deref() == 30

    def test_send_off(self):
        ag = Agent(0)
        ag.send_off(lambda x: x + 100)
        time.sleep(0.1)
        assert ag.deref() == 100


class TestAgentMany:
    def test_many_sends_serialize_correctly(self):
        # Sends to a single agent are processed in order.
        ag = Agent(0)
        for _ in range(50):
            ag.send(lambda x: x + 1)
        # Wait for all to drain.
        deadline = time.time() + 5.0
        while ag.get_queue_count() > 0 and time.time() < deadline:
            time.sleep(0.01)
        assert ag.deref() == 50


class TestAgentErrors:
    def test_failing_action_blocks_continuing_state(self):
        ag = Agent(0)
        ag.set_error_mode(Keyword.intern("fail"))
        ag.send(lambda x: 1 / 0)   # ZeroDivisionError
        # Wait for failure to register.
        deadline = time.time() + 1.0
        while ag.get_error() is None and time.time() < deadline:
            time.sleep(0.01)
        assert ag.get_error() is not None

    def test_continue_mode_swallows_error(self):
        ag = Agent(0)
        ag.set_error_mode(Keyword.intern("continue"))
        ag.send(lambda x: 1 / 0)
        time.sleep(0.1)
        # In :continue mode, agent isn't put in error state.
        assert ag.get_error() is None

    def test_restart_clears_error(self):
        ag = Agent(0)
        ag.set_error_mode(Keyword.intern("fail"))
        ag.send(lambda x: 1 / 0)
        deadline = time.time() + 1.0
        while ag.get_error() is None and time.time() < deadline:
            time.sleep(0.01)
        assert ag.get_error() is not None

        ag.restart(99, clear_actions=True)
        assert ag.get_error() is None
        assert ag.deref() == 99


class TestAgentInTransaction:
    def test_send_during_tx_is_deferred(self):
        # Sends made inside a transaction are queued and only dispatched on commit.
        ag = Agent(0)
        r = Ref(False)
        # We send during the tx; the agent shouldn't see it until tx commits.

        observed_during_tx = []

        def tx():
            ag.send(lambda x: x + 1)
            # Inside the tx, the agent's state hasn't changed.
            observed_during_tx.append(ag.deref())

        dosync(tx)
        time.sleep(0.1)
        # The send was dispatched on commit and processed.
        assert observed_during_tx == [0]
        assert ag.deref() == 1


class TestAgentInterfaces:
    def test_isinstance_iref(self):
        ag = Agent(0)
        assert isinstance(ag, IRef)
        assert isinstance(ag, IDeref)


# =========================================================================
# Cleanup hook — shut down executors at the end of the test session.
# =========================================================================

@pytest.fixture(scope="session", autouse=True)
def _shutdown_agent_pools():
    yield
    Agent.shutdown_executors()
