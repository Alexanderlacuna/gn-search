//! `gn-search`: the GeneNetwork search engine seam as a crate.
//!
//! SCOUT sections implemented here: SCOUT-NATIVE §1 (the entry point —
//! one call carrying the `(q, type, limit, page)` request the route owns,
//! dispatched behind GN4's `Engine` trait per SYNTHESIS §3), with the
//! engine itself ([`native`]), the hit shape (§4, [`hit`]), the error
//! vocabulary (§6, [`error`]) and the sidecar manifest reader
//! (SCOUT-INDEXER §3/§8, [`manifest`]).
//!
//! # The engine
//!
//! [`native`] is GN4's `src/search/native.rs`, moved here: the xapian
//! `QueryParser` with gn3's registrations, the checked parse, the
//! full-match-set fetch and the stored-document decode. It is the
//! authority on what a query means — nothing in this crate renders a
//! query by hand. [`grammar`] survives as the FIXTURE BACKEND's document
//! selector (`index::fixture`, behind the non-default `fixture`
//! feature): the renderer selects documents for the fixture backend;
//! production never renders a query.
//!
//! # What this crate is, and what it deliberately is not
//!
//! One public entry point, [`search`], exactly as wide as GN4's engine
//! seam: `(index_path, q, type, limit, page) -> Vec<SearchHit>`.
//! Everything a consumer of `/search` observes — the RFC-7807-shaped error
//! bodies, the `Envelope` (`data`/`meta`/`notes`/`next`),
//! `lod = lrs / 4.604`, the PublishXRef `locus`/`abbreviation`
//! enrichment, `sort`/`order`, the `limit` cap of 100, notes and links —
//! belongs to GN4's route and is NOT in this crate (SCOUT-NATIVE §8;
//! SYNTHESIS §3, §6). Likewise the request validation that produces 400
//! `invalid_parameter` (empty `q`, `type` outside `phenotype`/`gene`,
//! `page < 1`) happens in the route before the engine is called, so it is
//! not repeated here: duplicating it would move the 400/422 boundary
//! (SYNTHESIS §2d).
//!
//! Two things this crate does own that GN4's route reads from the engine:
//! the *variants* of [`SearchError`] (so the route's status mapping cannot
//! be surprised; SCOUT-NATIVE §6) and the *passthrough* of unknown hit
//! keys through [`SearchHit::extra`] (so `pubmed_link`, which only the
//! proxy path produces, survives — SCOUT-PROXY §3, §4.2).
//!
//! # Backends
//!
//! [`index`] defines the [`Index`] trait. The production backend is the
//! xapian index the engine opens by path; the in-memory fixture
//! (`index::fixture`, `--features fixture`) is a TEST DOUBLE that the
//! parity corpus runner and the crate's own tests use, and it is not on
//! [`search`]'s path at all: a build that answered a real index path with
//! its five recorded hits would fabricate search results.
//!
//! # Errors
//!
//! The query is parsed before any index is touched, so a malformed query
//! is a [`SearchError::InvalidQuery`] even when the index is unusable —
//! gn3's order (parse first, open second), preserved per SCOUT-NATIVE §6.
//! An unset index path is [`SearchError::Internal`] naming
//! `XAPIAN_DB_PATH`, in exactly one place (`native::open_database`),
//! so every caller of this crate sees the same misconfiguration class.

pub mod error;
pub mod grammar;
pub mod hit;
pub mod index;
pub mod manifest;
pub mod native;

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
/// §3): parse `q` with the gn3 parser configuration, attach the `XT{type}`
/// document filter, and fetch the match set from the requested page's
/// offset. This is a thin function over the moved engine
/// (`native::search_blocking`); the reasoning, the registration table
/// and the guards are that module's, unchanged.
///
/// The return value is the match set from the offset — the contract GN4's
/// native engine has (`mset(first_item, u32::MAX)`, `native.rs:80`,
/// `native.rs:514`); truncation belongs to the caller. In GN4 that caller
/// is the route, which truncates to its own `limit` (at most 100) or to
/// `SORT_FETCH_LIMIT` = 1000 on the sorted path (`routes/search.rs:212-243`).
/// So `limit` here only places the offset (`(page - 1) * limit`,
/// saturating), `limit == 0` and `limit == u64::MAX` both mean "from the
/// top, unbounded", and a page past the end yields an empty vector, never
/// an error (SCOUT-NATIVE §5).
///
/// The returned hits are in the index's order (relevance). Ranking,
/// sorting and truncation stay above this seam.
pub fn search(
    index_path: &str,
    q: &str,
    type_: &str,
    limit: u64,
    page: u64,
) -> Result<Vec<SearchHit>, SearchError> {
    native::search_blocking(index_path, q, type_, limit, page)
}
