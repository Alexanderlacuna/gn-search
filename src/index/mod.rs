//! The index seam: how a backend is opened and asked for its match set.
//!
//! SCOUT sections implemented: SCOUT-NATIVE §2 (opening an index, and the
//! pre-checks a binding needs before it opens one), §4 (a hit is decoded
//! from the document's stored JSON) and §5 (the retrieval contract: `page`
//! shifts the offset, and the FULL match set comes back).
//!
//! This trait is the FIXTURE BACKEND's interface, not the production one.
//! Production goes through [`crate::search`] into [`crate::native`], which
//! opens the index with the pre-checks its binding needs (the xapian
//! `Database::open` aborts the process instead of returning an error, so
//! every reachable failure must be an error before the call —
//! `native::open_database`). The fixture is an in-memory test double
//! behind the non-default `fixture` feature; it implements this trait so
//! the parity corpus runner and the crate's tests can ask for a match set
//! without an index directory.
//!
//! `open` is a static method returning `Self`, so the trait is not
//! object-safe; that is deliberate — a backend is chosen at build or
//! config time, not dispatched through a `dyn` handle, and the concrete
//! type keeps the fixture's construction out of a vtable.

use crate::grammar::ParsedQuery;
use crate::hit::SearchHit;
use crate::SearchError;

#[cfg(feature = "fixture")]
pub mod fixture;

/// A searchable index.
pub trait Index: Sized {
    /// Open the index at `path`.
    ///
    /// A backend that wraps a C/C++ binding must do its existence and
    /// shape checks here rather than trusting the open call: the xapian
    /// binding GN4 uses aborts the process on a failed open, so every
    /// reachable failure has to be an error before the binding is
    /// touched (SCOUT-NATIVE §2).
    fn open(path: &str) -> Result<Self, SearchError>;

    /// Fetch the match set for `q`, from the page's offset to its end.
    ///
    /// This is the FULL match set, not one page of it: GN4's native engine
    /// retrieves `mset(first_item, u32::MAX)` — `FULL_MATCH_SET_MAX_ITEMS`
    /// (`native.rs:80`, `native.rs:514`) — and the ROUTE truncates, to its
    /// own `limit` (at most 100) or to `SORT_FETCH_LIMIT` = 1000 on the
    /// sorted path (`routes/search.rs:214-233`). `limit` therefore places
    /// the offset (`offset_for_page`) and is not a cap on the result: a
    /// caller that wants a page truncates what it gets back, and a
    /// backend that returned only `limit` hits would make the route's
    /// sorted path rank the wrong documents.
    ///
    /// `limit == 0` and `limit == u64::MAX` both mean "from the top,
    /// unbounded" (offset 0), which is what GN4's own `first_item =
    /// (page - 1) * limit` arithmetic yields for the first page — the
    /// route's `limit=0` response is an empty `data` array because the
    /// route truncates to 0, not because the engine fetches nothing
    /// (SCOUT-NATIVE §5).
    ///
    /// `q` carries the document-type filter ([`ParsedQuery::type_filter`])
    /// because this signature has no `type_` parameter — the assigned
    /// seam is the four-tuple `(q, type, limit, page)` at the top of the
    /// crate, and the type travels with the query.
    fn search(&self, q: &ParsedQuery, limit: u64, page: u64)
        -> Result<Vec<SearchHit>, SearchError>;
}

/// The offset into the match set that one page starts at:
/// `offset = (page - 1) * limit`, saturating.
///
/// `limit == 0` yields 0 for every page, and a page past the end of the
/// match set yields an empty result, never an error — GN4's engine has the
/// same arithmetic (`first_item`, `native.rs:511`), and `page` is validated
/// to be >= 1 in the route before the engine is called (SCOUT-NATIVE §5).
pub fn offset_for_page(limit: u64, page: u64) -> u64 {
    page.saturating_sub(1).saturating_mul(limit)
}

/// A count as a `usize`, saturating rather than wrapping on a 32-bit
/// target (`limit` comes from a query parameter and is only capped
/// upstream). Only the fixture backend counts anything; the engine hands
/// its counts to xapian.
#[cfg(feature = "fixture")]
pub(crate) fn to_usize(count: u64) -> usize {
    usize::try_from(count).unwrap_or(usize::MAX)
}
