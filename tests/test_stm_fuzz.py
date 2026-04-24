"""Hypothesis-driven stress test for STM linearizability.

Generates random schedules across N threads operating on M refs with a mix
of alter / commute / ensure, and checks the observable final state against
a sequential reference model.

The key invariant: when every op is an alter/commute of the form
`(f @ref delta)` with f commutative+associative, the final value of each
ref must equal `initial + sum(deltas)` regardless of thread interleaving —
no lost updates, no phantom updates.
"""

import threading
from hypothesis import given, strategies as st, settings, HealthCheck
from clojure._core import eval_string


def _ev(s):
    return eval_string(s)


@settings(
    deadline=None,
    max_examples=30,
    suppress_health_check=[HealthCheck.function_scoped_fixture, HealthCheck.too_slow],
)
@given(
    n_refs=st.integers(min_value=1, max_value=4),
    n_threads=st.integers(min_value=2, max_value=4),
    ops_per_thread=st.lists(
        st.lists(
            # (ref_index, op_kind, delta) where op_kind is 0=alter, 1=commute
            st.tuples(
                st.integers(min_value=0, max_value=9),   # ref idx (mod n_refs)
                st.integers(min_value=0, max_value=1),
                st.integers(min_value=-5, max_value=5),
            ),
            min_size=5,
            max_size=25,
        ),
        min_size=2,
        max_size=4,
    ),
)
def test_stm_linearizable_sum(n_refs, n_threads, ops_per_thread):
    # Truncate/pad thread count to match.
    schedules = ops_per_thread[:n_threads]
    while len(schedules) < n_threads:
        schedules.append([])

    # Initialize refs in a named Clojure vector so threads can index.
    _ev(
        "(def --fuzz-refs (vec (repeatedly %d (fn* [] (ref 0)))))" % n_refs
    )

    # Compute expected per-ref sum sequentially.
    expected = [0] * n_refs
    for sched in schedules:
        for (ri, _kind, delta) in sched:
            expected[ri % n_refs] += delta

    def worker(sched):
        for (ri, kind, delta) in sched:
            r_idx = ri % n_refs
            op = "alter" if kind == 0 else "commute"
            eval_string(
                "(dosync (%s (nth --fuzz-refs %d) + %d))" % (op, r_idx, delta)
            )

    threads = [threading.Thread(target=worker, args=(s,)) for s in schedules]
    for t in threads:
        t.start()
    for t in threads:
        t.join()

    observed = [_ev("@(nth --fuzz-refs %d)" % i) for i in range(n_refs)]
    assert observed == expected, (observed, expected, schedules)


@settings(
    deadline=None,
    max_examples=20,
    suppress_health_check=[HealthCheck.function_scoped_fixture, HealthCheck.too_slow],
)
@given(
    total_increments=st.integers(min_value=10, max_value=80),
    n_threads=st.integers(min_value=2, max_value=4),
)
def test_stm_counter_no_lost_updates(total_increments, n_threads):
    _ev("(def --fuzz-counter (ref 0))")

    per_thread = total_increments // n_threads
    actual_total = per_thread * n_threads

    def worker():
        for _ in range(per_thread):
            eval_string("(dosync (alter --fuzz-counter inc))")

    threads = [threading.Thread(target=worker) for _ in range(n_threads)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()

    assert _ev("@--fuzz-counter") == actual_total
