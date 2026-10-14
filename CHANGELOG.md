# Changelog

All notable changes to `global-lib` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

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

[Unreleased]: https://github.com/jamesgober/global-lib/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/jamesgober/global-lib/releases/tag/v1.0.0
