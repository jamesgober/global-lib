<h1 align="center">
    <b>global-lib</b><br>
    <sub><sup>STABILITY CONTRACT 1.0</sup></sub>
</h1>
<div align="center">
    <sup>
        <a href="../README.md" title="Project Home"><b>HOME</b></a>
        <span>&nbsp;&mdash;&nbsp;</span>
        <a href="./API.md" title="API Reference"><b>API</b></a>
        <span>&nbsp;&mdash;&nbsp;</span>
        <a href="./PERFORMANCE.md" title="Performance"><b>PERFORMANCE</b></a>
        <span>&nbsp;&mdash;&nbsp;</span>
        <a href="./GUIDELINES.md" title="Developer Guidelines"><b>GUIDELINES</b></a>
        <span>&nbsp;&mdash;&nbsp;</span>
        <span>STABILITY</span>
    </sup>
</div>
<br>

This document is the contract `global-lib 1.0.0` ships under. It lists what is frozen for every 1.x release, what may still change in a minor or patch release, and how the contract itself changes.

> **Status:** In effect as of **1.0.0** and covers every 1.x release. Anything here that later needs to break requires a major version (2.0.0).

The signatures and behaviour of each item are documented in the [API reference](./API.md) and on [docs.rs](https://docs.rs/global-lib). Measured performance is in [PERFORMANCE.md](./PERFORMANCE.md).

---

## What 1.x Guarantees

### Public API surface

The items re-exported from the crate root are the complete public API. Their signatures and documented semantics do not change incompatibly within 1.x.

#### Free functions

Each operates on the process-wide store and behaves exactly as the `Globals` method of the same name.

| Item | Signature |
|---|---|
| `set` | `fn set<T: Any + Send + Sync>(name: &str, value: T)` |
| `define` | `fn define<T: Any + Send + Sync>(name: &str, value: T) -> Result<()>` |
| `get` | `fn get<T: Any + Send + Sync>(name: &str) -> Option<Arc<T>>` |
| `try_get` | `fn try_get<T: Any + Send + Sync>(name: &str) -> Result<Arc<T>>` |
| `value` | `fn value<T: Any + Send + Sync + Clone>(name: &str) -> Option<T>` |
| `with` | `fn with<T: Any + Send + Sync, R>(name: &str, f: impl FnOnce(&T) -> R) -> Option<R>` |
| `edit` | `fn edit<T: Any + Send + Sync + Clone>(name: &str, f: impl FnMut(&mut T)) -> Result<()>` |
| `remove` | `fn remove(name: &str) -> bool` |
| `contains` | `fn contains(name: &str) -> bool` |
| `globals` | `fn globals() -> &'static Globals` |

The order and number of explicit type parameters is part of the signature, so turbofish calls such as `with::<u16, _>` and `edit::<u16>` keep compiling.

#### Types

| Item | Kind | Notes |
|---|---|---|
| `Globals` | struct | `Debug + Default + Send + Sync`; `Debug` lists names and types, never values |
| `Global<T>` | struct | `Debug + Default`, and `Send + Sync` for `T: Send + Sync + 'static`; `Debug` shows the type and whether it is set, never the value |
| `Error` | enum | `#[non_exhaustive]`; `Missing { name }`, `TypeMismatch { name, expected, found }`, `AlreadyDefined { name }`; `Debug + Clone + PartialEq + Eq + Display + std::error::Error` |
| `Result<T>` | type | `std::result::Result<T, Error>` |

The fields of each `Error` variant and their types (`name: String`, `expected` and `found: &'static str`) are part of the contract.

#### `Globals` methods

| Method | Notes |
|---|---|
| `Globals::new()` | an empty store |
| `set(name, value)` | replaces any previous value, of any type |
| `define(name, value)` | set-once; `Error::AlreadyDefined` keeps the existing value |
| `get::<T>(name)`, `value::<T>(name)`, `with::<T, R>(name, f)` | `None` when missing or of another type |
| `try_get::<T>(name)` | `Error::Missing` or `Error::TypeMismatch` |
| `edit::<T>(name, f: impl FnMut(&mut T))` | `Error::Missing` or `Error::TypeMismatch`; nothing changed on error; `f` may run more than once |
| `remove(name)`, `contains(name)` | `bool` |
| `type_of(name)` | `Option<&'static str>` |
| `len()`, `is_empty()` | number of values |
| `names()` | `Vec<String>`, sorted |
| `clear()` | removes every value |

#### `Global<T>` methods

| Method | Notes |
|---|---|
| `const Global::new()` | unset; usable in a `static` |
| `set(value)` | returns the previous value as `Option<Arc<T>>` |
| `define(value)` | set-once; `Err(value)` gives the value back and keeps the existing one |
| `get()` | `Option<Arc<T>>` |
| `value()` | `Option<T>`, for `T: Clone` |
| `with(f)` | `Option<R>` |
| `edit(f: impl FnMut(&mut T)) -> bool` | for `T: Clone`; `false`, without calling `f`, when unset at the start; `false` if taken while `f` runs; `f` may run more than once |
| `take()` | removes and returns the value |
| `is_set()` | `bool` |

That `Global::new` is a `const fn` is part of the contract.

### Behavioural guarantees

- **`set` replaces.** `set` replaces the value of a name, whatever type it held before.
- **`define` is set-once.** When the name or `Global<T>` is already set, `define` changes nothing: `Globals::define` and the free function return `Error::AlreadyDefined`, and `Global::define` returns `Err(value)`. Of concurrent `define` calls for the same name, exactly one succeeds.
- **Exact types.** A value is found only under the type it was stored with. A different type makes `get`, `value`, and `with` return `None`, and `try_get` and `edit` return `Error::TypeMismatch` naming the requested and the stored type. A missing name gives `Error::Missing` from `try_get` and `edit`.
- **No lost updates.** `edit` clones the current value, runs the closure on the copy with no lock held, and installs the copy only if the value is still the one it started from, starting again from the new value otherwise. Concurrent `edit` calls on the same value each take effect, one after another. Because of the retry, the closure may run more than once and should only compute the new value. `edit` on a missing value, a value of another type, or an unset `Global<T>` does not call the closure and changes nothing; if the name is removed while the closure runs, `Globals::edit` returns `Error::Missing`.
- **No half-edited values.** Readers see a value as it was before a write or after it, never in between.
- **Handles stay valid.** An `Arc` returned by `get`, `try_get`, `Global::set`, or `Global::take` keeps its value alive and unchanged after the global is replaced or removed.
- **Lock-free reads.** `get`, `try_get`, `value`, `with`, `contains`, and `Global::is_set` take no lock on their common path and allocate nothing of their own (`value` clones the value, and a failed `try_get` builds an `Error`), and `with` changes no reference count. Readers never block writers or each other. The only exception is the documented fallback for reads nested deeper than the per-thread hazard slot count, or made during thread-local teardown.
- **No deadlock from user code.** Writers never wait for readers, and no lock is held while a `with` or `edit` closure runs, so calling `set`, `edit`, or any other operation from inside either closure, on any store or `Global<T>`, does not deadlock. Writing the name being edited from inside its own `edit` closure is unsupported: it makes every attempt conflict, and the edit keeps retrying.
- **Values never appear in output.** No `Error` and no `Debug` output contains a stored value. Errors name the global and the types involved; names in errors are truncated to 64 characters.
- **Sorted names.** `Globals::names` returns names in sorted order, and `Debug` for `Globals` lists entries sorted by name.
- **Process-wide state is bounded.** The crate keeps only the default store and one hazard record per concurrently reading thread; records are reused across threads and never grow beyond the peak number of threads reading at the same time.

### Dependencies and toolchain

| Property | Value |
|---|---|
| **Runtime dependencies** | None |
| **Edition** | 2024 |
| **MSRV** | Rust 1.85, frozen for 1.x |
| **License** | Apache-2.0 OR MIT |
| **Platforms** | Linux, macOS, and Windows, tested in CI; FreeBSD type-checked |

Raising the MSRV or adding a runtime dependency is a major-version change. The crate has no feature flags. Dev-dependencies may change freely.

---

## What Can Change in 1.x

### Additions in minor releases

- New methods on `Globals` and `Global<T>`, and new free functions, provided existing ones keep their signatures and semantics.
- New `Error` variants. `Error` is `#[non_exhaustive]`; match on it with a wildcard arm.
- New trait implementations on existing types.

### Not covered by the contract

- **Error message text.** The `Display` output of `Error` may be reworded. The variant and its fields are the stable part; match on those, not on strings.
- **Type name strings.** `TypeMismatch::expected`, `TypeMismatch::found`, and `type_of` report `std::any::type_name`, whose exact text Rust itself does not guarantee across compiler versions. Compare types with `get`, not by their names.
- **The `Debug` format** of `Globals` and `Global<T>`, except that it never includes values and lists `Globals` entries sorted by name.
- **Performance characteristics.** The numbers in [PERFORMANCE.md](./PERFORMANCE.md) are measurements, not guarantees, and may change in any release, as may the cost of writes relative to the size of a store, the hasher used for names, and the nesting depth at which reads fall back to the locked path. Reads stay lock-free on their common path in every 1.x release.
- **Internal structure:** module layout, private types, and the hazard-pointer implementation.

### Outside the 1.x scope

Change notifications and subscriptions (use `registry-io`), persistence (use `env-lib` or `config-lib`), scoped and thread-local values, and async-aware waiting for a value to be set are not part of 1.0. They may be added in a minor release only as new, opt-in items that leave the guarantees above unchanged.

---

## Versioning Policy

| Bump | Triggered by |
|---|---|
| Major | Any incompatible change to an item listed above; a change to `set`, `define`, `edit`, or type-matching semantics; weakening a behavioural guarantee, including lock-free reads; raising the MSRV; adding a runtime dependency |
| Minor | New items or `Error` variants; performance improvements |
| Patch | Bug fixes, documentation, internal changes with no behavioural effect |

### Deprecation

An item is deprecated with `#[deprecated(since = "1.Y.0", note = "...")]` in a minor release and removed no earlier than 2.0.0. The release notes describe the replacement.

---

## How the Contract Is Enforced

- **CI** runs `cargo fmt --check`, `cargo clippy -D warnings`, the full test suite, and `cargo doc` with `-D warnings` on Linux, macOS, and Windows, on stable and Rust 1.85.0, and type-checks FreeBSD.
- **Unit tests** pin each operation's results and errors, set-once `define`, type mismatches naming both types, handles surviving replacement and removal, writing from inside a read and from inside an edit without deadlock, edits retrying when the value changes underneath and reporting a removal, rejected `define` values being dropped, and that no `Debug` output contains a value.
- **Concurrency tests** check that readers never see a torn value while writers replace and edit it, that a typed global under contention only moves forward and loses no edit, and, by counting live instances, that every replaced or removed value is dropped and that dropping a store drops its values.
- **Miri** checks the unit tests, including the concurrent ones across many thread schedules, whenever the hazard-pointer core changes.
- **`cargo audit`** and **`cargo deny`** run on every push.

---

## Changing This Document

When an item on the frozen list needs to change:

1. The change is a major version. It does not land on a 1.x branch.
2. Open an issue that states what would change and why.
3. Update this document on the 2.0 branch.
4. Where possible, deprecate the old item in a 1.x minor release first.

<br>

<div align="center">
  <sup>global-lib 1.0.0 &mdash; COPYRIGHT <small>&copy;</small> 2026 <strong>JAMES GOBER.</strong> Apache-2.0 OR MIT.</sup>
</div>
