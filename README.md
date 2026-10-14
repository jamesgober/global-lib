<h1 align="center">
    <img width="99" alt="Rust logo" src="https://raw.githubusercontent.com/jamesgober/rust-collection/72baabd71f00e14aa9184efcb16fa3deddda3a0a/assets/rust-logo.svg">
    <br><b>global-lib</b>
    <br><sub><sup>PROCESS-WIDE VALUES FOR RUST</sup></sub>
</h1>

<p align="center">
    <a href="https://crates.io/crates/global-lib"><img src="https://img.shields.io/crates/v/global-lib.svg" alt="Crates.io"></a>
    <a href="https://crates.io/crates/global-lib" alt="Download"><img alt="Crates.io Downloads" src="https://img.shields.io/crates/d/global-lib?color=%230099ff"></a>
    <a href="https://docs.rs/global-lib"><img src="https://docs.rs/global-lib/badge.svg" alt="Documentation"></a>
    <a href="https://github.com/jamesgober/global-lib/actions"><img alt="GitHub CI" src="https://github.com/jamesgober/global-lib/actions/workflows/ci.yml/badge.svg"></a>
    <a href="https://github.com/rust-lang/rfcs/blob/master/text/2495-min-rust-version.md" title="MSRV"><img alt="MSRV" src="https://img.shields.io/badge/MSRV-1.85%2B-blue"></a>
</p>

<p align="center">
    <b>Set It Once, Read It Anywhere, Change It Without Losing an Update</b>
    <br>
    <i>Globals by name or through typed statics, with lock-free, allocation-free reads. Zero dependencies.</i>
</p>

<p>
    <strong>global-lib</strong> holds the values a whole program shares: settings decided at startup, handles to shared services, counters, switches. Set a value from anywhere, read it from any thread, change it later, or remove it. There are two ways in. By name, any <code>Send + Sync + 'static</code> type goes into one process-wide store through a handful of free functions such as <code>global_lib::set("port", 7000u16)</code>. Typed, a <code>static PORT: Global&lt;u16&gt; = Global::new();</code> is checked at compile time and skips the name lookup.
</p>

<p>
    Globals are read far more often than they are written, and the design follows from that. A read announces which snapshot it is using through a per-thread hazard pointer, borrows the value in place, and releases it: no lock, no allocation, and with <code>with</code> no reference-count change, so readers never block writers or each other. Writes are serialized and copy-on-write: they build a new snapshot, swap it in, and free the old one once no reader holds it. A typed read takes about 5 ns, a read by name about 16 ns.
</p>

<p>
    The semantics are strict where mistakes are easy. <code>define</code> is set-once and fails with <code>AlreadyDefined</code> instead of replacing a value. Asking for a different type than the one stored returns <code>None</code>, or an error that names both types. <code>edit</code> changes a copy and swaps it in only if no other write got there first, starting again if one did, so concurrent edits never lose an update, readers never see a half-edited value, and the closure can use any global without risk of deadlock. Values never appear in <code>Debug</code> output or in errors, since globals often hold configuration and credentials, and errors end up in logs.
</p>

<p>
    The API is stable. Version 1.0 freezes the public items and the guarantees above for every 1.x release; the terms are in the <a href="docs/STABILITY-1.0.md">stability contract</a>.
</p>

## Features

### Globals by Name
- **Any type** &mdash; anything `Send + Sync + 'static`: numbers, strings, collections, `Arc<dyn Trait + Send + Sync>`
- **`set`, `define`, `remove`, `contains`** &mdash; `set` replaces a value of any type; `define` is set-once
- **`get`, `try_get`, `value`, `with`** &mdash; a shared handle, the same with the reason for a miss, a clone, or a closure over the value in place
- **`edit`** &mdash; changes a value through a copy; concurrent edits never lose an update
- **Your own stores** &mdash; `Globals::new()` for an isolated set of values, or a fresh one per test, with `names`, `len`, `type_of`, and `clear` on top

### Typed Statics
- **`static X: Global<T> = Global::new();`** &mdash; type checked at compile time, no name to hash
- **`set`, `define`, `take`** &mdash; `set` returns the previous value; `define` gives your value back if one is already set
- **`get`, `value`, `with`, `is_set`** &mdash; `with` is the fastest read in the crate, about 5 ns
- **`edit`** &mdash; returns `false` without calling the closure when nothing is set

### Reads and Writes
- **Lock-free, allocation-free reads** &mdash; a hazard-pointer announcement, a borrow, a release
- **Readers share nothing they write** &mdash; aggregate read throughput grows with the number of threads
- **Copy-on-write writes** &mdash; serialized, never waiting for readers, and no lock is held while your closures run, so writing from inside a read or an edit cannot deadlock
- **Handles stay valid** &mdash; an `Arc` from `get` keeps its value alive after it is replaced or removed

### Secrets
- **Never printed** &mdash; `Debug` lists names and types; errors name the global and the types involved, never the value
- **Bounded error text** &mdash; names echoed in errors are cut to 64 characters

<hr>

## Installation

```toml
[dependencies]
global-lib = "1.0"
```

<hr>

## Quick Start

```rust
use global_lib::Global;

// By name: any Send + Sync + 'static type, in one process-wide store.
global_lib::set("port", 7000u16);
global_lib::define("app_name", "HiveDB")?;           // set-once

let port: u16 = global_lib::value("port").unwrap_or(8080);
global_lib::edit::<u16>("port", |p| *p += 1)?;
let len = global_lib::with::<&str, _>("app_name", |name| name.len());

// Typed: checked at compile time, no name lookup.
static MAX_CONNECTIONS: Global<usize> = Global::new();

MAX_CONNECTIONS.set(64);
let max = MAX_CONNECTIONS.with(|n| *n).unwrap_or(16);
# Ok::<(), global_lib::Error>(())
```

### Wrong Types and Missing Values

```rust
use global_lib::Error;

global_lib::set("workers", 8u16);

// The quiet readers return None for a wrong type or a missing name.
assert_eq!(global_lib::value::<u32>("workers"), None);

// try_get and edit say why.
let err = global_lib::try_get::<u32>("workers").unwrap_err();
assert_eq!(err.to_string(), r#"global "workers" holds a u16, not a u32"#);
assert!(matches!(global_lib::try_get::<u16>("threads"), Err(Error::Missing { .. })));
```

### A Store of Your Own

```rust
use std::sync::LazyLock;
use global_lib::Globals;

static PLUGINS: LazyLock<Globals> = LazyLock::new(Globals::new);

PLUGINS.set("compression", String::from("zstd"));
PLUGINS.set("checksum", String::from("crc32c"));
assert_eq!(PLUGINS.names(), ["checksum", "compression"]);
```

The free functions use one process-wide `Globals`, returned by `global_lib::globals()`. A `Globals` of your own keeps a group of values apart from it, and a `Globals::new()` per test keeps tests from seeing each other's values.

<hr>

## Documentation

- [API reference](docs/API.md) &mdash; every public item, the semantics of each operation, and the cost model.
- [Stability contract](docs/STABILITY-1.0.md) &mdash; what 1.x guarantees and what may still change.
- [Performance](docs/PERFORMANCE.md) &mdash; benchmark methodology and results.
- [Developer guidelines](docs/GUIDELINES.md) &mdash; the engineering bar this project is built to.
- [Release notes](docs/release/) &mdash; what shipped, when, and why.

<br>

## Contributing

Contributions are welcome under the project's dual license. Before opening a pull request, please make sure:

1. `cargo fmt --all -- --check` passes.
2. `cargo clippy --all-targets --all-features -- -D warnings` is clean.
3. `cargo test --all-features` passes.
4. New public items include documentation and at least one example.
5. The [API reference](docs/API.md) and [CHANGELOG](CHANGELOG.md) are updated alongside code changes.

<br>

<!-- LICENSE
############################################# -->
<div id="license">
    <h2>License</h2>
    <p><b>global-lib</b> is <b>dual-licensed</b> under either of:</p>
    <ul>
        <li><b>Apache License, Version 2.0</b> &mdash; see <a href="./LICENSE-APACHE" title="Apache-2.0 License">LICENSE-APACHE</a> or <a href="http://www.apache.org/licenses/LICENSE-2.0" title="Apache-2.0 License" target="_blank">apache.org/licenses/LICENSE-2.0</a></li>
        <li><b>MIT License</b> &mdash; see <a href="./LICENSE-MIT" title="MIT License">LICENSE-MIT</a> or <a href="https://opensource.org/licenses/MIT" title="MIT License" target="_blank">opensource.org/licenses/MIT</a></li>
    </ul>
    <p>at your option.</p>
    <br>
    <h3>Contribution</h3>
    <p>Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in <b>global-lib</b> by you, as defined in the <b>Apache-2.0 license</b>, shall be <b>dual-licensed</b> as above, without any additional terms or conditions.</p>
    <p>Unless required by applicable law or agreed to in writing, software distributed under the Licenses is distributed on an <b>"AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND</b>, either express or implied.</p>
</div>

<!-- FOOT COPYRIGHT
################################################# -->
<div align="center">
  <h2></h2>
  <sup>COPYRIGHT <small>&copy;</small> 2026 <strong>JAMES GOBER.</strong></sup>
</div>
