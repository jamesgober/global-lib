<h1 align="center">
    <b>global-lib</b><br>
    <sub><sup>DEVELOPER GUIDELINES</sup></sub>
</h1>
<div align="center">
    <sup>
        <a href="../README.md" title="Project Home"><b>HOME</b></a>
        <span>&nbsp;&mdash;&nbsp;</span>
        <a href="./API.md" title="API Reference"><b>API</b></a>
        <span>&nbsp;&mdash;&nbsp;</span>
        <a href="./PERFORMANCE.md" title="Performance"><b>PERFORMANCE</b></a>
        <span>&nbsp;&mdash;&nbsp;</span>
        <a href="./STABILITY-1.0.md" title="Stability Contract"><b>STABILITY</b></a>
        <span>&nbsp;&mdash;&nbsp;</span>
        <span>GUIDELINES</span>
    </sup>
</div>
<br>

This document describes the engineering standards this project is built to. [REPS](../REPS.md) is the governing standard; this page records how it applies here.

## Standards

- **MSRV:** Rust 1.85
- **Edition:** 2024
- **License:** Apache-2.0 OR MIT (dual)
- **Cross-platform:** Linux, macOS, Windows tested in CI; FreeBSD type-checked

## Quality Bar

- **Zero runtime dependencies.** `criterion` is a dev-dependency only.
- **`unsafe` confined to two modules.** The crate root denies `unsafe_code`; only the hazard-pointer domain (`src/hazard.rs`) and the snapshot cell (`src/snapshot.rs`) opt out. Every `unsafe` block carries a `SAFETY` comment that states the invariant it relies on.
- **Miri-checked concurrency core.** The hazard-pointer design is shared with `registry-io` 1.1. In this crate every unit test runs clean under Miri, and the concurrent ones across 16 thread schedules.
- **No panics in library code** &mdash; no `unwrap`, `expect`, `todo`, `unimplemented`, or `unreachable`.
- **Values stay out of output.** No error message and no `Debug` output may contain a stored value. Tests check this for `Globals` and `Global<T>`.
- **No lock around user code.** Closures passed to `with` and `edit` run with no lock held, so they may use any global. `edit` installs its result with a pointer comparison under the write lock and retries on conflict, rather than holding the lock while the closure runs.
- **Lock-free reads.** No read on the common path takes a lock or allocates. The locked fallback exists only for reads nested more than 8 levels deep on one thread and for reads during thread-local teardown.
- **No lost updates, no leaks.** Concurrency tests check that readers never see a torn value while writers replace and edit it, that concurrent edits all land, and, by counting live instances, that every replaced or removed value is dropped.
- **Benchmark-backed claims** &mdash; performance statements are verified by committed benchmarks.

## Contributing

1. `cargo fmt --all -- --check` passes.
2. `cargo clippy --all-targets --all-features -- -D warnings` is clean, also with `--target` set to a macOS and a Linux triple.
3. `cargo test --all-features` passes.
4. New public items include documentation and examples.
5. A change to `src/hazard.rs` or `src/snapshot.rs` is run under Miri before it is merged, with the concurrent tests across several thread schedules, for example `MIRIFLAGS="-Zmiri-disable-isolation" cargo +nightly miri test --lib` for all unit tests, then `MIRIFLAGS="-Zmiri-disable-isolation -Zmiri-many-seeds=0..16" cargo +nightly miri test --lib -- <filters>` with filters selecting the concurrent tests. Any change to an atomic ordering states in a comment why the ordering is sufficient.
6. Tests that use the process-wide store run in parallel with each other, so each one uses names of its own, as `tests/process.rs` does. Tests that need an empty store create a `Globals` instead.
7. A change to the public API is checked against the [stability contract](./STABILITY-1.0.md) before it is merged.
