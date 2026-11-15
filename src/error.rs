//! The error vocabulary shared with GN4's route.
//!
//! SCOUT section implemented: SCOUT-NATIVE §6 (the variant list and the
//! statuses GN4 maps them to). The mapping itself is GN4's
//! (`src/routes/search.rs::engine_error`) and lives there — this crate
//! only guarantees that the variants exist, carry the message text GN4
//! puts in a response body, and cannot be extended without whoever adds
//! one having to decide what status it means. `tests/error_status.rs`
//! holds that table, kept out of the crate's API on purpose: exporting a
//! status from here would be a second source of truth for a contract
//! GN4 owns (SYNTHESIS §6).
//!
//! The five variants, and what GN4 does with each (SCOUT-NATIVE §6):
//!
//! | Variant | GN4 status | code |
//! |---|---|---|
//! | [`SearchError::InvalidQuery`] | 422 | `invalid_query` |
//! | [`SearchError::Unreachable`] | 502 | `backend_unreachable` |
//! | [`SearchError::Misbehaving`] | 502 | `backend_misbehaving` |
//! | [`SearchError::Backend`] | 502 | `backend_error` |
//! | [`SearchError::Internal`] | 500 | `internal_error` |

use std::error::Error;
use std::fmt;

/// A search failure, in the vocabulary GN4's route already maps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchError {
    /// The backend rejected the query as malformed.
    ///
    /// `message` is the backend's diagnostic, put in the response body
    /// verbatim; `xapian_error_type` is the exception class when the
    /// backend reported one (`QueryParserError`, `InvalidOperationError`),
    /// carried through to `detail.xapian_error_type`. Recorded examples:
    /// `"Syntax: <expression> AND <expression>"` with `QueryParserError`
    /// and `"InvalidOperationError: Query is invalid (invalid Hs/Mm
    /// location field)"` (SCOUT-NATIVE §6).
    InvalidQuery {
        message: String,
        xapian_error_type: Option<String>,
    },
    /// The index (or, on the proxy path, the upstream) could not be
    /// reached: an index path that does not exist, is not a directory, or
    /// is not a glass index (SCOUT-NATIVE §2), or a transport failure.
    Unreachable(String),
    /// Something answered, but not with a search result: a document whose
    /// stored data is not a hit, or a response body that is not a hit
    /// array (SCOUT-NATIVE §4, SCOUT-PROXY §6).
    Misbehaving(String),
    /// The backend failed on its own side (the proxy path's prose 5xx;
    /// SCOUT-PROXY §6).
    Backend(String),
    /// A local misconfiguration or bug: no index configured, no index
    /// path set, an unparsable upstream URL, a failed blocking task
    /// (SCOUT-NATIVE §2, §6).
    Internal(String),
}

impl SearchError {
    /// The diagnostic text GN4 puts in the response body's `detail`.
    pub fn message(&self) -> &str {
        match self {
            Self::InvalidQuery { message, .. } => message,
            Self::Unreachable(message)
            | Self::Misbehaving(message)
            | Self::Backend(message)
            | Self::Internal(message) => message,
        }
    }

    /// The xapian exception class, when the failure carried one.
    pub fn xapian_error_type(&self) -> Option<&str> {
        match self {
            Self::InvalidQuery {
                xapian_error_type, ..
            } => xapian_error_type.as_deref(),
            _ => None,
        }
    }
}

impl fmt::Display for SearchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidQuery {
                xapian_error_type, ..
            } => match xapian_error_type {
                Some(error_type) => write!(f, "{} ({error_type})", self.message()),
                None => write!(f, "{}", self.message()),
            },
            _ => write!(f, "{}", self.message()),
        }
    }
}

impl Error for SearchError {}
