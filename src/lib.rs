//! `gn-search`: the GeneNetwork search engine seam as a crate.
//!
//! SCOUT sections implemented here: SCOUT-NATIVE §1 (the entry point —
//! one call carrying the `(q, type, limit, page)` request the route owns,
//! dispatched behind GN4's `Engine` trait per SYNTHESIS §3), with the
//! grammar behind it (§3, [`grammar`]), the hit shape (§4, [`hit`]), the
//! index seam (§2, §5, [`index`]) and the error vocabulary (§6,
//! [`error`]); the sidecar manifest reader is SCOUT-INDEXER §3/§8
//! ([`manifest`]).
//!
//! # What this crate is, and what it deliberately is not
//!
//! One public entry point, [`search`], exactly as wide as GN4's engine
//! seam: `(q, type, limit, page) -> Vec<SearchHit>`. Everything a
//! consumer of `/search` observes — the RFC-7807-shaped error bodies, the
//! `Envelope` (`data`/`meta`/`notes`/`next`), `lod = lrs / 4.604`, the
//! PublishXRef `locus`/`abbreviation` enrichment, `sort`/`order`, the
//! `limit` cap of 100, notes and links — belongs to GN4's route and is
//! NOT in this crate (SCOUT-NATIVE §8; SYNTHESIS §3, §6). Likewise the
//! request validation that produces 400 `invalid_parameter` (empty `q`,
//! `type` outside `phenotype`/`gene`, `page < 1`) happens in the route
//! before the engine is called, so it is not repeated here: duplicating
//! it would move the 400/422 boundary (SYNTHESIS §2d).
//!
//! Two things this crate does own that GN4's route reads from the engine:
//! the *variants* of [`SearchError`] (so the route's status mapping cannot
//! be surprised; SCOUT-NATIVE §6) and the *passthrough* of unknown hit
//! keys through [`SearchHit::extra`] (so `pubmed_link`, which only the
//! proxy path produces, survives — SCOUT-PROXY §3, §4.2).
//!
//! # Backends
//!
//! [`index`] defines the [`Index`] trait. In the default build the only
//! implementation is the in-memory fixture ([`index::fixture`]), so the
//! whole crate is testable with no index directory and no network — but the
//! fixture is a TEST DOUBLE and refuses a non-empty index path rather than
//! answering a real one with its five recorded hits.
//! Building with `--features xapian` selects the xapian backend instead —
//! which today is a stub: it performs the pre-checks of SCOUT-NATIVE §2
//! and then stops, because the Rust binding GN4 depends on is a fork, not
//! a crates.io dependency (SCOUT-NATIVE §8).
//!
//! # Errors
//!
//! The query is rendered before any index is touched, so a malformed
//! query is a [`SearchError::InvalidQuery`] even when the index is
//! unusable — gn3's order (parse first, open second), preserved per
//! SCOUT-NATIVE §6.

pub mod error;
pub mod grammar;
pub mod hit;
pub mod index;

pub use error::SearchError;
pub use grammar::{
    Grammar, LocationShorthand, ParsedQuery, RenderError, RenderedTerm, StemStrategy, SyntaxFault,
    TermKind, GRAMMAR,
};
pub use hit::SearchHit;
pub use index::Index;

/// Run a search against the index at `index_path`.
///
/// SCOUT-NATIVE §1/§5/§6, at the width of GN4's engine seam (SYNTHESIS
/// §3): render `q` with the GN4 grammar, attach the `XT{type}` document
/// filter, then let the configured [`Index`] fetch the match set from the
/// requested page's offset.
///
/// The return value is that backend's whole match set from the offset — the
/// contract GN4's native engine has (`mset(first_item, u32::MAX)`,
/// `native.rs:80`, `native.rs:514`); truncation belongs to the caller. In
/// GN4 that caller is the route, which truncates to its own `limit`
/// (at most 100) or to `SORT_FETCH_LIMIT` = 1000 on the sorted path
/// (`routes/search.rs:212-243`). So `limit` here only places the offset
/// (`(page - 1) * limit`, saturating), `limit == 0` and `limit == u64::MAX`
/// both mean "from the top, unbounded", and a page past the end yields an
/// empty vector, never an error (SCOUT-NATIVE §5).
///
/// The returned hits are in the backend's order — relevance for a real
/// index ([`index::fixture`] returns its build order). Ranking, sorting and
/// truncation stay above this seam.
pub fn search(
    index_path: &str,
    q: &str,
    type_: &str,
    limit: u64,
    page: u64,
) -> Result<Vec<SearchHit>, SearchError> {
    let parsed = match GRAMMAR.render(q) {
        Ok(parsed) => parsed,
        // The query faults the grammar itself detects, all of them the 422
        // `invalid_query` class at GN4: a numeric bound that is not a number
        // (Q52), an invalid `Hs:`/`Mm:` location (Q42) and a syntactically
        // malformed query (Q51) — SCOUT-NATIVE §6.
        Err(err) => {
            return Err(SearchError::InvalidQuery {
                message: err.message().to_string(),
                xapian_error_type: Some(err.xapian_error_type().to_string()),
            })
        }
    };
    open_index(index_path)?.search(&parsed.with_type(type_), limit, page)
}

/// Open the backend selected at build time.
///
/// The default build's backend is the in-memory fixture, and its
/// [`open`](Index::open) REFUSES a non-empty index path: with no xapian
/// backend compiled in, a build that answered a real index path with the
/// fixture's five recorded hits would fabricate search results (see
/// [`index::fixture`]).
#[cfg(not(feature = "xapian"))]
fn open_index(path: &str) -> Result<index::fixture::FixtureIndex, SearchError> {
    index::fixture::FixtureIndex::open(path)
}

/// Open the xapian backend (a stub: see [`index::xapian`]).
#[cfg(feature = "xapian")]
fn open_index(path: &str) -> Result<index::xapian::XapianIndex, SearchError> {
    index::xapian::XapianIndex::open(path)
}
