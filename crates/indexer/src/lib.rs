//! The native indexer: crawl, filter, detect, update in place.
//!
//! This module tree is the Rust port of gn3's Python indexer
//! (`~/project/genenetwork3/scripts/index-genenetwork`) described by
//! `~/gn-gemtext-threads/topics/gn4/gn4-search-indexing.gmi`. It shares
//! the crate's configuration (`SQL_URI`, `XAPIAN_DB_PATH`) with the
//! native search engine and writes documents into the live index by
//! their stable identity term, so a per-document update is visible with
//! no service stop.
//!
//! Three modes, one mechanism:
//!
//! * FULL (bootstrap): crawl every public, non-confidential freeze and
//!   build a fresh index. A missing or empty sidecar means FULL.
//! * INCREMENTAL: crawl, hash each document's complete source, diff
//!   against the sidecar, replace only changed documents, delete
//!   vanished identity terms, and write nothing when nothing changed.
//! * DELTA: name changed rows via system versioning (`ROW_START`/
//!   `ROW_END`), re-diff only the affected datasets, and fall back to
//!   FULL on any doubt. The periodic INCREMENTAL sweep remains the
//!   correctness bound (cache text and unversioned tables are invisible
//!   to the delta).
//!
//! The phenotype crawl carries `confidentiality < 1 AND public > 0`
//! (the Python query has no WHERE clause, which is the metadata leak
//! this module closes by construction).

pub mod build;
pub mod config;
pub mod db;
pub mod cache;
pub mod delta;
pub mod livemem;
pub mod pyjson;
pub mod rows;
pub mod run;
pub mod schema;
pub mod sha256;
pub mod sidecar;

pub use run::{RunConfig, Stats, run_delta, run_full, run_incremental};
