//! Reads stay allocation-free while another thread holds a long read of
//! a value that has since been replaced. This binary installs a counting
//! allocator and counts every allocation in the process, so it holds one
//! test: a second test, or the harness reporting it, would be counted too.

#![allow(clippy::unwrap_used, missing_docs, unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::time::Duration;

use global_lib::{Global, Globals};

struct Counting;

static ALLOCS: AtomicUsize = AtomicUsize::new(0);

// SAFETY: every call is forwarded to the system allocator unchanged; the
// counter is only bookkeeping.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _previous = ALLOCS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: the caller upholds `GlobalAlloc::alloc`'s contract.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller upholds `GlobalAlloc::dealloc`'s contract.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Runs `hold` on another thread inside a read, waits until it is inside,
/// runs `during` on this thread, then lets the holder finish.
fn while_held(hold: impl FnOnce(&dyn Fn()) + Send + 'static, during: impl FnOnce()) {
    let inside = Arc::new(Barrier::new(2));
    let release = Arc::new(AtomicBool::new(false));
    let holder = {
        let (inside, release) = (Arc::clone(&inside), Arc::clone(&release));
        std::thread::spawn(move || {
            hold(&|| {
                inside.wait();
                while !release.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(1));
                }
            });
        })
    };
    inside.wait();
    during();
    release.store(true, Ordering::SeqCst);
    holder.join().unwrap();
}

#[test]
fn test_reads_allocate_nothing_while_a_replaced_value_is_held() {
    typed();
    by_name();
}

fn typed() {
    static G: Global<u64> = Global::new();
    let _old = G.set(1);
    for _ in 0..10 {
        let _ = G.with(|v| *v);
    }
    while_held(
        |wait| {
            let _ = G.with(|_| wait());
        },
        || {
            let _old = G.set(2); // retires the value the holder reads
            let before = ALLOCS.load(Ordering::Relaxed);
            let mut sum = 0u64;
            for _ in 0..100_000 {
                sum = sum.wrapping_add(G.with(|v| *v).unwrap());
            }
            let allocs = ALLOCS.load(Ordering::Relaxed) - before;
            assert_eq!(sum, 200_000);
            assert_eq!(allocs, 0, "reads allocated while a replaced value was held");
        },
    );
}

fn by_name() {
    let g: &'static Globals = Box::leak(Box::new(Globals::new()));
    g.set("a", 1u64);
    g.set("b", 2u64);
    for _ in 0..10 {
        let _ = g.with::<u64, _>("b", |v| *v);
    }
    while_held(
        move |wait| {
            let _ = g.with::<u64, _>("a", |_| wait());
        },
        || {
            g.set("unrelated", 3u8); // retires the map the holder reads
            let before = ALLOCS.load(Ordering::Relaxed);
            for _ in 0..100_000 {
                assert_eq!(g.with::<u64, _>("b", |v| *v), Some(2));
            }
            let allocs = ALLOCS.load(Ordering::Relaxed) - before;
            assert_eq!(allocs, 0, "reads allocated while an old map was held");
        },
    );
}
