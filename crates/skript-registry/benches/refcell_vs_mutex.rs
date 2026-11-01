//! Comparison benchmark: Rc<RefCell> vs Arc<Mutex> overhead on the
//! pattern-matching hot path.
//!
//! This benchmark doesn't use the real pattern engine (which is frozen
//! to Rc<RefCell>). Instead it builds a minimal linked-list structure
//! that mirrors the pattern engine's shape — a chain of nodes, each
//! with a `next` pointer, traversed during a "match" — in two variants:
//!
//!   1. Rc<RefCell<Node>>  — the current approach (Send = false)
//!   2. Arc<Mutex<Node>>   — the "make it Send" alternative
//!
//! The benchmark walks the chain (read-only traversal, the common case
//! during matching) and measures the per-node cost. This isolates the
//! synchronization overhead from the actual matching logic.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

/// A node in the linked list — mirrors PatternElement's shape.
struct Node {
    value: u32,
    next: Option<Ptr>,
}

// --- Variant 1: Rc<RefCell> (current, !Send) ---
type Ptr = Rc<RefCell<Node>>;

fn build_rc_chain(len: usize) -> Ptr {
    let mut tail = Rc::new(RefCell::new(Node {
        value: len as u32,
        next: None,
    }));
    for i in (0..len - 1).rev() {
        let node = Rc::new(RefCell::new(Node {
            value: i as u32,
            next: Some(tail.clone()),
        }));
        tail = node;
    }
    tail
}

fn walk_rc(node: &Ptr) -> u32 {
    let mut sum = 0u32;
    let mut current = Some(node.clone());
    while let Some(n) = current {
        let borrowed = n.borrow();
        sum = sum.wrapping_add(borrowed.value);
        current = borrowed.next.clone();
    }
    sum
}

// --- Variant 2: Arc<Mutex> (Send-safe alternative) ---
struct NodeMutex {
    value: u32,
    next: Option<Arc<Mutex<NodeMutex>>>,
}

fn build_mutex_chain(len: usize) -> Arc<Mutex<NodeMutex>> {
    let mut tail = Arc::new(Mutex::new(NodeMutex {
        value: len as u32,
        next: None,
    }));
    for i in (0..len - 1).rev() {
        let node = Arc::new(Mutex::new(NodeMutex {
            value: i as u32,
            next: Some(tail.clone()),
        }));
        tail = node;
    }
    tail
}

fn walk_mutex(node: &Arc<Mutex<NodeMutex>>) -> u32 {
    let mut sum = 0u32;
    let mut current = Some(node.clone());
    while let Some(n) = current {
        let guarded = n.lock().unwrap();
        sum = sum.wrapping_add(guarded.value);
        current = guarded.next.clone();
    }
    sum
}

fn bench_chain_walk(c: &mut Criterion) {
    // 10 nodes — typical pattern element chain length.
    let rc_chain = build_rc_chain(10);
    let mutex_chain = build_mutex_chain(10);

    let mut group = c.benchmark_group("linked-list-walk (10 nodes)");
    group.bench_function("Rc<RefCell> (current, !Send)", |b| {
        b.iter(|| black_box(walk_rc(&rc_chain)))
    });
    group.bench_function("Arc<Mutex> (Send alternative)", |b| {
        b.iter(|| black_box(walk_mutex(&mutex_chain)))
    });
    group.finish();
}

fn bench_long_chain_walk(c: &mut Criterion) {
    // 100 nodes — stress test for lock overhead.
    let rc_chain = build_rc_chain(100);
    let mutex_chain = build_mutex_chain(100);

    let mut group = c.benchmark_group("linked-list-walk (100 nodes)");
    group.bench_function("Rc<RefCell> (current, !Send)", |b| {
        b.iter(|| black_box(walk_rc(&rc_chain)))
    });
    group.bench_function("Arc<Mutex> (Send alternative)", |b| {
        b.iter(|| black_box(walk_mutex(&mutex_chain)))
    });
    group.finish();
}

criterion_group!(benches, bench_chain_walk, bench_long_chain_walk);
criterion_main!(benches);
