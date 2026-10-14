//! Error type for every fallible operation in the crate.
//!
//! Errors name the global and the types involved, never the value: values
//! are often configuration or credentials, and errors end up in logs.

use std::fmt;

/// Shorthand for `Result<T, global_lib::Error>`.
pub type Result<T> = std::result::Result<T, Error>;

/// Longest name echoed back in an error, in characters.
const NAME_PREVIEW: usize = 64;

/// Everything that can go wrong in `global-lib`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// No global with this name is set.
    Missing {
        /// The name that was looked up, truncated.
        name: String,
    },

    /// A global with this name is set, but holds a different type than the
    /// one requested. Request the type it was set with; `found` names it.
    TypeMismatch {
        /// The name that was looked up, truncated.
        name: String,
        /// The type that was requested.
        expected: &'static str,
        /// The type the global holds.
        found: &'static str,
    },

    /// [`define`](crate::Globals::define) found the name already set.
    /// Definitions are set-once; use [`set`](crate::Globals::set) to
    /// replace a value deliberately.
    AlreadyDefined {
        /// The name that was already set, truncated.
        name: String,
    },
}

impl Error {
    pub(crate) fn missing(name: &str) -> Self {
        Self::Missing {
            name: preview(name),
        }
    }

    pub(crate) fn mismatch(name: &str, expected: &'static str, found: &'static str) -> Self {
        Self::TypeMismatch {
            name: preview(name),
            expected,
            found,
        }
    }

    pub(crate) fn already_defined(name: &str) -> Self {
        Self::AlreadyDefined {
            name: preview(name),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing { name } => write!(f, "no global named {name:?} is set"),
            Self::TypeMismatch {
                name,
                expected,
                found,
            } => write!(f, "global {name:?} holds a {found}, not a {expected}"),
            Self::AlreadyDefined { name } => write!(f, "global {name:?} is already defined"),
        }
    }
}

impl std::error::Error for Error {}

fn preview(name: &str) -> String {
    match name.char_indices().nth(NAME_PREVIEW) {
        Some((cut, _)) => format!("{}...", &name[..cut]),
        None => name.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_every_variant() {
        assert_eq!(
            Error::missing("port").to_string(),
            "no global named \"port\" is set"
        );
        assert_eq!(
            Error::mismatch("port", "u32", "u16").to_string(),
            "global \"port\" holds a u16, not a u32"
        );
        assert_eq!(
            Error::already_defined("app").to_string(),
            "global \"app\" is already defined"
        );
    }

    #[test]
    fn test_long_names_are_truncated() {
        let shown = Error::missing(&"n".repeat(500)).to_string();
        assert!(shown.len() < 110, "{shown}");
        assert!(shown.contains("..."));
    }
}
