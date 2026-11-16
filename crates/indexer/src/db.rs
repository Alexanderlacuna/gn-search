//! The one database check the binary performs before a run.

use mysql::prelude::Queryable;
use mysql::{Opts, Pool};

/// Fails unless the configured database answers `SELECT 1`.
pub fn probe(sql_uri: &str) -> Result<(), String> {
    if sql_uri.is_empty() {
        return Err("SQL_URI is not set".to_string());
    }
    let pool = Pool::new(Opts::from_url(sql_uri).map_err(|e| format!("SQL_URI: {e}"))?)
        .map_err(|e| format!("database pool: {e}"))?;
    let mut conn = pool.get_conn().map_err(|e| format!("connect: {e}"))?;
    conn.query_drop("SELECT 1").map_err(|e| format!("probe: {e}"))
}
