//! [`Globals`]: values of any type, addressed by name.

use std::any::{Any, type_name};
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, OnceLock};

use crate::error::{Error, Result};
use crate::snapshot::SnapCell;

/// One stored value and the name of its type, for error messages and
/// [`Globals::type_of`].
#[derive(Clone)]
struct Slot {
    value: Arc<dyn Any + Send + Sync>,
    type_name: &'static str,
}

impl Slot {
    fn new<T: Any + Send + Sync>(value: T) -> Self {
        Self {
            value: Arc::new(value),
            type_name: type_name::<T>(),
        }
    }
}

type Map = HashMap<Box<str>, Slot>;

/// A set of named values of any `Send + Sync` type.
///
/// Reads are lock-free and allocation-free: they borrow a snapshot of the
/// map protected by a hazard pointer, so any number of threads can read
/// at once without contending. Writes copy the map and swap the copy in,
/// which makes them `O(n)` in the number of values; globals are read far
/// more often than they are written, which is the trade this makes.
///
/// The free functions of this crate ([`set`](crate::set),
/// [`get`](crate::get), ...) use one process-wide `Globals`
/// ([`globals`](crate::globals)). Create your own for isolated groups of
/// values, or to give each test a fresh set.
///
/// `Debug` lists names and types, never values.
///
/// # Examples
///
/// ```
/// use global_lib::Globals;
///
/// let g = Globals::new();
/// g.set("port", 7000u16);
/// g.define("app_name", String::from("HiveDB"))?;
///
/// assert_eq!(g.value::<u16>("port"), Some(7000));
/// g.edit::<u16>("port", |p| *p += 1)?;
/// assert_eq!(g.with::<u16, _>("port", |p| *p), Some(7001));
///
/// assert!(g.get::<u32>("port").is_none());       // wrong type
/// assert!(g.try_get::<u32>("port").is_err());     // ...and why
/// # Ok::<(), global_lib::Error>(())
/// ```
pub struct Globals {
    map: SnapCell<Map>,
}

impl Globals {
    /// An empty set.
    #[must_use]
    pub fn new() -> Self {
        Self {
            map: SnapCell::new(HashMap::new()),
        }
    }

    /// Sets `name` to `value`, replacing any previous value of any type.
    pub fn set<T: Any + Send + Sync>(&self, name: &str, value: T) {
        let slot = Slot::new(value);
        let _unit = self.map.update(|map| {
            let mut next = map.clone();
            let _previous = next.insert(name.into(), slot);
            Some((next, ()))
        });
    }

    /// Sets `name` to `value` only if it is not already set: a set-once
    /// definition, for values that must not change after startup.
    ///
    /// # Errors
    ///
    /// [`Error::AlreadyDefined`] if `name` is set. The existing value is
    /// kept and `value` is dropped.
    pub fn define<T: Any + Send + Sync>(&self, name: &str, value: T) -> Result<()> {
        // Taken only when inserted, so a rejected value is dropped here,
        // after the write lock is released, not inside it.
        let mut slot = Some(Slot::new(value));
        let inserted = self.map.update(|map| {
            if map.contains_key(name) {
                return None;
            }
            let mut next = map.clone();
            let _previous = next.insert(name.into(), slot.take()?);
            Some((next, ()))
        });
        drop(slot);
        inserted.ok_or_else(|| Error::already_defined(name))
    }

    /// A shared handle to the value of `name`, or `None` if it is not set
    /// or holds a different type. Use [`try_get`](Globals::try_get) to
    /// tell those apart.
    #[must_use]
    pub fn get<T: Any + Send + Sync>(&self, name: &str) -> Option<Arc<T>> {
        self.try_get(name).ok()
    }

    /// A shared handle to the value of `name`.
    ///
    /// # Errors
    ///
    /// [`Error::Missing`] if `name` is not set, or [`Error::TypeMismatch`]
    /// (naming both types) if it holds a different type.
    pub fn try_get<T: Any + Send + Sync>(&self, name: &str) -> Result<Arc<T>> {
        self.map.read(|map| {
            let slot = map.get(name).ok_or_else(|| Error::missing(name))?;
            Arc::clone(&slot.value)
                .downcast::<T>()
                .map_err(|_| Error::mismatch(name, type_name::<T>(), slot.type_name))
        })
    }

    /// A clone of the value of `name`, or `None` if it is not set or holds
    /// a different type. Convenient for small `Copy` and `Clone` values.
    #[must_use]
    pub fn value<T: Any + Send + Sync + Clone>(&self, name: &str) -> Option<T> {
        self.with(name, T::clone)
    }

    /// Runs `f` on the value of `name` in place and returns its result,
    /// or `None` if it is not set or holds a different type.
    ///
    /// This is the fastest read: no reference count changes and nothing
    /// is cloned. Keep `f` short; the value it sees stays alive until it
    /// returns, even if another thread replaces it meanwhile.
    pub fn with<T: Any + Send + Sync, R>(&self, name: &str, f: impl FnOnce(&T) -> R) -> Option<R> {
        self.map.read(|map| {
            map.get(name)
                .and_then(|slot| slot.value.downcast_ref::<T>())
                .map(f)
        })
    }

    /// Changes the value of `name`: `f` edits a copy, and the copy replaces
    /// the value. Readers see the old value or the new one, never a partly
    /// edited one.
    ///
    /// No lock is held while `f` runs, so `f` may read or write any global,
    /// including this store, without risk of deadlock. If another thread
    /// changes `name` while `f` runs, the edit starts again from the new
    /// value, so concurrent edits never lose an update; `f` can therefore
    /// run more than once and should only compute the new value. (Writing
    /// `name` itself from inside `f` makes every attempt conflict, so the
    /// edit keeps retrying; do not do that.)
    ///
    /// # Errors
    ///
    /// [`Error::Missing`] or [`Error::TypeMismatch`], as for
    /// [`try_get`](Globals::try_get). Nothing is changed.
    pub fn edit<T: Any + Send + Sync + Clone>(
        &self,
        name: &str,
        mut f: impl FnMut(&mut T),
    ) -> Result<()> {
        loop {
            let (base, type_name_found) = self
                .map
                .read(|map| {
                    map.get(name)
                        .map(|slot| (Arc::clone(&slot.value), slot.type_name))
                })
                .ok_or_else(|| Error::missing(name))?;
            let Some(current) = base.downcast_ref::<T>() else {
                return Err(Error::mismatch(name, type_name::<T>(), type_name_found));
            };
            let mut edited = current.clone();
            f(&mut edited);
            let mut slot = Some(Slot::new(edited));
            let swapped = self.map.update(|map| {
                let unchanged = map
                    .get(name)
                    .is_some_and(|now| Arc::ptr_eq(&now.value, &base));
                if !unchanged {
                    return None;
                }
                let mut next = map.clone();
                let _previous = next.insert(name.into(), slot.take()?);
                Some((next, ()))
            });
            if swapped.is_some() {
                return Ok(());
            }
            // Someone changed or removed `name` meanwhile: start again from
            // whatever it holds now. The unused copy drops here, outside the
            // lock.
            drop(slot);
        }
    }

    /// Removes `name`. Returns whether it was set.
    pub fn remove(&self, name: &str) -> bool {
        self.map
            .update(|map| {
                if !map.contains_key(name) {
                    return None;
                }
                let mut next = map.clone();
                let _removed = next.remove(name);
                Some((next, ()))
            })
            .is_some()
    }

    /// Whether `name` is set, to a value of any type.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.map.read(|map| map.contains_key(name))
    }

    /// The type `name` holds, as reported by [`std::any::type_name`].
    #[must_use]
    pub fn type_of(&self, name: &str) -> Option<&'static str> {
        self.map
            .read(|map| map.get(name).map(|slot| slot.type_name))
    }

    /// Number of values set.
    #[must_use]
    pub fn len(&self) -> usize {
        self.map.read(HashMap::len)
    }

    /// Whether no values are set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.map.read(HashMap::is_empty)
    }

    /// The names of all values, sorted.
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .map
            .read(|map| map.keys().map(ToString::to_string).collect());
        names.sort_unstable();
        names
    }

    /// Removes every value.
    pub fn clear(&self) {
        let _unit = self
            .map
            .update(|map| (!map.is_empty()).then(|| (HashMap::new(), ())));
    }
}

impl Default for Globals {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Globals {
    /// Names and types, sorted by name. Values are never shown.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut entries: Vec<(String, &'static str)> = self.map.read(|map| {
            map.iter()
                .map(|(k, slot)| (k.to_string(), slot.type_name))
                .collect()
        });
        entries.sort_unstable();
        f.debug_map().entries(entries).finish()
    }
}

/// The process-wide [`Globals`] behind this crate's free functions.
///
/// Created on first use and never dropped. This is the crate's purpose,
/// and its only process-wide state besides the hazard-pointer records.
#[must_use]
pub fn globals() -> &'static Globals {
    static GLOBALS: OnceLock<Globals> = OnceLock::new();
    GLOBALS.get_or_init(Globals::new)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_set_get_value_with() {
        let g = Globals::new();
        g.set("n", 5u32);
        assert_eq!(*g.get::<u32>("n").unwrap(), 5);
        assert_eq!(g.value::<u32>("n"), Some(5));
        assert_eq!(g.with::<u32, _>("n", |n| n * 2), Some(10));
        assert_eq!(g.get::<u32>("missing"), None);
    }

    #[test]
    fn test_set_replaces_any_type() {
        let g = Globals::new();
        g.set("x", 1u8);
        g.set("x", "now a str");
        assert_eq!(g.value::<&str>("x"), Some("now a str"));
        assert_eq!(g.get::<u8>("x"), None);
    }

    #[test]
    fn test_try_get_explains_failures() {
        let g = Globals::new();
        g.set("port", 7000u16);
        assert_eq!(
            g.try_get::<u16>("nope").unwrap_err(),
            Error::missing("nope")
        );
        assert_eq!(
            g.try_get::<u32>("port").unwrap_err(),
            Error::mismatch("port", "u32", "u16")
        );
    }

    #[test]
    fn test_define_is_set_once() {
        let g = Globals::new();
        g.define("app", "HiveDB").unwrap();
        assert_eq!(
            g.define("app", "Other").unwrap_err(),
            Error::already_defined("app")
        );
        assert_eq!(g.value::<&str>("app"), Some("HiveDB"));
    }

    #[test]
    fn test_edit_changes_value_or_explains() {
        let g = Globals::new();
        g.set("list", vec![1, 2]);
        g.edit::<Vec<i32>>("list", |v| v.push(3)).unwrap();
        assert_eq!(g.value::<Vec<i32>>("list"), Some(vec![1, 2, 3]));
        assert_eq!(
            g.edit::<u8>("none", |_| {}).unwrap_err(),
            Error::missing("none")
        );
        assert!(matches!(
            g.edit::<u8>("list", |_| {}),
            Err(Error::TypeMismatch { .. })
        ));
        assert_eq!(g.value::<Vec<i32>>("list"), Some(vec![1, 2, 3]));
    }

    #[test]
    fn test_remove_contains_len_names_clear_type_of() {
        let g = Globals::new();
        assert!(g.is_empty());
        g.set("b", 1u8);
        g.set("a", String::from("x"));
        assert_eq!(g.len(), 2);
        assert_eq!(g.names(), ["a", "b"]);
        assert_eq!(g.type_of("a"), Some(type_name::<String>()));
        assert!(g.contains("b"));
        assert!(g.remove("b"));
        assert!(!g.remove("b"));
        g.clear();
        assert!(g.is_empty());
        g.clear();
    }

    #[test]
    fn test_debug_shows_names_and_types_not_values() {
        let g = Globals::new();
        g.set("token", String::from("s3cr3t"));
        let shown = format!("{g:?}");
        assert!(shown.contains("token"));
        assert!(shown.contains("String"));
        assert!(!shown.contains("s3cr3t"));
    }

    #[test]
    fn test_handles_survive_replacement_and_removal() {
        let g = Globals::new();
        g.set("v", String::from("old"));
        let held = g.get::<String>("v").unwrap();
        g.set("v", String::from("new"));
        assert!(g.remove("v"));
        assert_eq!(held.as_str(), "old");
    }

    #[test]
    fn test_reentrant_edit_from_inside_with_does_not_deadlock() {
        let g = Globals::new();
        g.set("a", 1u32);
        g.set("b", 2u32);
        let r = g.with::<u32, _>("a", |a| {
            g.edit::<u32>("b", |b| *b += *a).unwrap();
            g.value::<u32>("b")
        });
        assert_eq!(r, Some(Some(3)));
    }

    #[test]
    fn test_edit_closure_may_write_the_same_store() {
        let g = Globals::new();
        g.set("a", 1u32);
        g.set("log", Vec::<u32>::new());
        g.edit::<u32>("a", |a| {
            *a += 1;
            g.set("seen_inside_edit", true);
        })
        .unwrap();
        assert_eq!(g.value::<u32>("a"), Some(2));
        assert_eq!(g.value::<bool>("seen_inside_edit"), Some(true));
    }

    #[test]
    fn test_edit_retries_when_value_changes_underneath() {
        let g = Globals::new();
        g.set("n", 0u32);
        let mut runs = 0;
        g.edit::<u32>("n", |n| {
            runs += 1;
            if runs == 1 {
                // Another writer gets in between the copy and the swap.
                g.set("n", 100u32);
            }
            *n += 1;
        })
        .unwrap();
        assert_eq!(runs, 2, "the conflicting attempt was redone");
        assert_eq!(g.value::<u32>("n"), Some(101), "built on the newer value");
    }

    #[test]
    fn test_edit_reports_removal_during_edit() {
        let g = Globals::new();
        g.set("n", 0u32);
        let r = g.edit::<u32>("n", |_| {
            let _removed = g.remove("n");
        });
        assert_eq!(r.unwrap_err(), Error::missing("n"));
    }

    #[test]
    fn test_rejected_define_value_is_dropped() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct Counted(Arc<AtomicUsize>);
        impl Drop for Counted {
            fn drop(&mut self) {
                let _ = self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        let drops = Arc::new(AtomicUsize::new(0));
        let g = Globals::new();
        g.define("x", Counted(Arc::clone(&drops))).unwrap();
        assert!(g.define("x", Counted(Arc::clone(&drops))).is_err());
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_process_wide_instance_is_shared() {
        assert!(std::ptr::eq(globals(), globals()));
    }
}
