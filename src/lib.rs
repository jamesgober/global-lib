//! # global-lib
//!
//! PROCESS-WIDE VALUES FOR RUST
//!
//! Set a value once and read it from anywhere in the program, edit it
//! later, or remove it: by name through a few free functions, or through a
//! typed [`Global<T>`] in a `static`. Reads are lock-free and allocate
//! nothing, so globals can sit on hot paths read by many threads at once.
//!
//! # By name
//!
//! ```
//! global_lib::set("port", 7000u16);
//! global_lib::define("app_name", "HiveDB")?;       // set-once
//!
//! let port: Option<u16> = global_lib::value("port");
//! global_lib::edit::<u16>("port", |p| *p += 1)?;
//! let doubled = global_lib::with::<u16, _>("port", |p| *p * 2);
//!
//! assert!(global_lib::get::<u32>("port").is_none());   // wrong type
//! global_lib::remove("port");
//! # assert_eq!(port, Some(7000));
//! # assert_eq!(doubled, Some(14002));
//! # Ok::<(), global_lib::Error>(())
//! ```
//!
//! Any `Send + Sync + 'static` type can be stored. Asking for a different
//! type than the one stored returns `None` from [`get`], [`value`], and
//! [`with`], and an [`Error::TypeMismatch`] naming both types from
//! [`try_get`] and [`edit`].
//!
//! # Typed statics
//!
//! ```
//! use global_lib::Global;
//!
//! static MAX_CONNECTIONS: Global<usize> = Global::new();
//!
//! MAX_CONNECTIONS.set(64);
//! let n = MAX_CONNECTIONS.with(|n| *n).unwrap_or(16);
//! # assert_eq!(n, 64);
//! ```
//!
//! # Cost model
//!
//! - **Reads** publish a hazard pointer for the current snapshot, borrow
//!   it, and release it: no lock, no allocation, and with [`with`] no
//!   reference-count change. Readers never block writers or each other.
//! - **Writes** are serialized and copy-on-write: the map (or the single
//!   value) is copied, changed, and swapped in. The old snapshot is freed
//!   once no reader holds it. Globals are read far more often than written,
//!   which is the trade this design makes.
//! - [`edit`] runs its closure on a copy with no lock held, then swaps
//!   the copy in only if the value has not changed meanwhile, starting
//!   again from the new value if it has. Concurrent edits never lose an
//!   update, and the closure may use any global without risk of deadlock.
//!
//! # Global state
//!
//! The free functions use one process-wide [`Globals`] ([`globals`]),
//! created on first use. For isolated groups of values, or a fresh set per
//! test, create your own [`Globals`].
//!
//! # Secrets
//!
//! `Debug` output and errors name globals and their types, never their
//! values.

#![deny(warnings)]
#![deny(missing_docs)]
#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(unused_must_use)]
#![deny(unused_results)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::todo)]
#![deny(clippy::unimplemented)]
#![deny(clippy::print_stdout)]
#![deny(clippy::print_stderr)]
#![deny(clippy::dbg_macro)]
#![deny(clippy::unreachable)]
#![deny(clippy::undocumented_unsafe_blocks)]
#![deny(clippy::missing_safety_doc)]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

use std::any::Any;
use std::sync::Arc;

mod error;
mod global;
#[allow(unsafe_code)]
mod hazard;
#[allow(unsafe_code)]
mod snapshot;
mod store;

pub use error::{Error, Result};
pub use global::Global;
pub use store::{Globals, globals};

/// Sets the process-wide global `name` to `value`, replacing any previous
/// value of any type. See [`Globals::set`].
pub fn set<T: Any + Send + Sync>(name: &str, value: T) {
    globals().set(name, value);
}

/// Sets the process-wide global `name` only if it is not set yet.
///
/// # Errors
///
/// [`Error::AlreadyDefined`] if it is set. See [`Globals::define`].
pub fn define<T: Any + Send + Sync>(name: &str, value: T) -> Result<()> {
    globals().define(name, value)
}

/// A shared handle to the process-wide global `name`, or `None` if it is
/// unset or holds another type. See [`Globals::get`].
#[must_use]
pub fn get<T: Any + Send + Sync>(name: &str) -> Option<Arc<T>> {
    globals().get(name)
}

/// A shared handle to the process-wide global `name`.
///
/// # Errors
///
/// [`Error::Missing`] or [`Error::TypeMismatch`]. See
/// [`Globals::try_get`].
pub fn try_get<T: Any + Send + Sync>(name: &str) -> Result<Arc<T>> {
    globals().try_get(name)
}

/// A clone of the process-wide global `name`, or `None` if it is unset or
/// holds another type. See [`Globals::value`].
#[must_use]
pub fn value<T: Any + Send + Sync + Clone>(name: &str) -> Option<T> {
    globals().value(name)
}

/// Runs `f` on the process-wide global `name` in place. See
/// [`Globals::with`].
pub fn with<T: Any + Send + Sync, R>(name: &str, f: impl FnOnce(&T) -> R) -> Option<R> {
    globals().with(name, f)
}

/// Changes the process-wide global `name` in place.
///
/// # Errors
///
/// [`Error::Missing`] or [`Error::TypeMismatch`]. See [`Globals::edit`].
pub fn edit<T: Any + Send + Sync + Clone>(name: &str, f: impl FnMut(&mut T)) -> Result<()> {
    globals().edit(name, f)
}

/// Removes the process-wide global `name`. Returns whether it was set.
// Called for its effect far more often than for the result.
#[allow(clippy::must_use_candidate)]
pub fn remove(name: &str) -> bool {
    globals().remove(name)
}

/// Whether the process-wide global `name` is set.
#[must_use]
pub fn contains(name: &str) -> bool {
    globals().contains(name)
}
