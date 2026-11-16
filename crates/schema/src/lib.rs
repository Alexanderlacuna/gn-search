//! What a GeneNetwork search document IS — the one definition shared by the
//! engine that reads an index and the indexer that writes one.
//!
//! Keeping the vocabulary in a crate with no heavy dependencies means a
//! consumer can depend on it without inheriting xapian, MySQL or a web server,
//! and it means writer and reader cannot drift apart silently: they agree at
//! compile time.

pub mod error;
pub mod hit;
pub mod manifest;

pub use error::SearchError;
pub use hit::SearchHit;
pub use manifest::{age_seconds, read as read_manifest, sidecar_path, Manifest, ShardEntry};
