//! The indexer's configuration: the same environment keys GN4 reads, and no more.
//!
//! The indexer needs three things from the environment — where the database is,
//! where the index goes, and where the RDF caches live — so it carries its own
//! reader rather than linking a web application's configuration. The keys are
//! unchanged (`SQL_URI`, `XAPIAN_DB_PATH`, `GN4_RDF_CACHE`), so an operator's
//! existing environment works as-is.

use std::path::PathBuf;

/// Process configuration, read once per run.
#[derive(Debug, Clone)]
pub struct Config {
    /// `SQL_URI` — `mysql://user:password@host/database`.
    pub sql_uri: String,
    /// `XAPIAN_DB_PATH` — the directory the index is written to.
    pub xapian_db_path: String,
    /// `GN4_RDF_CACHE` — the RIF/wiki cache file.
    pub rdf_cache: String,
}

impl Config {
    /// Read the environment, with the defaults GN4 uses.
    pub fn load() -> Self {
        Self {
            sql_uri: std::env::var("SQL_URI").unwrap_or_default(),
            xapian_db_path: std::env::var("XAPIAN_DB_PATH").unwrap_or_default(),
            rdf_cache: std::env::var("GN4_RDF_CACHE")
                .unwrap_or_else(|_| "/tmp/gn4-rdf-cache.json".to_string()),
        }
    }

    /// The RDF cache as a path.
    pub fn rdf_cache_path(&self) -> PathBuf {
        PathBuf::from(&self.rdf_cache)
    }
}
