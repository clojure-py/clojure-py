//! Loom: validate the remote_free CAS-prepend protocol on its own
//! (against loom's atomic types, since loom can't intercept
//! core::sync::atomic).

#![cfg(loom)]

use loom::sync::atomic::{AtomicPtr, Ordering};
use loom::sync::Arc;
use loom::thread;

#[derive(Debug)]
struct Node {
    next: *mut Node,
    payload: usize,
}
unsafe impl Send for Node {}
unsafe impl Sync for Node {}

fn cas_prepend(head: &AtomicPtr<Node>, node: *mut Node) {
    loop {
        let h = head.load(Ordering::Acquire);
        unsafe { (*node).next = h; }
        if head.compare_exchange(h, node, Ordering::Release, Ordering::Acquire).is_ok() {
            return;
        }
    }
}

fn drain(head: &AtomicPtr<Node>) -> Vec<usize> {
    let mut out = Vec::new();
    let mut cur = head.swap(core::ptr::null_mut(), Ordering::AcqRel);
    while !cur.is_null() {
        unsafe {
            out.push((*cur).payload);
            cur = (*cur).next;
        }
    }
    out
}

#[test]
fn remote_free_cas_no_pointer_lost() {
    loom::model(|| {
        let head = Arc::new(AtomicPtr::new(core::ptr::null_mut()));
        // Two writer threads each prepend one node; one reader drains.
        let n1 = Box::into_raw(Box::new(Node { next: core::ptr::null_mut(), payload: 1 }));
        let n2 = Box::into_raw(Box::new(Node { next: core::ptr::null_mut(), payload: 2 }));
        let head_a = head.clone();
        let head_b = head.clone();
        let head_r = head.clone();

        let ja = thread::spawn(move || cas_prepend(&head_a, n1));
        let jb = thread::spawn(move || cas_prepend(&head_b, n2));
        let jr = thread::spawn(move || drain(&head_r));

        ja.join().unwrap();
        jb.join().unwrap();
        let drained_first = jr.join().unwrap();
        // After all threads finish, drain again to capture anything still queued.
        let drained_second = drain(&head);

        // Combined must equal {1, 2}; nothing lost, nothing duplicated.
        let mut combined = drained_first;
        combined.extend(drained_second);
        combined.sort();
        assert_eq!(combined, vec![1, 2]);

        // Cleanup boxes (loom owns memory differently, but this prevents real leaks).
        unsafe {
            drop(Box::from_raw(n1));
            drop(Box::from_raw(n2));
        }
    });
}
