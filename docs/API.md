<h1 align="center">
    <img width="99" alt="Rust logo" src="https://raw.githubusercontent.com/jamesgober/rust-collection/72baabd71f00e14aa9184efcb16fa3deddda3a0a/assets/rust-logo.svg">
    <br><b>global-lib</b><br>
    <sub><sup>API REFERENCE</sup></sub>
</h1>
<div align="center">
    <sup>
        <a href="../README.md" title="Project Home"><b>HOME</b></a>
        <span>&nbsp;&mdash;&nbsp;</span>
        <span>API</span>
        <span>&nbsp;&mdash;&nbsp;</span>
        <a href="./PERFORMANCE.md" title="Performance"><b>PERFORMANCE</b></a>
        <span>&nbsp;&mdash;&nbsp;</span>
        <a href="./GUIDELINES.md" title="Developer Guidelines"><b>GUIDELINES</b></a>
        <span>&nbsp;&mdash;&nbsp;</span>
        <a href="./STABILITY-1.0.md" title="Stability Contract"><b>STABILITY</b></a>
    </sup>
</div>
<br>

> **Status:** This reference tracks the public surface of `global-lib` **1.0.1**. Everything documented here is frozen for 1.x under the [stability contract](./STABILITY-1.0.md).

## Table of Contents

- [Choosing an API](#choosing-an-api)
- [Free Functions](#free-functions)
  - [The process-wide store](#the-process-wide-store)
- [Globals](#globals)
  - [Creating](#creating)
  - [Writing](#writing)
  - [Reading](#reading)
  - [Editing](#editing)
  - [Inspecting](#inspecting)
- [Global&lt;T&gt;](#globalt)
- [Semantics](#semantics)
  - [Types](#types)
  - [Replacing, removing, and handles](#replacing-removing-and-handles)
  - [Concurrent writers](#concurrent-writers)
  - [Calling back into a store](#calling-back-into-a-store)
  - [Panics in closures](#panics-in-closures)
  - [When values are dropped](#when-values-are-dropped)
- [Errors](#errors)
- [Design Notes](#design-notes)
- [Not Included](#not-included)

<br>

## Choosing an API

| You want | Use |
|---|---|
| A few values shared across the program, set and read by name | The free functions: `global_lib::set`, `value`, `with`, ... |
| A value read on a hot path, with its type known at compile time | A `static` [`Global<T>`](#globalt) |
| A group of named values kept apart from the process-wide store, or a fresh set per test | Your own [`Globals`](#globals) |

All three use the same storage underneath and have the same read and write costs, except that a `Global<T>` does not hash a name. Each read is lock-free and allocation-free.

<br>

## Free Functions

The free functions operate on one process-wide [`Globals`](#globals), created on first use. Each is a shorthand for the method of the same name on `global_lib::globals()`.

| Function | Returns | Notes |
|---|---|---|
| `set(name, value)` | `()` | Replaces any previous value, of any type |
| `define(name, value)` | `Result<()>` | Set-once: `Error::AlreadyDefined` if `name` is set |
| `get::<T>(name)` | `Option<Arc<T>>` | `None` if unset or of another type |
| `try_get::<T>(name)` | `Result<Arc<T>>` | `Error::Missing` or `Error::TypeMismatch` |
| `value::<T>(name)` | `Option<T>` | A clone; needs `T: Clone` |
| `with::<T, R>(name, f)` | `Option<R>` | Runs `f(&T)` on the value in place; the fastest read |
| `edit::<T>(name, f)` | `Result<()>` | Runs `f(&mut T)` on a copy and swaps it in, retrying on conflict; needs `T: Clone` |
| `remove(name)` | `bool` | Whether `name` was set |
| `contains(name)` | `bool` | Whether `name` is set, to a value of any type |
| `globals()` | `&'static Globals` | The process-wide store itself |

Every value type must be `Send + Sync + 'static`. Names are `&str` and may be any text, including the empty string.

```rust
use global_lib::Error;

global_lib::set("port", 7000u16);
global_lib::define("app_name", "HiveDB")?;

assert_eq!(global_lib::value::<u16>("port"), Some(7000));
assert_eq!(global_lib::with::<u16, _>("port", |p| *p * 2), Some(14_000));
assert_eq!(*global_lib::get::<u16>("port").unwrap(), 7000);

global_lib::edit::<u16>("port", |p| *p += 1)?;
assert_eq!(global_lib::value::<u16>("port"), Some(7001));

assert!(matches!(
    global_lib::define("app_name", "Other"),
    Err(Error::AlreadyDefined { .. })
));

assert!(global_lib::remove("port"));
assert!(!global_lib::contains("port"));
# Ok::<(), global_lib::Error>(())
```

`with` and `get` take the value type as the first type parameter. `with` has a second one for the closure's result, which is usually left to inference: `with::<u16, _>`. When the result is bound with a type annotation, the turbofish on `value` and `get` can be dropped: `let port: Option<u16> = global_lib::value("port");`.

### The process-wide store

`globals()` returns the store the free functions use. It is created on first use and never dropped. Use it for the methods that have no free-function shorthand:

```rust
global_lib::set("api.workers", 8usize);

let store = global_lib::globals();
assert_eq!(store.type_of("api.workers"), Some("usize"));
assert!(store.names().contains(&"api.workers".to_owned()));
```

The process-wide store is shared by everything in the process, including libraries and tests that run in parallel. Give names a prefix of your own (`api.workers`) when several components use it, and avoid `clear` on it outside of a program's own shutdown logic.

<br>

## Globals

`Globals` is a set of named values of any `Send + Sync + 'static` type. It is `Send + Sync`, `Debug`, and `Default`.

### Creating

| Function | Does |
|---|---|
| `Globals::new()` | An empty set |
| `Globals::default()` | The same as `new` |

`Globals::new` is not a `const fn`, so a `Globals` in a `static` goes behind a `LazyLock` or `OnceLock`. To share one between threads without a `static`, wrap it in an `Arc`, or pass `&Globals` to scoped threads.

```rust
use std::sync::LazyLock;
use global_lib::Globals;

static CACHE_SETTINGS: LazyLock<Globals> = LazyLock::new(Globals::new);

CACHE_SETTINGS.set("capacity", 4096usize);
assert_eq!(CACHE_SETTINGS.value::<usize>("capacity"), Some(4096));
```

A `Globals` per test keeps tests from seeing each other's values, which the process-wide store cannot do when tests run in parallel:

```rust
let g = global_lib::Globals::new();
g.set("retries", 3u32);
assert_eq!(g.len(), 1);
```

Dropping a `Globals` drops the values it holds, except those still referenced by an `Arc` from `get` or `try_get`.

### Writing

| Method | Returns | Does |
|---|---|---|
| `set(name, value)` | `()` | Sets `name`, replacing any previous value of any type |
| `define(name, value)` | `Result<()>` | Sets `name` only if it is not set |
| `remove(name)` | `bool` | Removes `name`; returns whether it was set |
| `clear()` | `()` | Removes every value |

```rust
let g = global_lib::Globals::new();

g.set("mode", "fast");
g.set("mode", 2u8);                       // replaces the &str with a u8
assert_eq!(g.value::<u8>("mode"), Some(2));

g.define("build", "1.0.0")?;
assert!(g.define("build", "2.0.0").is_err());
assert_eq!(g.value::<&str>("build"), Some("1.0.0"));

assert!(g.remove("mode"));
assert!(!g.remove("mode"));
g.clear();
assert!(g.is_empty());
# Ok::<(), global_lib::Error>(())
```

`define` is for values that must not change after they are first set, such as a build identifier or a configuration chosen at startup. If `name` is already set, it fails with `Error::AlreadyDefined`, keeps the existing value, and drops the value it was given. Use `set` to replace a value deliberately.

### Reading

| Method | Returns | Missing or wrong type |
|---|---|---|
| `get::<T>(name)` | `Option<Arc<T>>` | `None` |
| `try_get::<T>(name)` | `Result<Arc<T>>` | `Error::Missing` or `Error::TypeMismatch` |
| `value::<T>(name)` | `Option<T>`, needs `T: Clone` | `None` |
| `with::<T, R>(name, f)` | `Option<R>` | `None`, and `f` is not called |
| `contains(name)` | `bool` | `false` when missing; any type counts |

```rust
use std::sync::Arc;

let g = global_lib::Globals::new();
g.set("hosts", vec![String::from("a.local"), String::from("b.local")]);

// In place: nothing is cloned and no reference count changes.
let count = g.with::<Vec<String>, _>("hosts", Vec::len);
assert_eq!(count, Some(2));

// A shared handle, cheap to keep or pass to another thread.
let hosts: Arc<Vec<String>> = g.get("hosts").unwrap();
assert_eq!(hosts[0], "a.local");

// A clone of the whole value.
let copy: Option<Vec<String>> = g.value("hosts");
assert_eq!(copy.map(|v| v.len()), Some(2));
```

`with` is the cheapest read. It borrows the value for the length of the closure, so keep the closure short: the snapshot it borrows stays alive until it returns, even if another thread replaces the value meanwhile. `get` adds one atomic reference-count increment and returns a handle that can be held as long as needed. `value` clones, which suits small `Copy` values and is the most convenient form for them.

`get`, `value`, and `with` return `None` both when `name` is not set and when it holds a different type. `try_get` returns the same handle as `get` but tells the two cases apart.

### Editing

| Method | Returns | Needs |
|---|---|---|
| `edit::<T>(name, f)` | `Result<()>` | `T: Clone`; `f: impl FnMut(&mut T)` |

```rust
let g = global_lib::Globals::new();
g.set("allowed", vec![22u16, 80]);

g.edit::<Vec<u16>>("allowed", |ports| ports.push(443))?;
assert_eq!(g.value::<Vec<u16>>("allowed"), Some(vec![22, 80, 443]));

assert!(g.edit::<u16>("allowed", |_| {}).is_err());   // wrong type, nothing changed
# Ok::<(), global_lib::Error>(())
```

`edit` reads the current value, clones it, and runs `f` on the clone with no lock held. It then takes the store's write lock and installs the clone, but only if `name` still holds the value it started from (the same allocation, compared by pointer). If another thread replaced or edited `name` in the meantime, `edit` discards its copy and starts again from the new value. Concurrent edits therefore never lose an update, and readers see the old value or the new one, never a partly edited one. Writes to other names in the same store do not cause a retry.

Because no lock is held while it runs, `f` may read or write any global, including other names in the same store, without risk of deadlock. For the same reason `f` is `FnMut` and may run more than once: it should only compute the new value from the one it is given, and leave side effects to the code after `edit` returns. Do not write `name` itself from inside its own `edit` closure. Every attempt then conflicts with the write it just made, and the edit retries indefinitely.

If `name` is not set, `edit` fails with `Error::Missing`; if it holds another type, with `Error::TypeMismatch`. In both cases `f` is not called and nothing changes. If `name` is removed while `f` runs, `edit` returns `Error::Missing` and nothing is installed; if it is replaced by a value of another type, `Error::TypeMismatch`.

The cost of an edit is one read, one clone of the value, and a write, plus one more of each for every retry. For a large value edited often, store an `Arc<Mutex<T>>` or an atomic instead and change it through that; the store then only hands out the shared handle.

### Inspecting

| Method | Returns |
|---|---|
| `type_of(name)` | `Option<&'static str>`, the stored type as given by `std::any::type_name` |
| `len()` | Number of values set |
| `is_empty()` | Whether no values are set |
| `names()` | `Vec<String>`, every name, sorted |

```rust
let g = global_lib::Globals::new();
g.set("token", String::from("s3cr3t"));
g.set("port", 7000u16);

assert_eq!(g.names(), ["port", "token"]);
assert_eq!(g.type_of("port"), Some("u16"));
assert_eq!(g.type_of("missing"), None);

let shown = format!("{g:?}");
assert!(shown.contains("token") && !shown.contains("s3cr3t"));
```

`Debug` lists names and their types, sorted by name, and never values.

<br>

## Global&lt;T&gt;

`Global<T>` is one typed value meant for a `static`. Its type is checked at compile time and there is no name to look up, so it is the choice for values read on hot paths. `Global::new` is a `const fn`.

```rust
use global_lib::Global;

static PORT: Global<u16> = Global::new();

assert!(!PORT.is_set());
assert_eq!(PORT.set(7000), None);
assert_eq!(PORT.with(|p| *p + 1), Some(7001));

assert!(PORT.edit(|p| *p += 1));
assert_eq!(PORT.value(), Some(7001));

assert_eq!(PORT.define(9000), Err(9000));       // already set: value handed back
assert_eq!(PORT.take().as_deref(), Some(&7001));
assert!(!PORT.edit(|p| *p += 1));                // unset: closure not called
```

| Method | Returns | Does |
|---|---|---|
| `Global::new()` | `Global<T>` | An unset global; `const`, usable in a `static` |
| `set(value)` | `Option<Arc<T>>` | Sets the value; returns the previous one |
| `define(value)` | `Result<(), T>` | Sets the value only if none is set; `Err(value)` gives it back |
| `get()` | `Option<Arc<T>>` | A shared handle |
| `value()` | `Option<T>` | A clone; needs `T: Clone` |
| `with(f)` | `Option<R>` | Runs `f(&T)` in place; the fastest read in the crate |
| `edit(f)` | `bool` | Runs `f(&mut T)` on a copy and swaps it in, retrying on conflict; `false`, without calling `f`, if unset; needs `T: Clone` |
| `take()` | `Option<Arc<T>>` | Removes the value and returns it |
| `is_set()` | `bool` | Whether a value is set |

`T` must be `Send + Sync + 'static`. `Global<T>` is then `Send + Sync`, so it can live in a `static`. It implements `Default` (unset) and `Debug`, which shows the type name and whether a value is set, never the value.

The semantics match `Globals`: `set` replaces, `define` is set-once, `edit` loses no update under concurrent edits, and handles from `get`, `set`, and `take` stay valid after the value changes. There is no type mismatch to report, so `define` returns the rejected value instead of an `Error`, and `edit` returns a `bool`. `Global::edit` works as described under [Editing](#editing): `f` runs on a copy with no lock held and may run more than once. It returns `false` if the global is unset when the edit starts, without calling `f`, or if it is taken while `f` runs.

<br>

## Semantics

### Types

A value is found only under the exact type it was stored with. `u16` and `u32` are different types, and so are `&'static str` and `String`:

```rust
let g = global_lib::Globals::new();
g.set("name", "HiveDB");                              // a &'static str

assert_eq!(g.value::<String>("name"), None);          // not a String
assert_eq!(g.value::<&str>("name"), Some("HiveDB"));

let err = g.try_get::<String>("name").unwrap_err();
assert_eq!(err.to_string(), r#"global "name" holds a &str, not a alloc::string::String"#);
```

When a lookup fails for this reason, `get`, `value`, and `with` return `None`, and `try_get` and `edit` return `Error::TypeMismatch` with the name, the requested type (`expected`), and the stored type (`found`). Both type names come from `std::any::type_name`. Use `try_get` while developing to find out which type a value was stored as, or `type_of` to ask directly.

Trait objects are stored behind a pointer type that is itself `Send + Sync`, and read back under that same type:

```rust
use std::sync::Arc;

type Hasher = Arc<dyn Fn(&str) -> u64 + Send + Sync>;

let g = global_lib::Globals::new();
g.set("hasher", Arc::new(|s: &str| s.len() as u64) as Hasher);

let hash = g.value::<Hasher>("hasher").unwrap();
assert_eq!(hash("abc"), 3);
```

### Replacing, removing, and handles

`set` replaces whatever `name` held, whatever its type. A handle obtained earlier from `get` or `try_get` is an ordinary `Arc` and keeps the value it pointed to alive after that value is replaced or removed. It never changes to the new value; call `get` again to see it.

```rust
let g = global_lib::Globals::new();
g.set("motd", String::from("old"));
let held = g.get::<String>("motd").unwrap();

g.set("motd", String::from("new"));
assert!(g.remove("motd"));

assert_eq!(held.as_str(), "old");
assert_eq!(g.get::<String>("motd"), None);
```

### Concurrent writers

Writes to one store, or to one `Global<T>`, are serialized. Each one starts from the latest value, so:

- concurrent `edit` calls each take effect, one after another, and no update is lost;
- of two `define` calls for the same name, exactly one succeeds;
- readers see each value either before or after a write, never in between.

Writes to different stores, or to different `Global<T>`s, do not wait for each other.

Reads never wait for writes, apart from the rare [fallback path](#design-notes) for deeply nested reads. A read that starts while a write is in progress sees the value from before it; a read that starts after the write returns sees the new value.

### Calling back into a store

Writers never wait for readers, and no lock is held while user code runs, so a `with` or `edit` closure may read or write any global, including the same store or `Global<T>`, without deadlocking. A `with` closure keeps seeing the value it was given; later reads see the change:

```rust
let g = global_lib::Globals::new();
g.set("a", 1u32);
g.set("b", 2u32);

let b = g.with::<u32, _>("a", |a| {
    g.edit::<u32>("b", |b| *b += *a).unwrap();
    g.value::<u32>("b")
});
assert_eq!(b, Some(Some(3)));
```

The one thing not to do is write the name being edited from inside its own `edit` closure. That does not deadlock, but every attempt conflicts with the write the closure itself made, so the edit retries indefinitely. Writing other names in the same store, or other stores, is fine.

### Panics in closures

A panic inside a `with` closure unwinds through the read, which releases its hazard announcement; the store stays usable. A panic inside an `edit` closure leaves the value unchanged, because the copy is installed only after the closure returns, and no lock is held while it runs; the store stays usable. No function in the crate panics on its own.

### When values are dropped

A replaced or removed value is dropped when the last reference to it goes away: the last snapshot that contains it, and every handle from `get`, `try_get`, `set`, or `take`. An old snapshot is freed as soon as no reader is using it, either by the writer that replaced it or by the last reader to finish with it. A value's `Drop` can therefore run on any thread that reads or writes the store, not only on the thread that replaced it.

Replaced, removed, and rejected values (the value a failed `define` was given, or the unused copy from an `edit` that had to retry) are all dropped outside the write lock, so their `Drop` implementations may use the store they were in.

<br>

## Errors

Fallible functions return `global_lib::Result<T>`, an alias for `std::result::Result<T, global_lib::Error>`. `Global::define` is the exception: it returns `Result<(), T>`, giving the value back.

| Variant | Returned by | Meaning |
|---|---|---|
| `Missing { name }` | `try_get`, `edit` | No global with this name is set |
| `TypeMismatch { name, expected, found }` | `try_get`, `edit` | The global holds a `found`, not the requested `expected` |
| `AlreadyDefined { name }` | `define` | The name is set; the existing value was kept |

```rust
use global_lib::{Error, Globals};

let g = Globals::new();
g.set("port", 7000u16);

match g.try_get::<u32>("port") {
    Err(Error::TypeMismatch { name, expected, found }) => {
        assert_eq!((name.as_str(), expected, found), ("port", "u32", "u16"));
    }
    other => panic!("unexpected: {other:?}"),
}
```

`Error` is `Debug`, `Clone`, `PartialEq`, `Eq`, `Display`, and `std::error::Error`. It is `#[non_exhaustive]`, so match it with a wildcard arm.

Errors name the global and the types involved, never the value: globals often hold configuration and credentials, and errors end up in logs. A name echoed in an error is cut to its first 64 characters, followed by `...`.

<br>

## Design Notes

**Reads.** Every store, and every `Global<T>`, holds its current snapshot behind an atomic pointer. A read announces that pointer in a per-thread hazard slot (one sequentially consistent store), checks that it is still current (one load), borrows the snapshot, and clears the announcement. There is no lock, no allocation, and, with `with`, no reference-count change. Each thread writes only to its own hazard record, which is aligned to 128 bytes so that no two threads share a cache line; readers therefore do not slow each other down, and aggregate read throughput grows with the number of threads.

**Writes.** Writers take a mutex per store, build a new snapshot from the current one, swap the pointer, and retire the old snapshot. For a `Globals`, the snapshot is the whole name-to-value map, so every write copies the map: `O(n)` in the number of values, about 1.2 µs per `set` with 32 values on the benchmark machine. This is the deliberate trade for values that are read far more often than written. Values themselves are reference-counted and are not cloned by the copy, except by `edit`, which clones the one value it changes.

**Edits are optimistic.** `edit` reads and clones the value without a lock, runs the closure, and installs the result with a pointer comparison under the write lock, retrying if the value changed in between. Holding the lock while user code runs would make any write from inside the closure a deadlock; the retry keeps updates from being lost without that risk. Under heavy contention on one name an edit may run its closure several times; for a counter written by many threads at once, an atomic stored once is cheaper.

**Reclamation.** A retired snapshot is freed once no hazard slot announces it: immediately by the writer when no reader holds it, otherwise by the last reader holding it, on its way out. Writers never wait for readers.

**Names are hashed with SipHash.** The map uses the standard library's default hasher, which resists collision attacks. Names may come from input, such as a configuration file or a request, and a faster hasher without that resistance would let crafted names degrade lookups. For values read in tight loops, a `Global<T>` avoids the hash entirely.

**Deep nesting falls back to a lock.** Each thread has 8 hazard slots, one per level of nested reads (a `with` closure that reads another global uses the next slot). A read nested deeper than 8 levels on one thread, or a read made while the thread's thread-local storage is being torn down, takes a slower path that clones the snapshot handle under the store's write lock. Results are the same; only the cost differs.

**Process-wide state.** The crate has two pieces of process-wide state: the default store behind the free functions, and the list of per-thread hazard records. A record is allocated the first time a thread reads a global, is never freed, and is reused by the next new thread after its owner exits. The list is therefore bounded by the peak number of threads that were reading globals at the same time.

**`unsafe` is confined.** The hazard-pointer domain (`src/hazard.rs`) and the snapshot cell (`src/snapshot.rs`) are the only modules allowed `unsafe` code, and every block carries a `SAFETY` comment. The crate root denies `unsafe_code` everywhere else. The design is the one used by `registry-io` 1.1 and is checked with Miri; see the [release notes](./release/v1.0.0.md#verification).

**Zero dependencies.** Everything is built on the standard library.

<br>

## Not Included

These are outside the 1.0 scope:

- **Change notifications.** Nothing calls you back when a global changes. [`registry-io`](https://crates.io/crates/registry-io) provides subscriptions.
- **Persistence.** Values live for the life of the process. Load them from, and save them to, a file with [`env-lib`](https://crates.io/crates/env-lib) or `config-lib`.
- **Scoped and thread-local values.** Every value is visible to every thread. Use `std::thread_local!` for per-thread values.
- **Async-aware waiting.** There is no way to wait until a global is set. Set required globals during startup, before the code that reads them runs.
