<h1 align="center">
    <b>global-lib</b><br>
    <sub><sup>PERFORMANCE</sup></sub>
</h1>
<div align="center">
    <sup>
        <a href="../README.md" title="Project Home"><b>HOME</b></a>
        <span>&nbsp;&mdash;&nbsp;</span>
        <a href="./API.md" title="API Reference"><b>API</b></a>
        <span>&nbsp;&mdash;&nbsp;</span>
        <a href="./GUIDELINES.md" title="Developer Guidelines"><b>GUIDELINES</b></a>
        <span>&nbsp;&mdash;&nbsp;</span>
        <a href="./STABILITY-1.0.md" title="Stability Contract"><b>STABILITY</b></a>
        <span>&nbsp;&mdash;&nbsp;</span>
        <span>PERFORMANCE</span>
    </sup>
</div>
<br>

> **Status:** Numbers below are the **1.0.0** baseline. Re-run the suite locally to compare on your hardware.

## Methodology

- **Harness:** [`criterion`](https://crates.io/crates/criterion) 0.5, `cargo bench --bench global_bench`.
- **Profile:** `release` with `lto = "fat"`, `codegen-units = 1`.
- **Machine:** Windows 11, x86_64, stable Rust. Each number is the criterion median of a 3-second measurement after a 1-second warm-up.
- **Input:** a `Globals` holding 32 `u64` values named `key_0` to `key_31`. Named reads and writes address `key_16`; the miss looks up `absent`. Typed benchmarks use a `static Global<u64>`.

## Results

### Reads

| Operation | Median |
|---|---|
| `Global::with`, typed | 5.3 ns |
| `Global::value`, typed | 5.5 ns |
| `Globals::with`, by name | 16.5 ns |
| `Globals::value`, by name, missing name | 15.9 ns |
| `Globals::get`, by name, returns an `Arc` | 28.3 ns |
| `Globals::value`, by name | 33.4 ns |

A typed read is the hazard-pointer protocol and nothing else: announce the current snapshot (one sequentially consistent store), check that it is still current (one load), borrow the value, and clear the announcement. No lock is taken, nothing is allocated, and `with` changes no reference count.

A read by name adds hashing the name and probing the map, which accounts for most of the difference between 5.3 and 16.5 ns. The map uses the standard library's SipHash hasher. A faster hasher would shave a few nanoseconds, but names may come from input, such as a configuration file or a request, and SipHash keeps crafted names from degrading lookups. A miss costs about the same as a hit, since both hash the name; it skips only the type check.

`get` adds an atomic reference-count increment for the returned `Arc` and the matching decrement when the benchmark drops it. `value` on a `u64` does the work of `with` plus a copy, yet measured 33.4 ns, consistently about twice `with`. The gap is larger than the copy accounts for and does not appear on the typed path, where `value` and `with` are within 0.2 ns. Where a read by name sits on a hot path, use `with`, or better, a `Global<T>`.

### Writes

| Operation | Median |
|---|---|
| `Globals::set`, 32 values in the store | 1.21 µs |
| `Globals::edit`, 32 values in the store | 1.80 µs |
| `Global::set`, typed | 120 ns |

Writes are serialized and copy-on-write. A write to a `Globals` copies the whole name-to-value map, allocating each key again and cloning each value's reference count, then swaps the copy in and frees the old map once no reader holds it. Its cost is therefore `O(n)` in the number of values in the store: about 1.2 µs with 32.

`edit` is a read followed by that write. It reads the current value under a hazard announcement, clones it (a `u64` here), runs the closure on the copy with no lock held, and then performs the copy-on-write swap, after checking under the write lock that the value has not changed meanwhile. The benchmark is single-threaded, so the edit never has to retry. Under contention on the same name, each retry repeats the read, the clone, and the closure.

A typed `set` has no map to copy. It allocates the new value and the new snapshot, swaps the snapshot in, and frees the old one: about 120 ns. Each write also scans the per-thread hazard records to decide whether the old snapshot can be freed; that scan grows with the peak number of threads that have read globals at the same time, not with the number of values.

This is the trade the crate makes on purpose. Globals are set at startup and changed occasionally; they are read constantly. Making every read lock-free and allocation-free costs writes a copy. A store that is written thousands of times per second, or that holds thousands of values and is written often, is better served by a `Global<T>` per hot value, or by an `Arc<Mutex<T>>` or an atomic stored once and changed through that.

### Contended reads

`Globals::with` by name on one value, read concurrently by several threads. The figure is the aggregate time per read: total elapsed time divided by the total number of reads across all threads.

| Threads | ns per read, aggregate |
|---|---|
| 1 | 25.7 |
| 4 | 4.7 |
| 16 | 2.8 |

Reads scale with threads because readers share nothing they write. Each thread announces its reads in its own hazard record, aligned to 128 bytes so that no two threads' records share a cache line; the snapshot pointer and the map are only read. There is no shared reference count and no lock for threads to contend on. Each sample includes spawning and joining the threads, which accounts for the single-thread figure being above the 16.5 ns of the uncontended benchmark.

## Running the Benchmarks

```bash
cargo bench --bench global_bench
```

## Interpreting the Numbers

For values read on hot paths, a `static Global<T>` read with `with` costs about 5 ns and adds no contention as threads are added. Reads by name cost about three times as much because of the hash, which is still small next to most work that reads a setting. Writes cost microseconds and grow with the size of the store, so set values during startup or when configuration changes, not in a loop.
