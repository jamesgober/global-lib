# Changelog

All notable changes to `global-lib` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

---

## [1.0.2] - 2026-11-12

### Fixed

- **Reads stay lock-free and allocation-free while a replaced value is
  still being read.** When one thread held a read (a `with` closure, for
  example) of a value that another thread then replaced, every other read
  of that `Global`, or every read by name in the process for a store,
  made a lock attempt, scanned every hazard record, and allocated, until
  that closure returned: measured at about 80 ns and one allocation per
  read instead of about 8 ns and none. Only a reader that was reading a
  replaced value now tries to free it when it finishes, and the scan
  checks the announcements in place without allocating. Measured on the
  same machine, same job, three rounds each (typed `Global<u64>`, 1,000,000
  reads while another thread holds a replaced value): 71.7 to 86.9 ns and
  1,000,000 allocations before, 7.0 to 9.1 ns and none after. By name:
  94 to 96 ns and one allocation per read before, 24 ns and none after,
  the same as with nothing retired. Writers also no longer allocate a list
  of every announcement on each write.

### Documentation

- A read that is still running when its value is replaced keeps the old
  snapshot alive (for a store, the old copy of the map) until it returns,
  and the reader that held it frees it, running its `Drop` there.
- `set` stores exactly the type of its value: `set("port", 7000)` stores
  an `i32`, which a `u16` read never finds. The docs now say to write
  `7000u16` or use a `Global<u16>`.

---

## [1.0.1] - 2026-10-26

### Fixed

- The documentation builds on docs.rs again. The crate root asked nightly
  rustdoc for the `doc_auto_cfg` feature, which nightly has since folded
  into `doc_cfg` and removed, so the docs.rs build of 1.0.0 failed
  (`E0557`). It now asks for `doc_cfg`. No code change.

---

## [1.0.0] - 2026-10-13

Initial stable release. The public API is frozen for 1.x under
[`docs/STABILITY-1.0.md`](docs/STABILITY-1.0.md).

### Added

- Free functions on one process-wide store: `set`, `define`, `get`,
  `try_get`, `value`, `with`, `edit`, `remove`, and `contains`, for values
  of any `Send + Sync + 'static` type addressed by name. `globals()`
  returns the store itself as `&'static Globals`.
- `Globals`, a store of named values of any type, with the same methods
  plus `type_of`, `len`, `is_empty`, `names` (sorted), and `clear`. Use
  your own for an isolated group of values or a fresh set per test.
- `Global<T>`, a typed value for a `static`, built with the `const fn`
  `Global::new()`. Methods: `set` (returns the previous value), `define`
  (set-once; `Err(value)` gives the value back), `get`, `value`, `with`,
  `edit` (returns `false` and does not call the closure when unset),
  `take`, and `is_set`.
- Semantics: `set` replaces a value of any type; `define` is set-once
  and keeps the existing value; a lookup under the wrong type returns
  `None` from `get`, `value`, and `with`, and `Error::TypeMismatch` naming
  both types from `try_get` and `edit`; `edit` runs its `FnMut` closure
  on a copy with no lock held and swaps the copy in only if the value has
  not changed meanwhile, starting again from the new value if it has, so
  concurrent edits never lose an update, readers never see a half-edited
  value, and the closure may use any global without risk of deadlock.
  Handles from `get` stay valid after the value is replaced or removed,
  and writing from inside a `with` closure on the same store does not
  deadlock. Replaced, removed, and rejected values are dropped outside the
  write lock.
- Lock-free, allocation-free reads through a crate-internal hazard-pointer
  cell (`src/hazard.rs`, `src/snapshot.rs`), the design used by
  `registry-io` 1.1. Writes are serialized and copy-on-write, and an old
  snapshot is freed by the writer or by the last reader holding it. Reads
  nested more than 8 levels deep on one thread fall back to a locked path.
- `Error` (`#[non_exhaustive]`, `Clone`, `PartialEq`, `Eq`): `Missing`,
  `TypeMismatch { name, expected, found }`, and `AlreadyDefined`, and the
  `Result<T>` alias. Names in errors are truncated to 64 characters. No
  error and no `Debug` output contains a stored value.
- Unit, integration, and concurrency tests; criterion benchmarks; CI for
  Linux, macOS, and Windows on stable and MSRV 1.85.

[Unreleased]: https://github.com/jamesgober/global-lib/compare/v1.0.2...HEAD
[1.0.2]: https://github.com/jamesgober/global-lib/compare/v1.0.1...v1.0.2
[1.0.1]: https://github.com/jamesgober/global-lib/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/jamesgober/global-lib/releases/tag/v1.0.0
