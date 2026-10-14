//! Concurrent readers and writers, and resource release.

#![allow(clippy::unwrap_used, missing_docs)]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;

use global_lib::{Global, Globals};

/// Counts live instances, so leaks show up as a non-zero count.
struct Tracked {
    values: [u64; 8],
    live: Arc<AtomicUsize>,
}

impl Tracked {
    fn new(v: u64, live: &Arc<AtomicUsize>) -> Self {
        let _ = live.fetch_add(1, Ordering::SeqCst);
        Self {
            values: [v; 8],
            live: Arc::clone(live),
        }
    }
}

impl Clone for Tracked {
    fn clone(&self) -> Self {
        Self::new(self.values[0], &self.live)
    }
}

impl Drop for Tracked {
    fn drop(&mut self) {
        let _ = self.live.fetch_sub(1, Ordering::SeqCst);
    }
}

#[test]
fn test_readers_never_see_torn_values_while_writers_replace() {
    let g = Arc::new(Globals::new());
    let live = Arc::new(AtomicUsize::new(0));
    g.set("v", Tracked::new(0, &live));
    let stop = Arc::new(AtomicBool::new(false));

    let readers: Vec<_> = (0..4)
        .map(|_| {
            let (g, stop) = (Arc::clone(&g), Arc::clone(&stop));
            thread::spawn(move || {
                let mut reads = 0u64;
                while !stop.load(Ordering::Relaxed) || reads == 0 {
                    let ok = g
                        .with::<Tracked, _>("v", |t| t.values.iter().all(|x| *x == t.values[0]))
                        .unwrap();
                    assert!(ok, "torn value");
                    reads += 1;
                }
                reads
            })
        })
        .collect();

    let writers: Vec<_> = (0..2)
        .map(|w| {
            let (g, live) = (Arc::clone(&g), Arc::clone(&live));
            thread::spawn(move || {
                for i in 0..500u64 {
                    if i % 2 == 0 {
                        g.set("v", Tracked::new(w * 1_000 + i, &live));
                    } else {
                        g.edit::<Tracked>("v", |t| t.values = [t.values[0] + 1; 8])
                            .unwrap();
                    }
                }
            })
        })
        .collect();
    for w in writers {
        w.join().unwrap();
    }
    stop.store(true, Ordering::Relaxed);
    for r in readers {
        assert!(r.join().unwrap() > 0);
    }

    assert!(g.remove("v"));
    assert_eq!(
        live.load(Ordering::SeqCst),
        0,
        "every replaced value was dropped"
    );
}

#[test]
fn test_typed_global_under_contention() {
    static HOT: Global<u64> = Global::new();
    let _previous = HOT.set(0);
    let stop = Arc::new(AtomicBool::new(false));
    let readers: Vec<_> = (0..4)
        .map(|_| {
            let stop = Arc::clone(&stop);
            thread::spawn(move || {
                let mut last = 0;
                while !stop.load(Ordering::Relaxed) {
                    let now = HOT.value().unwrap();
                    assert!(now >= last, "values only grow: {now} < {last}");
                    last = now;
                }
            })
        })
        .collect();
    for _ in 0..2_000 {
        assert!(HOT.edit(|v| *v += 1));
    }
    stop.store(true, Ordering::Relaxed);
    for r in readers {
        r.join().unwrap();
    }
    assert_eq!(HOT.value(), Some(2_000));
}

#[test]
fn test_dropping_a_store_drops_its_values() {
    let live = Arc::new(AtomicUsize::new(0));
    {
        let g = Globals::new();
        for i in 0..16 {
            g.set(&format!("k{i}"), Tracked::new(i, &live));
        }
        assert_eq!(live.load(Ordering::SeqCst), 16);
        g.clear();
        assert_eq!(live.load(Ordering::SeqCst), 0);
        g.set("again", Tracked::new(1, &live));
    }
    assert_eq!(live.load(Ordering::SeqCst), 0);
}

#[test]
fn test_take_releases_typed_value() {
    let live = Arc::new(AtomicUsize::new(0));
    let g: Global<Tracked> = Global::new();
    let _previous = g.set(Tracked::new(1, &live));
    let taken = g.take().unwrap();
    assert_eq!(live.load(Ordering::SeqCst), 1);
    drop(taken);
    assert_eq!(live.load(Ordering::SeqCst), 0);
}
