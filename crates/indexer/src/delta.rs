//! The versioning-based change-detection (DELTA) path: a DB-clock
//! coverage marker, the seven system-versioned candidate queries, and the
//! fan-out from changed rows to affected dataset labels.
//!
//! This module is deliberately split: the marker/window/decision logic is
//! pure (no DB) and unit-tested directly; the DB helpers (`probe`,
//! `NOW(6)`, `oldest_history`, candidate + fan-out queries) are exercised
//! by the lane integration gate against `db_webqtl_empty`.
//!
//! The delta only ever *narrows* the work: it names changed rows and the
//! datasets they touch, then hands those datasets to the existing
//! build+hash+compare path. It is structurally blind to cache text and to
//! the unversioned contributing tables (`Geno`, `Phenotype`, `Publication`,
//! `ProbeFreeze`, `Tissue`); the periodic incremental sweep remains the
//! correctness bound.

use std::collections::BTreeSet;

use mysql::prelude::Queryable;
use mysql::{Conn, Row};

use super::schema;

/// Timestamp flavour the owner's `ADD SYSTEM VERSIONING` produces:
/// `ROW_START`/`ROW_END` are `TIMESTAMP(6)` (microsecond precision).
pub const FLAVOUR: &str = "timestamp6";

/// The test-only table-prefix seam (see [`table_name`]): the env var that
/// supplies the prefix and the second explicit opt-in required to honour
/// it (a debug build also enables it).
pub const PREFIX_ENV: &str = "GN4_INDEXER_DELTA_TABLE_PREFIX";
/// Second explicit opt-in for the table-prefix seam (test seam): the prefix is
/// ignored unless this is `1` or the binary is a debug build.
pub const TEST_ENV: &str = "GN4_INDEXER_DELTA_TEST";

/// The seven tables the owner versions, each mapped to its candidate key
/// columns (`ROW_START`/`ROW_END` are added by the query template).
pub const VERSIONED_TABLES: [&str; 7] = [
    "PublishXRef",
    "PublishFreeze",
    "ProbeSetXRef",
    "ProbeSetFreeze",
    "ProbeSet",
    "InbredSet",
    "Species",
];

/// How many labels/keys go into one `IN (…)` list before it is split into
/// OR-groups. `Species`/`InbredSet` fan-out can name every dataset.
const IN_CHUNK: usize = 1000;

// ---------------------------------------------------------------------------
// Timestamp + window math (pure, no DB, no new dependency)
// ---------------------------------------------------------------------------

/// A DB timestamp in the fixed `YYYY-MM-DD HH:MM:SS.ffffff` format. The
/// fixed width and zero padding make lexicographic order equal to
/// chronological order, and the hand-rolled civil-date arithmetic avoids
/// adding a time dependency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbTimestamp(String);

impl DbTimestamp {
    /// Strict parse: exact length, separators, and digit positions, with
    /// calendar/clock range checks. `None` for anything else (so a
    /// NULL/unparseable marker cannot silently compare to 0 rows).
    pub fn parse(text: &str) -> Option<DbTimestamp> {
        let bytes = text.as_bytes();
        if bytes.len() != 26 {
            return None;
        }
        let sep = |i: usize, c: u8| bytes[i] == c;
        if !(sep(4, b'-')
            && sep(7, b'-')
            && sep(10, b' ')
            && sep(13, b':')
            && sep(16, b':')
            && sep(19, b'.'))
        {
            return None;
        }
        let digits = |a: usize, b: usize| -> Option<u32> {
            let mut value = 0u32;
            for &byte in &bytes[a..b] {
                if !byte.is_ascii_digit() {
                    return None;
                }
                value = value * 10 + (byte - b'0') as u32;
            }
            Some(value)
        };
        digits(0, 4)?;
        let month = digits(5, 7)?;
        let day = digits(8, 10)?;
        let hour = digits(11, 13)?;
        let minute = digits(14, 16)?;
        let second = digits(17, 19)?;
        let micros = digits(20, 26)?;
        if !(1..=12).contains(&month)
            || !(1..=31).contains(&day)
            || hour > 23
            || minute > 59
            || second > 59
            || micros > 999_999
        {
            return None;
        }
        Some(DbTimestamp(text.to_string()))
    }

    /// The timestamp as the canonical string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Microseconds since the civil epoch (1970-01-01 00:00:00), used only
    /// for ordering and the overlap subtraction.
    fn to_micros(&self) -> i64 {
        let (year, month, day) = self.civil();
        let (hour, minute, second, micros) = self.clock();
        let days = days_from_civil(year, month, day);
        ((days * 86_400 + hour as i64 * 3600 + minute as i64 * 60 + second as i64) * 1_000_000)
            + micros as i64
    }

    fn civil(&self) -> (i64, u32, u32) {
        let b = self.0.as_bytes();
        let num = |a: usize, n: usize| -> i64 {
            b[a..a + n]
                .iter()
                .fold(0i64, |acc, &c| acc * 10 + (c - b'0') as i64)
        };
        (num(0, 4), num(5, 2) as u32, num(8, 2) as u32)
    }

    fn clock(&self) -> (u32, u32, u32, u32) {
        let b = self.0.as_bytes();
        let num = |a: usize, n: usize| -> u32 {
            b[a..a + n]
                .iter()
                .fold(0u32, |acc, &c| acc * 10 + (c - b'0') as u32)
        };
        (num(11, 2), num(14, 2), num(17, 2), num(20, 6))
    }

    fn from_micros(total: i64) -> DbTimestamp {
        let secs = total.div_euclid(1_000_000);
        let micros = total.rem_euclid(1_000_000) as u32;
        let days = secs.div_euclid(86_400);
        let secs_of_day = secs.rem_euclid(86_400);
        let (year, month, day) = civil_from_days(days);
        let hour = (secs_of_day / 3600) as u32;
        let minute = ((secs_of_day % 3600) / 60) as u32;
        let second = (secs_of_day % 60) as u32;
        DbTimestamp(format!(
            "{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}.{micros:06}"
        ))
    }
}

impl std::fmt::Display for DbTimestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl PartialOrd for DbTimestamp {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DbTimestamp {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.to_micros().cmp(&other.to_micros())
    }
}

/// Days from 1970-01-01 for a proleptic-Gregorian civil date (Howard
/// Hinnant's `days_from_civil`).
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = if month > 2 { month - 3 } else { month + 9 } as i64;
    let doy = (153 * mp + 2) / 5 + day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Inverse of [`days_from_civil`].
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { y + 1 } else { y }, month, day)
}

/// The overlap floor: `prev - overlap_secs`, saturating at the civil
/// epoch. Re-examines the tail of the previous window so a late-committed
/// transaction (started before `prev`, committed after the scan) is caught
/// on the next run.
pub fn window_floor(prev: &DbTimestamp, overlap_secs: u64) -> DbTimestamp {
    // MySQL TIMESTAMP cannot precede the civil epoch, so clamp there.
    let micros = prev
        .to_micros()
        .saturating_sub(overlap_secs as i64 * 1_000_000)
        .max(0);
    DbTimestamp::from_micros(micros)
}

/// Overlap seconds, `GN4_INDEXER_DELTA_OVERLAP_SECONDS` (default 30).
pub fn overlap_seconds() -> u64 {
    std::env::var("GN4_INDEXER_DELTA_OVERLAP_SECONDS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(30)
}

/// The startup decision: run the delta at `floor`, or fall back to FULL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Startup {
    /// Run the delta. `prev` is the previous marker, `new` the captured
    /// one, `floor` the overlap-adjusted lower bound.
    Delta {
        prev: DbTimestamp,
        new: DbTimestamp,
        floor: DbTimestamp,
    },
    /// Fall back to FULL, with the reason to log.
    Full(String),
}

/// Decide whether the delta can run, encoding the fallback rules exactly:
/// versioning absent, marker NULL/unparseable, marker in the future,
/// marker not strictly monotonic (log "retry next tick" on the rare
/// equal-microsecond case), and history purged (oldest resolvable version
/// newer than the marker, or no resolvable history at all with a valid
/// previous marker).
///
/// The caller must probe versioning presence **before** computing
/// `oldest_history` (a `FOR SYSTEM_TIME ALL` query errors on an
/// unversioned table), so `probe_ok` is expected to be `true` on the
/// DB path and is kept in the signature so the rule is directly testable.
pub fn decide(
    prev_marker: Option<&str>,
    new_marker: &DbTimestamp,
    probe_ok: bool,
    oldest_history: Option<&DbTimestamp>,
) -> Startup {
    if !probe_ok {
        return Startup::Full("versioning absent".to_string());
    }
    let Some(prev) = prev_marker.and_then(DbTimestamp::parse) else {
        return Startup::Full("no marker".to_string());
    };
    if prev > *new_marker {
        return Startup::Full("marker in future".to_string());
    }
    if prev == *new_marker {
        return Startup::Full(
            "marker not monotonic (equal timestamp; retry next tick)".to_string(),
        );
    }
    // A NULL `oldest_history` (no resolvable version at all) with a valid
    // previous marker means the window's lower bound cannot be resolved,
    // so it is as unsafe as a purge: route to FULL (advisory A2).
    match oldest_history {
        Some(oldest) if *oldest > prev => {
            return Startup::Full("history purged".to_string());
        }
        None => {
            return Startup::Full("history purged".to_string());
        }
        _ => {}
    }
    let floor = window_floor(&prev, overlap_seconds());
    Startup::Delta {
        prev,
        new: new_marker.clone(),
        floor,
    }
}

// ---------------------------------------------------------------------------
// Table-name resolver (the test-only prefix seam, test seam)
// ---------------------------------------------------------------------------

/// Resolve the effective table prefix from raw inputs. Kept pure so the
/// guard is directly testable without touching process env.
pub fn resolve_prefix(
    raw_prefix: Option<&str>,
    test_env: Option<&str>,
    debug_build: bool,
) -> Option<String> {
    let prefix = raw_prefix.unwrap_or("");
    if prefix.is_empty() {
        return None;
    }
    if debug_build || test_env == Some("1") {
        Some(prefix.to_string())
    } else {
        None
    }
}

#[cfg(test)]
thread_local! {
    static TABLE_PREFIX_OVERRIDE: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };
}

/// The first pair of case-variant labels in a set (two labels that differ
/// only by case), or `None`. `identity_term` lowercases the whole
/// `name:dataset`, so two such labels map their documents into the *same*
/// identity-term space; a delta cannot tell their shards apart (deleting
/// the old label's terms would delete the new label's documents), so the
/// caller falls back to FULL, exactly as FULL/INCREMENTAL treat a live
/// case-variant pair as a hard error.
pub fn case_collision(labels: &BTreeSet<String>) -> Option<(String, String)> {
    let mut seen: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for label in labels {
        let key = label.to_lowercase();
        match seen.get(&key) {
            Some(existing) if existing != label => {
                return Some((existing.clone(), label.clone()));
            }
            None => {
                seen.insert(key, label.clone());
            }
            _ => {}
        }
    }
    None
}

/// The effective table prefix. Honoured only when the explicit test env
/// (or a debug build) is present (test seam); otherwise the prefix env is ignored
/// and a warning is expected from the run entry point.
pub fn table_prefix() -> String {
    #[cfg(test)]
    {
        if let Some(prefix) = TABLE_PREFIX_OVERRIDE.with(|cell| cell.borrow().clone()) {
            return prefix;
        }
    }
    resolve_prefix(
        std::env::var(PREFIX_ENV).ok().as_deref(),
        std::env::var(TEST_ENV).ok().as_deref(),
        cfg!(debug_assertions),
    )
    .unwrap_or_default()
}

/// Test-only override for [`table_prefix`] (thread-local, so parallel
/// tests cannot race on process env).
#[cfg(test)]
pub(crate) fn set_table_prefix_override(prefix: Option<String>) {
    TABLE_PREFIX_OVERRIDE.with(|cell| *cell.borrow_mut() = prefix);
}

/// The single resolver for the seven versioned table names (used by both
/// the delta queries and the crawl SQL, so the integration gate can point
/// both at the `vdelta_*` copies). Non-versioned contributing tables
/// (`Geno`, `Phenotype`, `Publication`, `ProbeFreeze`, `Tissue`) are never
/// prefixed.
pub fn table_name(base: &str) -> String {
    format!("{}{}", table_prefix(), base)
}

/// Emit the seam warning once at a run's entry point (test seam).
pub fn log_prefix_warning() {
    let raw = std::env::var(PREFIX_ENV).unwrap_or_default();
    if raw.is_empty() {
        return;
    }
    if table_prefix().is_empty() {
        eprintln!(
            "gn-indexer: WARNING: {PREFIX_ENV}={raw:?} ignored: {TEST_ENV}=1 (or a debug build) is required to enable the test seam"
        );
    } else {
        eprintln!(
            "gn-indexer: WARNING: {PREFIX_ENV}={raw:?} active: the indexer is reading PREFIXED tables (test seam)"
        );
    }
}

// ---------------------------------------------------------------------------
// SQL literal helpers (shared with the crawl's scoped filter, A5)
// ---------------------------------------------------------------------------

/// Quote a raw DB label as a MySQL string literal, escaping backslashes
/// and single quotes. The label is compared with `COLLATE utf8mb4_bin`,
/// so escaping must be exact.
pub fn quote_sql_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('\'');
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("''"),
            _ => out.push(ch),
        }
    }
    out.push('\'');
    out
}

/// A chunked `column IN (…) [OR column IN (…)]` clause over labels, or
/// `0=1` for an empty set (so the caller can always append it).
pub fn label_in_clause(column: &str, labels: &BTreeSet<String>) -> String {
    if labels.is_empty() {
        return "0=1".to_string();
    }
    let mut groups: Vec<String> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    for label in labels {
        current.push(quote_sql_string(label));
        if current.len() == IN_CHUNK {
            groups.push(current.join(", "));
            current.clear();
        }
    }
    if !current.is_empty() {
        groups.push(current.join(", "));
    }
    if groups.len() == 1 {
        format!("{column} IN ({})", groups[0])
    } else {
        let joined = groups
            .iter()
            .map(|group| format!("{column} IN ({group})"))
            .collect::<Vec<_>>()
            .join(" OR ");
        format!("({joined})")
    }
}

/// A chunked `column IN (…)` clause over integer keys, or `0=1` when the
/// set is empty.
pub fn int_in_clause(column: &str, values: &BTreeSet<i64>) -> String {
    if values.is_empty() {
        return "0=1".to_string();
    }
    let mut groups: Vec<String> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    for value in values {
        current.push(value.to_string());
        if current.len() == IN_CHUNK {
            groups.push(current.join(", "));
            current.clear();
        }
    }
    if !current.is_empty() {
        groups.push(current.join(", "));
    }
    if groups.len() == 1 {
        format!("{column} IN ({})", groups[0])
    } else {
        let joined = groups
            .iter()
            .map(|group| format!("{column} IN ({group})"))
            .collect::<Vec<_>>()
            .join(" OR ");
        format!("({joined})")
    }
}

// ---------------------------------------------------------------------------
// DB helpers
// ---------------------------------------------------------------------------

/// Connect for the delta scan: reuse the crawl's connection, then pin
/// READ COMMITTED (design §1.6 — a long REPEATABLE READ snapshot would
/// hide versions committed during the scan).
pub fn connect(sql_uri: &str) -> Result<Conn, String> {
    let mut conn = super::rows::connect(sql_uri)?;
    conn.query_drop("SET SESSION TRANSACTION ISOLATION LEVEL READ COMMITTED")
        .map_err(|e| format!("set read committed: {e}"))?;
    Ok(conn)
}

/// `SELECT NOW(6)` on the DB clock.
pub fn capture_now6(conn: &mut Conn) -> Result<DbTimestamp, String> {
    let value: Option<String> = conn
        .query_first("SELECT NOW(6)")
        .map_err(|e| format!("SELECT NOW(6): {e}"))?;
    value
        .as_deref()
        .and_then(DbTimestamp::parse)
        .ok_or_else(|| "NOW(6) returned an unparseable value".to_string())
}

/// True when all seven versioned tables are system-versioned. Probing
/// `information_schema.TABLES.TABLE_TYPE` is safe on unversioned tables
/// (unlike a `FOR SYSTEM_TIME` query, which errors `Unknown column
/// 'ROW_START'`), and — on MariaDB 10.6 — it is also the *only* reliable
/// tell: the implicit `ROW_START`/`ROW_END` columns are system columns and
/// do NOT appear in `information_schema.COLUMNS`, and the table reports
/// `TABLE_TYPE = 'SYSTEM VERSIONED'` instead of `BASE TABLE`.
pub fn versioning_probe(conn: &mut Conn) -> Result<bool, String> {
    for base in VERSIONED_TABLES {
        let table = table_name(base);
        let sql = format!(
            "SELECT COUNT(*) FROM information_schema.TABLES \
             WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = {} \
             AND TABLE_TYPE = 'SYSTEM VERSIONED'",
            quote_sql_string(&table)
        );
        let count: Option<i64> = conn
            .query_first(&sql)
            .map_err(|e| format!("versioning probe {table}: {e}"))?;
        if count.unwrap_or(0) == 0 {
            return Ok(false);
        }
    }
    Ok(true)
}

/// The oldest resolvable history across the seven tables: `MIN(ROW_START)`
/// of the seven-way `FOR SYSTEM_TIME ALL` UNION (design §4).
pub fn oldest_history(conn: &mut Conn) -> Result<Option<DbTimestamp>, String> {
    let mut parts: Vec<String> = Vec::new();
    for base in VERSIONED_TABLES {
        let table = table_name(base);
        parts.push(format!(
            "SELECT MIN(ROW_START) AS oldest FROM {table} FOR SYSTEM_TIME ALL"
        ));
    }
    let sql = format!("SELECT MIN(oldest) FROM ({}) u", parts.join(" UNION ALL "));
    let value: Option<String> = conn
        .query_first(&sql)
        .map_err(|e| format!("oldest_history: {e}"))?;
    Ok(value.as_deref().and_then(DbTimestamp::parse))
}

/// Capture a fresh coverage marker when versioning is present, for FULL
/// and INCREMENTAL (both re-hash every source, so both are full coverage
/// points). Returns `(marker, flavour)`, or `None` when versioning is
/// absent or the DB cannot be reached — a marker is optional and must
/// never fail an otherwise-valid FULL.
pub fn capture_coverage_marker(sql_uri: &str) -> Option<(String, String)> {
    let mut conn = connect(sql_uri).ok()?;
    if !versioning_probe(&mut conn).unwrap_or(false) {
        return None;
    }
    let now = capture_now6(&mut conn).ok()?;
    Some((now.as_str().to_string(), FLAVOUR.to_string()))
}

// ---------------------------------------------------------------------------
// Candidate scan + fan-out
// ---------------------------------------------------------------------------

/// The change set of one delta window: per-table candidate counts (for
/// stats), the identity terms the candidates name (reporting/twin-gate
/// reasoning), and the affected dataset labels (the write path's unit).
#[derive(Debug, Default, Clone)]
pub struct Candidates {
    pub per_table: std::collections::BTreeMap<&'static str, usize>,
    pub idterms: BTreeSet<String>,
    pub datasets: BTreeSet<String>,
}

/// The `[floor, new)` window predicate: `ROW_START`-in-window (upserts)
/// OR `ROW_END`-in-window (deletes — a `DELETE` leaves `ROW_START` alone
/// and only moves `ROW_END`).
fn window_predicate(floor: &DbTimestamp, new_marker: &DbTimestamp) -> String {
    let floor = quote_sql_string(floor.as_str());
    let new_marker = quote_sql_string(new_marker.as_str());
    format!(
        "(ROW_START >= {floor} AND ROW_START < {new_marker}) \
         OR (ROW_END >= {floor} AND ROW_END < {new_marker})"
    )
}

/// Scan the window and fan the candidates out to affected dataset labels.
pub fn scan(
    conn: &mut Conn,
    floor: &DbTimestamp,
    new_marker: &DbTimestamp,
) -> Result<Candidates, String> {
    let mut out = Candidates::default();
    let window = window_predicate(floor, new_marker);

    // --- PublishXRef (phenotype rows; key (InbredSetId, Id)) ---
    let pubxref = table_name("PublishXRef");
    let pubfreeze = table_name("PublishFreeze");
    let rows = query_i64_rows(
        conn,
        &format!(
            "SELECT DISTINCT InbredSetId, Id FROM {pubxref} FOR SYSTEM_TIME ALL WHERE {window}"
        ),
        2,
    )?;
    out.per_table.insert("PublishXRef", rows.len());
    let mut pub_ids: BTreeSet<i64> = BTreeSet::new();
    for row in &rows {
        if let (Some(_isid), Some(id)) = (row[0], row[1]) {
            pub_ids.insert(id);
        }
    }
    // The InbredSetId(s) of those rows across ALL versions, so a row moved
    // between groups fans out to both old and new labels.
    let mut inbred_sets: BTreeSet<i64> = BTreeSet::new();
    for row in &rows {
        if let Some(isid) = row[0] {
            inbred_sets.insert(isid);
        }
    }
    for value in query_ints(
        conn,
        &format!(
            "SELECT DISTINCT InbredSetId FROM {pubxref} FOR SYSTEM_TIME ALL WHERE {}",
            int_in_clause("Id", &pub_ids)
        ),
    )? {
        inbred_sets.insert(value);
    }
    let freeze_names = query_strings(
        conn,
        &format!(
            "SELECT DISTINCT Name COLLATE utf8mb4_bin FROM {pubfreeze} WHERE {}",
            int_in_clause("InbredSetId", &inbred_sets)
        ),
    )?;
    for name in &freeze_names {
        out.datasets.insert(name.clone());
    }
    for id in &pub_ids {
        for name in &freeze_names {
            out.idterms.insert(schema::identity_term(
                schema::DOCTYPE_PHENOTYPE,
                &id.to_string(),
                name,
            ));
        }
    }

    // --- PublishFreeze (dataset-label owner) ---
    let rows = query_i64_rows(
        conn,
        &format!("SELECT DISTINCT Id FROM {pubfreeze} FOR SYSTEM_TIME ALL WHERE {window}"),
        1,
    )?;
    out.per_table.insert("PublishFreeze", rows.len());
    let freeze_ids: BTreeSet<i64> = rows.iter().filter_map(|r| r[0]).collect();
    for name in query_strings(
        conn,
        &format!(
            "SELECT DISTINCT Name COLLATE utf8mb4_bin FROM {pubfreeze} FOR SYSTEM_TIME ALL WHERE {}",
            int_in_clause("Id", &freeze_ids)
        ),
    )? {
        out.datasets.insert(name);
    }

    // --- ProbeSetXRef (gene rows; key (ProbeSetFreezeId, ProbeSetId)) ---
    let probexref = table_name("ProbeSetXRef");
    let probeset = table_name("ProbeSet");
    let probefreeze = table_name("ProbeSetFreeze");
    let rows = query_i64_rows(
        conn,
        &format!(
            "SELECT DISTINCT ProbeSetFreezeId, ProbeSetId FROM {probexref} FOR SYSTEM_TIME ALL WHERE {window}"
        ),
        2,
    )?;
    out.per_table.insert("ProbeSetXRef", rows.len());
    let mut probe_ids: BTreeSet<i64> = BTreeSet::new();
    let mut probe_freeze_ids: BTreeSet<i64> = BTreeSet::new();
    for row in &rows {
        if let Some(psf) = row[0] {
            probe_freeze_ids.insert(psf);
        }
        if let Some(psid) = row[1] {
            probe_ids.insert(psid);
        }
    }
    for value in query_ints(
        conn,
        &format!(
            "SELECT DISTINCT ProbeSetFreezeId FROM {probexref} FOR SYSTEM_TIME ALL WHERE {}",
            int_in_clause("ProbeSetId", &probe_ids)
        ),
    )? {
        probe_freeze_ids.insert(value);
    }
    let gene_freeze_names = query_strings(
        conn,
        &format!(
            "SELECT DISTINCT Name COLLATE utf8mb4_bin FROM {probefreeze} WHERE {}",
            int_in_clause("Id", &probe_freeze_ids)
        ),
    )?;
    for name in &gene_freeze_names {
        out.datasets.insert(name.clone());
    }
    let probe_names = query_strings(
        conn,
        &format!(
            "SELECT DISTINCT Name COLLATE utf8mb4_bin FROM {probeset} WHERE {}",
            int_in_clause("Id", &probe_ids)
        ),
    )?;
    for probe in &probe_names {
        for freeze in &gene_freeze_names {
            out.idterms
                .insert(schema::identity_term(schema::DOCTYPE_GENE, probe, freeze));
        }
    }

    // --- ProbeSetFreeze (dataset-label owner) ---
    let rows = query_i64_rows(
        conn,
        &format!("SELECT DISTINCT Id FROM {probefreeze} FOR SYSTEM_TIME ALL WHERE {window}"),
        1,
    )?;
    out.per_table.insert("ProbeSetFreeze", rows.len());
    let rows_ids: BTreeSet<i64> = rows.iter().filter_map(|r| r[0]).collect();
    for name in query_strings(
        conn,
        &format!(
            "SELECT DISTINCT Name COLLATE utf8mb4_bin FROM {probefreeze} FOR SYSTEM_TIME ALL WHERE {}",
            int_in_clause("Id", &rows_ids)
        ),
    )? {
        out.datasets.insert(name);
    }

    // --- ProbeSet (gene name owner) ---
    let rows = query_i64_rows(
        conn,
        &format!("SELECT DISTINCT Id FROM {probeset} FOR SYSTEM_TIME ALL WHERE {window}"),
        1,
    )?;
    out.per_table.insert("ProbeSet", rows.len());
    let probeset_ids: BTreeSet<i64> = rows.iter().filter_map(|r| r[0]).collect();
    for name in query_strings(
        conn,
        &format!(
            "SELECT DISTINCT f.Name COLLATE utf8mb4_bin FROM {probefreeze} f \
             JOIN {probexref} x ON x.ProbeSetFreezeId = f.Id \
             WHERE {}",
            int_in_clause("x.ProbeSetId", &probeset_ids)
        ),
    )? {
        out.datasets.insert(name);
    }

    // --- InbredSet (group term owner) ---
    let inbredset = table_name("InbredSet");
    let rows = query_i64_rows(
        conn,
        &format!("SELECT DISTINCT Id FROM {inbredset} FOR SYSTEM_TIME ALL WHERE {window}"),
        1,
    )?;
    out.per_table.insert("InbredSet", rows.len());
    let group_ids: BTreeSet<i64> = rows.iter().filter_map(|r| r[0]).collect();
    for name in query_strings(
        conn,
        &format!(
            "SELECT DISTINCT Name COLLATE utf8mb4_bin FROM {pubfreeze} WHERE {}",
            int_in_clause("InbredSetId", &group_ids)
        ),
    )? {
        out.datasets.insert(name);
    }
    for name in query_strings(
        conn,
        &format!(
            "SELECT DISTINCT f.Name COLLATE utf8mb4_bin FROM {probefreeze} f \
             JOIN ProbeFreeze pr ON pr.Id = f.ProbeFreezeId \
             WHERE {}",
            int_in_clause("pr.InbredSetId", &group_ids)
        ),
    )? {
        out.datasets.insert(name);
    }

    // --- Species (species term owner) ---
    let species = table_name("Species");
    let rows = query_i64_rows(
        conn,
        &format!("SELECT DISTINCT Id FROM {species} FOR SYSTEM_TIME ALL WHERE {window}"),
        1,
    )?;
    out.per_table.insert("Species", rows.len());
    let species_ids: BTreeSet<i64> = rows.iter().filter_map(|r| r[0]).collect();
    for name in query_strings(
        conn,
        &format!(
            "SELECT DISTINCT pf.Name COLLATE utf8mb4_bin FROM {pubfreeze} pf \
             JOIN {inbredset} i ON i.Id = pf.InbredSetId \
             WHERE {}",
            int_in_clause("i.SpeciesId", &species_ids)
        ),
    )? {
        out.datasets.insert(name);
    }
    for name in query_strings(
        conn,
        &format!(
            "SELECT DISTINCT f.Name COLLATE utf8mb4_bin FROM {probefreeze} f \
             JOIN ProbeFreeze pr ON pr.Id = f.ProbeFreezeId \
             JOIN {inbredset} i2 ON i2.Id = pr.InbredSetId \
             WHERE {}",
            int_in_clause("i2.SpeciesId", &species_ids)
        ),
    )? {
        out.datasets.insert(name);
    }

    Ok(out)
}

/// Run a query and return each row as a vector of `Option<i64>` (one per
/// key column), skipping rows whose key columns are all NULL.
fn query_i64_rows(
    conn: &mut Conn,
    sql: &str,
    columns: usize,
) -> Result<Vec<Vec<Option<i64>>>, String> {
    let mut out = Vec::new();
    let result = conn
        .query_iter(sql)
        .map_err(|e| format!("delta query failed: {e}"))?;
    for row in result {
        let row = row.map_err(|e| format!("delta query failed: {e}"))?;
        let mut values = Vec::with_capacity(columns);
        for index in 0..columns {
            let value: Option<i64> = int_at(&row, index)?;
            values.push(value);
        }
        if values.iter().any(|v| v.is_some()) {
            out.push(values);
        }
    }
    Ok(out)
}

/// Run a query returning a single integer column, dropping NULLs.
fn query_ints(conn: &mut Conn, sql: &str) -> Result<Vec<i64>, String> {
    let rows = query_i64_rows(conn, sql, 1)?;
    Ok(rows
        .into_iter()
        .filter_map(|mut row| row.pop().flatten())
        .collect())
}

/// Run a query returning a single string column, dropping NULLs.
fn query_strings(conn: &mut Conn, sql: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let result = conn
        .query_iter(sql)
        .map_err(|e| format!("delta query failed: {e}"))?;
    for row in result {
        let row = row.map_err(|e| format!("delta query failed: {e}"))?;
        if let Some(value) = text_at(&row, 0)? {
            out.push(value);
        }
    }
    Ok(out)
}

fn int_at(row: &Row, index: usize) -> Result<Option<i64>, String> {
    match row.get_opt::<Option<i64>, _>(index) {
        Some(Ok(value)) => Ok(value),
        Some(Err(e)) => Err(format!("column {index}: {e}")),
        None => Err(format!("column {index} not in result set")),
    }
}

fn text_at(row: &Row, index: usize) -> Result<Option<String>, String> {
    match row.get_opt::<Option<String>, _>(index) {
        Some(Ok(value)) => Ok(value),
        Some(Err(e)) => Err(format!("column {index}: {e}")),
        None => Err(format!("column {index} not in result set")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(text: &str) -> DbTimestamp {
        DbTimestamp::parse(text).expect("valid timestamp")
    }

    #[test]
    fn marker_parse_round_trips_and_rejects_garbage() {
        for text in [
            "2026-10-13 19:37:47.469634",
            "2038-01-19 06:14:07.999999",
            "1970-01-01 00:00:00.000000",
            "2026-12-31 23:59:59.999999",
        ] {
            let parsed = ts(text);
            assert_eq!(parsed.as_str(), text);
            assert_eq!(parsed.to_string(), text);
        }
        for bad in [
            "",
            "2026-10-13",
            "2026-10-13 19:37:47",
            "2026-10-13 19:37:47.46963",   // 5 micro digits
            "2026-10-13 19:37:47.4696340", // 7 micro digits
            "2026-13-13 19:37:47.000000",  // month 13
            "2026-10-13 24:37:47.000000",  // hour 24
            "2026-10-1  19:37:47.000000",  // misaligned
            "not a timestamp",
        ] {
            assert!(DbTimestamp::parse(bad).is_none(), "must reject {bad:?}");
        }
    }

    #[test]
    fn ordering_is_chronological() {
        assert!(ts("2026-10-13 19:37:47.469634") < ts("2026-10-13 19:37:47.469635"));
        assert!(ts("2026-10-13 19:37:47.469634") < ts("2026-10-13 19:37:48.000000"));
        assert!(ts("2026-10-13 23:59:59.999999") < ts("2026-10-14 00:00:00.000000"));
    }

    #[test]
    fn floor_subtraction_crosses_second_and_minute_boundaries() {
        assert_eq!(
            window_floor(&ts("2026-10-13 19:37:47.469634"), 30).as_str(),
            "2026-10-13 19:37:17.469634"
        );
        assert_eq!(
            window_floor(&ts("2026-10-13 19:37:10.000000"), 30).as_str(),
            "2026-10-13 19:36:40.000000"
        );
        // Across a minute/hour/day boundary.
        assert_eq!(
            window_floor(&ts("2026-10-14 00:00:05.000000"), 30).as_str(),
            "2026-10-13 23:59:35.000000"
        );
        // Saturates at the epoch instead of underflowing.
        assert_eq!(
            window_floor(&ts("1970-01-01 00:00:00.000000"), 30).as_str(),
            "1970-01-01 00:00:00.000000"
        );
    }

    #[test]
    fn default_overlap_is_thirty_seconds() {
        // No env var in the unit-test process; the default is 30.
        assert_eq!(overlap_seconds(), 30);
    }

    #[test]
    fn decide_fallback_table() {
        let new = ts("2026-10-13 19:40:00.000000");

        // Rule 2: versioning absent wins even with an otherwise-good marker.
        assert_eq!(
            decide(Some("2026-10-13 19:39:00.000000"), &new, false, None),
            Startup::Full("versioning absent".to_string())
        );
        // Rule 3: NULL / unparseable marker.
        assert_eq!(
            decide(None, &new, true, None),
            Startup::Full("no marker".to_string())
        );
        assert_eq!(
            decide(Some("garbage"), &new, true, None),
            Startup::Full("no marker".to_string())
        );
        // Rule 4: marker in the future.
        assert_eq!(
            decide(Some("2026-10-13 19:41:00.000000"), &new, true, None),
            Startup::Full("marker in future".to_string())
        );
        // Rule 5: non-monotonic (equal) -> retry next tick.
        let equal = decide(Some(new.as_str()), &new, true, None);
        match equal {
            Startup::Full(reason) => {
                assert!(reason.contains("not monotonic"), "{reason}");
                assert!(reason.contains("retry next tick"), "{reason}");
            }
            other => panic!("equal marker must fall back, got {other:?}"),
        }
        // Rule 6: history purged (oldest resolvable newer than the marker).
        let prev = ts("2026-10-13 19:39:00.000000");
        assert_eq!(
            decide(
                Some(prev.as_str()),
                &new,
                true,
                Some(&ts("2026-10-13 19:39:30.000000"))
            ),
            Startup::Full("history purged".to_string())
        );
        // Rule 6b (advisory A2): NO resolvable history at all with a valid
        // previous marker is as unsafe as a purge -> FULL.
        assert_eq!(
            decide(Some(prev.as_str()), &new, true, None),
            Startup::Full("history purged".to_string())
        );
        // Healthy: prev < new and oldest <= prev -> delta at the overlap floor.
        match decide(
            Some(prev.as_str()),
            &new,
            true,
            Some(&ts("2026-10-13 19:00:00.000000")),
        ) {
            Startup::Delta { floor, .. } => {
                assert_eq!(floor.as_str(), "2026-10-13 19:38:30.000000")
            }
            other => panic!("healthy marker must run delta, got {other:?}"),
        }
    }

    #[test]
    fn case_collision_detects_case_variant_pairs() {
        let labels: BTreeSet<String> = ["bxd", "BXD", "other"]
            .into_iter()
            .map(String::from)
            .collect();
        let (a, b) = case_collision(&labels).expect("case-variant pair");
        assert!(a.eq_ignore_ascii_case(&b) && a != b, "{a} vs {b}");
        // Identical labels cannot occur in a set; distinct non-case-variant
        // labels do not collide.
        let none: BTreeSet<String> = ["Alpha", "beta", "Gamma"]
            .into_iter()
            .map(String::from)
            .collect();
        assert!(case_collision(&none).is_none());
    }

    #[test]
    fn prefix_resolver_requires_the_second_opt_in() {
        // Prefix set, but no opt-in and a release build: ignored.
        assert_eq!(resolve_prefix(Some("vdelta_"), None, false), None);
        assert_eq!(resolve_prefix(Some("vdelta_"), Some("0"), false), None);
        // Explicit test env, or a debug build: honoured.
        assert_eq!(
            resolve_prefix(Some("vdelta_"), Some("1"), false),
            Some("vdelta_".to_string())
        );
        assert_eq!(
            resolve_prefix(Some("vdelta_"), None, true),
            Some("vdelta_".to_string())
        );
        // Empty prefix: nothing.
        assert_eq!(resolve_prefix(Some(""), Some("1"), true), None);
        assert_eq!(resolve_prefix(None, Some("1"), true), None);
    }

    #[test]
    fn table_name_honours_the_prefix_override() {
        set_table_prefix_override(None);
        assert_eq!(table_name("PublishXRef"), "PublishXRef");
        set_table_prefix_override(Some("vdelta_".to_string()));
        assert_eq!(table_name("PublishXRef"), "vdelta_PublishXRef");
        set_table_prefix_override(None);
    }

    #[test]
    fn quote_sql_string_escapes_quotes_and_backslashes() {
        assert_eq!(quote_sql_string("BXDPublish"), "'BXDPublish'");
        assert_eq!(quote_sql_string("O'Brien"), "'O''Brien'");
        assert_eq!(quote_sql_string("a\\b"), "'a\\\\b'");
        assert_eq!(quote_sql_string("a\\'b"), "'a\\\\''b'");
    }

    #[test]
    fn label_in_clause_chunks_and_escapes() {
        let labels: BTreeSet<String> = ["BXDPublish".to_string(), "O'Brien".to_string()]
            .into_iter()
            .collect();
        let clause = label_in_clause("PublishFreeze.Name COLLATE utf8mb4_bin", &labels);
        assert!(
            clause.contains("PublishFreeze.Name COLLATE utf8mb4_bin IN ("),
            "{clause}"
        );
        assert!(clause.contains("'O''Brien'"), "{clause}");
        assert_eq!(label_in_clause("x", &BTreeSet::new()), "0=1");

        // More than IN_CHUNK labels split into OR-groups.
        let many: BTreeSet<String> = (0..(IN_CHUNK + 5)).map(|i| format!("D{i}")).collect();
        let clause = label_in_clause("col", &many);
        assert!(clause.starts_with('('), "{clause}");
        assert_eq!(clause.matches("col IN (").count(), 2, "{clause}");
    }

    #[test]
    fn window_predicate_covers_start_and_end() {
        let predicate = window_predicate(
            &ts("2026-10-13 19:37:00.000000"),
            &ts("2026-10-13 19:40:00.000000"),
        );
        assert!(predicate.contains("ROW_START >=") && predicate.contains("ROW_START <"));
        assert!(predicate.contains("ROW_END >=") && predicate.contains("ROW_END <"));
    }

    #[test]
    fn versioning_probe_sql_shape() {
        set_table_prefix_override(None);
        // The probe must key on TABLE_TYPE (MariaDB hides the implicit
        // ROW_START column from information_schema.COLUMNS) and cover all
        // seven tables.
        for base in VERSIONED_TABLES {
            let table = table_name(base);
            let sql = format!(
                "SELECT COUNT(*) FROM information_schema.TABLES \
                 WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = {} \
                 AND TABLE_TYPE = 'SYSTEM VERSIONED'",
                quote_sql_string(&table)
            );
            assert!(sql.contains("information_schema.TABLES"), "{sql}");
            assert!(sql.contains("TABLE_TYPE = 'SYSTEM VERSIONED'"), "{sql}");
            assert!(sql.contains(&format!("'{}'", table)), "{sql}");
        }
        // With the prefix active the probe follows it.
        set_table_prefix_override(Some("vdelta_".to_string()));
        assert_eq!(table_name("PublishXRef"), "vdelta_PublishXRef");
        set_table_prefix_override(None);
    }

    #[test]
    fn oldest_history_sql_is_the_seven_way_union() {
        set_table_prefix_override(None);
        let mut parts: Vec<String> = Vec::new();
        for base in VERSIONED_TABLES {
            let table = table_name(base);
            parts.push(format!(
                "SELECT MIN(ROW_START) AS oldest FROM {table} FOR SYSTEM_TIME ALL"
            ));
        }
        let sql = format!("SELECT MIN(oldest) FROM ({}) u", parts.join(" UNION ALL "));
        assert_eq!(sql.matches("FOR SYSTEM_TIME ALL").count(), 7);
        assert_eq!(sql.matches("UNION ALL").count(), 6);
    }
}
