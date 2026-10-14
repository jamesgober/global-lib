//! [`Global<T>`]: one typed, process-wide value in a `static`.

use std::fmt;
use std::sync::{Arc, OnceLock};

use crate::snapshot::SnapCell;

/// A typed global value for a `static`, settable and editable at runtime.
///
/// Where [`Globals`](crate::Globals) looks values up by name and checks
/// their type at runtime, `Global<T>` is checked at compile time and
/// skips the name lookup, so it is the choice for hot paths. Reads are
/// lock-free and allocation-free; [`with`](Global::with) also avoids any
/// reference-count change.
///
/// `Debug` shows the type and whether a value is set, never the value.
///
/// # Examples
///
/// ```
/// use global_lib::Global;
///
/// static PORT: Global<u16> = Global::new();
///
/// assert!(!PORT.is_set());
/// PORT.set(7000);
/// PORT.edit(|p| *p += 1);
/// assert_eq!(PORT.value(), Some(7001));
/// assert_eq!(PORT.with(|p| p.to_string()), Some("7001".to_owned()));
/// ```
pub struct Global<T: Send + Sync + 'static> {
    cell: OnceLock<SnapCell<Option<Arc<T>>>>,
}

impl<T: Send + Sync + 'static> Global<T> {
    /// An unset global, usable in a `static`.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            cell: OnceLock::new(),
        }
    }

    fn cell(&self) -> &SnapCell<Option<Arc<T>>> {
        self.cell.get_or_init(|| SnapCell::new(None))
    }

    /// Sets the value, returning the previous one if there was one.
    pub fn set(&self, value: T) -> Option<Arc<T>> {
        let value = Arc::new(value);
        self.cell()
            .update(|current| Some((Some(value), current.clone())))
            .flatten()
    }

    /// Sets the value only if none is set: a set-once definition.
    ///
    /// # Errors
    ///
    /// Gives `value` back if a value is already set; the existing value is
    /// kept.
    pub fn define(&self, value: T) -> Result<(), T> {
        let mut value = Some(value);
        let _unit = self.cell().update(|current| {
            if current.is_some() {
                return None;
            }
            value.take().map(|v| (Some(Arc::new(v)), ()))
        });
        value.map_or(Ok(()), Err)
    }

    /// A shared handle to the value, or `None` if unset.
    #[must_use]
    pub fn get(&self) -> Option<Arc<T>> {
        self.cell().read(Clone::clone)
    }

    /// A clone of the value, or `None` if unset.
    #[must_use]
    pub fn value(&self) -> Option<T>
    where
        T: Clone,
    {
        self.with(T::clone)
    }

    /// Runs `f` on the value in place and returns its result, or `None` if
    /// unset. The fastest read: nothing is cloned and no reference count
    /// changes. Keep `f` short.
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> Option<R> {
        self.cell().read(|current| current.as_deref().map(f))
    }

    /// Changes the value: `f` edits a copy, which replaces the value.
    /// Returns `false` (and does not call `f`) if unset.
    ///
    /// No lock is held while `f` runs, so it may use any global without
    /// risk of deadlock. If another thread changes the value meanwhile, the
    /// edit starts again from the new value (concurrent edits never lose an
    /// update), so `f` can run more than once.
    pub fn edit(&self, mut f: impl FnMut(&mut T)) -> bool
    where
        T: Clone,
    {
        loop {
            let Some(base) = self.get() else {
                return false;
            };
            let mut edited = T::clone(&base);
            f(&mut edited);
            let mut next = Some(Arc::new(edited));
            let swapped = self.cell().update(|current| {
                let unchanged = current.as_ref().is_some_and(|now| Arc::ptr_eq(now, &base));
                if !unchanged {
                    return None;
                }
                Some((Some(next.take()?), ()))
            });
            if swapped.is_some() {
                return true;
            }
            drop(next);
        }
    }

    /// Removes the value and returns it, leaving the global unset.
    pub fn take(&self) -> Option<Arc<T>> {
        self.cell()
            .update(|current| current.clone().map(|old| (None, old)))
    }

    /// Whether a value is set.
    #[must_use]
    pub fn is_set(&self) -> bool {
        self.cell().read(Option::is_some)
    }
}

impl<T: Send + Sync + 'static> Default for Global<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Send + Sync + 'static> fmt::Debug for Global<T> {
    /// The type and whether it is set. The value is never shown.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Global")
            .field("type", &std::any::type_name::<T>())
            .field("set", &self.is_set())
            .finish()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_lifecycle() {
        static G: Global<String> = Global::new();
        assert!(!G.is_set());
        assert_eq!(G.get(), None);
        let called = std::sync::atomic::AtomicBool::new(false);
        assert!(!G.edit(|_| called.store(true, std::sync::atomic::Ordering::SeqCst)));
        assert!(
            !called.load(std::sync::atomic::Ordering::SeqCst),
            "edit ran on an unset global"
        );
        assert_eq!(G.set("a".into()), None);
        assert_eq!(G.set("b".into()).as_deref().map(String::as_str), Some("a"));
        assert!(G.edit(|s| s.push('!')));
        assert_eq!(G.value().as_deref(), Some("b!"));
        assert_eq!(G.with(String::len), Some(2));
        assert_eq!(G.take().as_deref().map(String::as_str), Some("b!"));
        assert!(!G.is_set());
        assert_eq!(G.take(), None);
    }

    #[test]
    fn test_edit_closure_may_write_other_globals_and_retries_on_conflict() {
        static A: Global<u32> = Global::new();
        static B: Global<u32> = Global::new();
        let _ = A.set(1);
        let mut runs = 0;
        assert!(A.edit(|a| {
            runs += 1;
            let _ = B.set(*a);
            if runs == 1 {
                let _ = A.set(10);
            }
            *a += 1;
        }));
        assert_eq!(runs, 2);
        assert_eq!(A.value(), Some(11));
        assert_eq!(B.value(), Some(10));
    }

    #[test]
    fn test_define_is_set_once_and_returns_value() {
        let g: Global<u8> = Global::new();
        assert_eq!(g.define(1), Ok(()));
        assert_eq!(g.define(2), Err(2));
        assert_eq!(g.value(), Some(1));
    }

    #[test]
    fn test_debug_never_shows_value() {
        let g: Global<String> = Global::default();
        let _previous = g.set("s3cr3t".into());
        let shown = format!("{g:?}");
        assert!(shown.contains("String"));
        assert!(shown.contains("true"));
        assert!(!shown.contains("s3cr3t"));
    }

    #[test]
    fn test_concurrent_edits_lose_no_updates() {
        static COUNTER: Global<u64> = Global::new();
        let _previous = COUNTER.set(0);
        let handles: Vec<_> = (0..8)
            .map(|_| {
                std::thread::spawn(|| {
                    for _ in 0..1_000 {
                        assert!(COUNTER.edit(|n| *n += 1));
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(COUNTER.value(), Some(8_000));
    }
}
