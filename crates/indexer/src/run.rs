//! Run orchestration: FULL (bootstrap) and INCREMENTAL (per-dataset
//! in-place update) over one mechanism — stream the crawl one dataset at
//! a time, diff each dataset against its shard, and write changed
//! documents by identity term.
//!
//! Design of this step (Step 2):
//!
//! * **Streaming, dataset-grouped crawl.** [`rows::stream_dataset_groups`]
//!   runs the two SQL crawls in producer threads ordered by dataset and
//!   hands the run one dataset at a time; no whole-crawl buffer exists.
//! * **Sharded sidecar.** Each dataset has one shard (see [`sidecar`]);
//!   the diff loads only the current dataset's shard, so the resident
//!   idterm->hash map is O(largest shard), not O(documents).
//! * **Cheap-first startup.** The steady-state check compares the
//!   manifest `doc_count` to `db.doc_count()` (O(1)) and recomputes every
//!   shard's `content_sha256` from its file; only on mismatch is the
//!   expensive identity-term enumeration performed (and the run falls
//!   back to FULL). `index_identity_terms` is therefore off the
//!   steady-state path.
//! * **Bounded commits.** Writes are committed every [`COMMIT_CHUNK_DOCS`]
//!   documents with a per-chunk RSS probe; FULL writes a temp index and
//!   swaps it in only on success.
//!
//! Duplicate-identity detection is a separate streaming pass that keeps
//! only the current dataset's identity set (O(largest dataset)) and a
//! crawl-wide map of lowercased dataset labels (O(#datasets)). The label
//! map catches the cross-dataset case: two labels that differ only by case
//! lowercase to the same `<dataset>` in `identity_term`, so their
//! documents collide even though they are distinct shards. The pass runs
//! to completion before any index is opened, so a duplicate crawl can
//! never touch an existing index or sidecar.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use xapian_rs::{Database, DbAction, DbBackend, Enquire, Query, WritableDatabase};

use super::build::{BuiltDocument, build_gene, build_phenotype, gene_idterm, phenotype_idterm};
use super::cache::RdfCaches;
use super::delta;
use super::livemem::{self, CountedDocument};
use super::rows::{self, CrawlRow, DatasetSink};
use super::schema;
use super::sidecar::{self, Manifest, ResidentShard, ShardEntry};

/// Documents written between commits. Stated once; the incremental and
/// FULL writers both commit every `COMMIT_CHUNK_DOCS` (replaced+deleted)
/// documents so a long run never accumulates one unbounded Xapian
/// transaction.
pub const COMMIT_CHUNK_DOCS: usize = 5000;

/// What a run did.
#[derive(Debug, Default, Clone)]
pub struct Stats {
    pub mode: String,
    pub built: usize,
    pub replaced: usize,
    pub deleted: usize,
    pub unchanged: usize,
    pub index_doc_count: u32,
    pub duplicate_identity_terms: Vec<String>,
    /// High-water mark of simultaneously live built documents.
    pub max_live_documents: usize,
    /// Shards written (staged + renamed) this run.
    pub shards_written: usize,
    /// Shards removed this run (dataset no longer present).
    pub shards_deleted: usize,
    /// High-water mark of simultaneously resident shard-map entries.
    pub max_resident_sidecar_entries: usize,
    /// High-water mark of the current dataset's identity-term set.
    pub max_resident_dataset_terms: usize,
    /// High-water mark of simultaneously open shards (load-all RED rises).
    pub peak_open_shards: usize,
    /// Cumulative shard-file bytes read.
    pub shard_bytes_read: usize,
    /// Chunk commits performed.
    pub commit_chunks: usize,
    /// Peak `VmRSS` (KiB) observed by the per-chunk probe.
    pub chunk_rss_peak_kib: u64,
    /// How the run classified startup (`incremental` or a FULL reason).
    pub startup: String,
    /// DELTA: candidates per versioned table (this run's window).
    pub candidates: BTreeMap<String, usize>,
    /// DELTA: dataset labels the window affects.
    pub affected_datasets: usize,
    /// DELTA: the previous coverage marker (`None` when absent).
    pub marker_from: Option<String>,
    /// DELTA: the marker this run advanced to (`None` for FULL/INCREMENTAL
    /// when versioning is absent).
    pub marker_to: Option<String>,
}

/// Inputs for one run.
pub struct RunConfig {
    pub sql_uri: String,
    pub index_path: PathBuf,
    pub rdf_cache: PathBuf,
    /// Apply the public/confidentiality filter to phenotypes. `false`
    /// is the leak RED control only.
    pub publish_filter: bool,
}

/// Test-only sabotage hooks for the RED controls. Never set in a normal
/// run; each value deliberately breaks one guarantee so the gate can be
/// shown to fail. `duplicate` skips the identity-uniqueness error
/// (last-writer-wins); `hash` perturbs every source hash; `delete` drops
/// every delete; `commit` cancels instead of committing; `no-recovery`
/// trusts the sidecar and skips the crash-recovery work entirely.
fn red_mode() -> Option<String> {
    std::env::var("GN4_INDEXER_RED").ok()
}

/// Test-only hooks that can hang or reshape a run (park after N commits,
/// override the commit chunk) are inert unless this opt-in is set, so an
/// accidental environment variable cannot affect a production run.
fn test_hooks_enabled() -> bool {
    std::env::var("GN4_INDEXER_TEST_HOOKS").as_deref() == Ok("1")
}

/// RED: skip the startup shard-integrity check and swallow a corrupt or
/// unreadable shard as empty (the silent-skip regression the check
/// closes).
fn red_skip_shard_check() -> bool {
    red_mode().as_deref() == Some("skip-shard-check")
}

fn index_exists(index_path: &Path) -> bool {
    index_path.is_dir()
}

/// The sibling temp directory a FULL build writes before its atomic
/// swap into place.
fn temp_index_path(index_path: &Path) -> PathBuf {
    let parent = index_path.parent().unwrap_or_else(|| Path::new("."));
    let name = index_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "index".to_string());
    parent.join(format!(".{name}.tmp.{}", std::process::id()))
}

/// Atomically replace `index_path` with the freshly built `temp` index
/// directory. The pre-existing index is moved aside first and restored
/// if the swap fails, so it is never left destroyed by a failed build.
fn swap_into_place(temp: &Path, index_path: &Path) -> Result<(), String> {
    let parent = index_path.parent().unwrap_or_else(|| Path::new("."));
    let backup = parent.join(format!(
        ".{}.old.{}",
        index_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "index".to_string()),
        std::process::id()
    ));
    let had_existing = index_path.exists();
    if had_existing {
        std::fs::rename(index_path, &backup)
            .map_err(|e| format!("index swap (move aside): {e}"))?;
    }
    match std::fs::rename(temp, index_path) {
        Ok(()) => {
            if had_existing {
                let _ = std::fs::remove_dir_all(&backup);
            }
            Ok(())
        }
        Err(e) => {
            if had_existing {
                let _ = std::fs::rename(&backup, index_path);
            }
            Err(format!("index swap (rename into place): {e}"))
        }
    }
}

/// The identity term of a crawl row, or `None` when it produces no
/// document. Uses the same helpers the document builder uses, so the
/// pre-pass and the builder cannot drift.
fn row_idterm(row: &CrawlRow) -> Option<String> {
    match row {
        CrawlRow::Gene(gene) => gene_idterm(gene),
        CrawlRow::Phenotype(phenotype) => phenotype_idterm(phenotype),
    }
}

/// Build the document for a crawl row.
fn build_row(row: CrawlRow, caches: &RdfCaches) -> Option<BuiltDocument> {
    match row {
        CrawlRow::Gene(gene) => build_gene(&gene, caches),
        CrawlRow::Phenotype(phenotype) => build_phenotype(&phenotype),
    }
}

fn red_nonce(red_hash: bool) -> Option<String> {
    if !red_hash {
        return None;
    }
    // Include a per-run counter so every RUN produces different hashes:
    // the point of the RED control is that a re-run over unchanged data
    // still rewrites (i.e. the idempotence gate can fail).
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    Some(format!("-red{}-{n}", std::process::id()))
}

/// Enumerate the identity (`Q`-prefixed) terms the index actually holds.
/// This is the expensive O(docs) enumeration; it is only reached on the
/// startup-mismatch path, never in the steady state.
fn index_identity_terms(db: &WritableDatabase) -> BTreeSet<String> {
    let read: Database = Database::from(db);
    let mut enquire = Enquire::new(&read);
    enquire.set_query(Query::match_all(), None);
    let mset = enquire.mset(0, u32::MAX, None, None);
    let mut terms = BTreeSet::new();
    for item in mset.matches() {
        let document = item.document();
        for term in document.terms() {
            let name: &str = term.as_ref();
            if name.starts_with('Q') {
                terms.insert(name.to_string());
                break;
            }
        }
    }
    terms
}

// ---------------------------------------------------------------------------
// Duplicate-identity pre-pass (one streaming pass, O(largest dataset)).
// ---------------------------------------------------------------------------

struct DuplicateSink {
    set: BTreeSet<String>,
    duplicates: Vec<String>,
    max_terms: usize,
    /// Lowercased dataset label -> the first raw label seen that lowers to
    /// it. The crawl yields one group per raw label; two labels that differ
    /// only by case lowercase to the same `<dataset>` in `identity_term`
    /// (schema.rs), so their documents collide in the index's identity term
    /// space even though they are distinct shards. The per-dataset `set` is
    /// cleared at every `begin_dataset`, so within-dataset idterm duplicates
    /// are caught there; this map is the cross-dataset check.
    labels: HashMap<String, String>,
}

impl DuplicateSink {
    fn new() -> Self {
        DuplicateSink {
            set: BTreeSet::new(),
            duplicates: Vec::new(),
            max_terms: 0,
            labels: HashMap::new(),
        }
    }

    /// Pre-seed the cross-dataset label map from an existing label set
    /// (the manifest's shard keys). "First raw label wins", exactly as if
    /// the crawl itself had yielded them.
    fn seed_labels<I: IntoIterator<Item = String>>(&mut self, labels: I) {
        for label in labels {
            self.labels.entry(label.to_lowercase()).or_insert(label);
        }
    }
}

impl DatasetSink for DuplicateSink {
    fn begin_dataset(&mut self, label: &str) -> Result<(), String> {
        self.set.clear();
        let key = label.to_lowercase();
        match self.labels.get(&key) {
            Some(existing) if existing != label => {
                self.duplicates.push(format!(
                    "case-variant dataset labels {existing:?} and {label:?} both lowercase to {key:?} (cross-dataset identity-term collision)"
                ));
            }
            Some(_) => {}
            None => {
                self.labels.insert(key, label.to_string());
            }
        }
        Ok(())
    }

    fn row(&mut self, row: CrawlRow) -> Result<(), String> {
        if let Some(idterm) = row_idterm(&row) {
            if !self.set.insert(idterm.clone()) {
                self.duplicates.push(idterm);
            }
            self.max_terms = self.max_terms.max(self.set.len());
        }
        Ok(())
    }

    fn end_dataset(&mut self, _label: &str) -> Result<(), String> {
        Ok(())
    }
}

/// Run the duplicate-identity pre-pass. Returns the duplicate terms
/// (empty when the crawl is unique). The per-dataset set is bounded by
/// the largest dataset, and the pass finishes before any index write.
fn duplicate_prepass(sql_uri: &str, filtered: bool) -> Result<(Vec<String>, usize), String> {
    duplicate_prepass_scoped(sql_uri, filtered, None, std::iter::empty())
}

/// The scoped duplicate pre-pass (B1): stream only `datasets` through the
/// [`DuplicateSink`], with its cross-dataset label map PRE-SEEDED from
/// `seed_labels` (the manifest labels that are NOT in scope, advisory A1). This
/// catches a case-variant collision between an affected label and an
/// out-of-scope shard, and (via the per-dataset set) a within-dataset
/// identity-term collision. `datasets = None` is the unscoped
/// FULL/INCREMENTAL pass.
fn duplicate_prepass_scoped<I: IntoIterator<Item = String>>(
    sql_uri: &str,
    filtered: bool,
    datasets: Option<&BTreeSet<String>>,
    seed_labels: I,
) -> Result<(Vec<String>, usize), String> {
    let mut sink = DuplicateSink::new();
    sink.seed_labels(seed_labels);
    rows::stream_dataset_groups_scoped(sql_uri, filtered, datasets, &mut sink)?;
    Ok((sink.duplicates, sink.max_terms))
}

/// The B1 seed set: the existing labels that are NOT already in scope
/// (advisory A1). In-scope labels are checked by the scoped crawl's own map, so
/// seeding them would false-positive on a case-only dataset rename (the
/// manifest keeps the old case, the crawl yields the new one). Pure so the
/// exclusion is directly testable.
fn seed_labels_excluding_scope<'a, I>(labels: I, scope: &BTreeSet<String>) -> Vec<String>
where
    I: Iterator<Item = &'a String>,
{
    labels
        .filter(|label| !scope.contains(*label))
        .cloned()
        .collect()
}

// ---------------------------------------------------------------------------
// Chunked-commit pacer with a per-chunk memory probe.
// ---------------------------------------------------------------------------

/// Reads this process's `VmRSS` (KiB) from `/proc/self/status`.
fn read_vm_rss_kib() -> u64 {
    let text = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    for line in text.lines() {
        if line.starts_with("VmRSS:") {
            if let Some(value) = line.split_whitespace().nth(1) {
                return value.parse().unwrap_or(0);
            }
        }
    }
    0
}

struct CommitPacer {
    chunk: usize,
    open: bool,
    writes_since: usize,
    chunks: usize,
    rss_peak_kib: u64,
    /// Optional file each chunk's probe is appended to (evidence).
    log_path: Option<PathBuf>,
    /// Park after this many chunk commits (crash-gate injection point).
    halt_after: Option<usize>,
    /// RED: cancel instead of committing.
    red_commit: bool,
}

impl CommitPacer {
    fn new(red_commit: bool) -> Self {
        let log_path = std::env::var("GN4_INDEXER_COMMIT_LOG")
            .ok()
            .map(PathBuf::from);
        // The parking hook and the chunk override can hang or reshape a
        // run, so they stay inert unless GN4_INDEXER_TEST_HOOKS opts in.
        let hooks = test_hooks_enabled();
        let halt_after = if hooks {
            std::env::var("GN4_INDEXER_HALT_AFTER_COMMITS")
                .ok()
                .and_then(|value| value.parse().ok())
        } else {
            None
        };
        // The production chunk size is COMMIT_CHUNK_DOCS; the crash gate
        // overrides it downward so a small mix spans several chunks.
        let chunk = if hooks {
            std::env::var("GN4_INDEXER_COMMIT_CHUNK")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(COMMIT_CHUNK_DOCS)
        } else {
            COMMIT_CHUNK_DOCS
        };
        CommitPacer {
            chunk,
            open: false,
            writes_since: 0,
            chunks: 0,
            rss_peak_kib: 0,
            log_path,
            halt_after,
            red_commit,
        }
    }

    /// Record one write, opening a transaction lazily and committing when
    /// the chunk fills.
    fn write(&mut self, db: &mut WritableDatabase) -> Result<(), String> {
        if !self.open {
            db.begin_transaction(None);
            self.open = true;
        }
        self.writes_since += 1;
        if self.writes_since >= self.chunk {
            self.flush(db)?;
        }
        Ok(())
    }

    /// Commit the open chunk (or cancel it under the RED), probe memory,
    /// and optionally park for the crash gate.
    fn flush(&mut self, db: &mut WritableDatabase) -> Result<(), String> {
        if self.red_commit {
            db.cancel_transaction();
        } else {
            db.commit_transaction();
        }
        self.open = false;
        self.writes_since = 0;
        self.chunks += 1;
        let rss = read_vm_rss_kib();
        self.rss_peak_kib = self.rss_peak_kib.max(rss);
        if let Some(path) = &self.log_path {
            use std::io::Write;
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
            {
                let _ = writeln!(file, "chunk={} rss_kib={}", self.chunks, rss);
            }
        }
        if let Some(limit) = self.halt_after {
            if self.chunks == limit {
                // Park forever: the crash gate kills -9 at this point, so
                // the process dies with the chunk committed and no
                // destructors, flushes or sidecar writes having run.
                eprintln!(
                    "gn-indexer: halted after {} chunk commits (kill -9 target)",
                    self.chunks
                );
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(3600));
                }
            }
        }
        Ok(())
    }

    /// Commit the final partial chunk.
    fn finish(&mut self, db: &mut WritableDatabase) -> Result<(), String> {
        if self.open {
            self.flush(db)?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Sidecar finalize (temp shards first, manifest last).
// ---------------------------------------------------------------------------

struct SidecarBuild {
    index_path: PathBuf,
    shard_dir: PathBuf,
    stage_dir: PathBuf,
    manifest: Manifest,
    staged: Vec<(PathBuf, ShardEntry)>,
}

impl SidecarBuild {
    fn new(
        index_path: &Path,
        mode: &str,
        rif_hash: String,
        wiki_hash: String,
        versioning_marker: Option<String>,
        versioning_flavour: Option<String>,
    ) -> Self {
        let stage_dir = PathBuf::from(format!(
            "{}.tmp.{}",
            sidecar::shard_dir(index_path).display(),
            std::process::id()
        ));
        SidecarBuild {
            index_path: index_path.to_path_buf(),
            shard_dir: sidecar::shard_dir(index_path),
            stage_dir,
            manifest: Manifest {
                version: sidecar::SIDECAR_VERSION,
                mode: mode.to_string(),
                doc_count: 0,
                rif_hash,
                wiki_hash,
                schema_version: Some(gn_search_schema::SCHEMA_VERSION),
                schema_hash: Some(gn_search_schema::schema_digest()),
                built_at: Some(built_at()),
                versioning_marker,
                versioning_flavour,
                shards: BTreeMap::new(),
            },
            staged: Vec::new(),
        }
    }

    /// Stage a shard's new content (temp + fsync) and record its manifest
    /// entry. The rename happens in [`finalize`].
    fn stage(&mut self, label: &str, map: &BTreeMap<String, String>) -> Result<(), String> {
        let (temp, entry) = sidecar::stage_shard(&self.stage_dir, label, map)?;
        self.manifest
            .shards
            .insert(label.to_string(), entry.clone());
        self.staged.push((temp, entry));
        Ok(())
    }

    /// Keep an unchanged shard's existing (recomputed) entry.
    fn keep(&mut self, label: &str, entry: ShardEntry) {
        self.manifest.shards.insert(label.to_string(), entry);
    }

    /// Drop a dataset whose rows are all gone.
    fn remove(&mut self, label: &str) {
        self.manifest.shards.remove(label);
    }

    fn shard_count(&self) -> usize {
        self.manifest.shards.len()
    }

    /// Sum of the (kept + staged) shard counts: the total document count
    /// the finalized manifest will describe.
    fn total_count(&self) -> usize {
        self.manifest.shards.values().map(|e| e.count).sum()
    }

    /// Rename every staged shard into place, fsync the shard directory,
    /// prune shard files the manifest no longer references, and write the
    /// **manifest last**.
    fn finalize(mut self) -> Result<(), String> {
        self.manifest.doc_count = self.manifest.shards.values().map(|e| e.count).sum();
        std::fs::create_dir_all(&self.shard_dir)
            .map_err(|e| format!("create shard dir {}: {e}", self.shard_dir.display()))?;
        for (temp, entry) in &self.staged {
            let target = self.shard_dir.join(&entry.file);
            std::fs::rename(temp, &target)
                .map_err(|e| format!("rename shard {}: {e}", target.display()))?;
        }
        sidecar::fsync_dir(&self.shard_dir)?;

        let keep: BTreeSet<String> = self
            .manifest
            .shards
            .values()
            .map(|e| e.file.clone())
            .collect();
        if let Ok(entries) = std::fs::read_dir(&self.shard_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue;
                }
                if !keep.contains(&name) {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
        let _ = std::fs::remove_dir_all(&self.stage_dir);
        self.manifest.store(&self.index_path)
    }
}

// ---------------------------------------------------------------------------
// FULL
// ---------------------------------------------------------------------------

/// Run a FULL (bootstrap) build: duplicate pre-pass, then a fresh temp
/// index written dataset by dataset through chunked commits, swapped into
/// place only on full success, then a fresh sharded sidecar.
/// When this manifest was finalised, as `YYYY-MM-DDTHH:MM:SSZ`.
///
/// Stamped so a reader can report how old the answers are instead of guessing;
/// not a coverage marker (that is `versioning_marker`, which is about the data).
fn built_at() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // civil-from-days, the same arithmetic as the sidecar's other timestamps
    let (days, secs) = ((now / 86_400) as i64, now % 86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        secs / 3600, (secs % 3600) / 60, secs % 60
    )
}

/// Howard Hinnant's days-to-civil-date, for the stamp above.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

pub fn run_full(config: &RunConfig) -> Result<Stats, String> {
    run_full_with_reason(config, None)
}

fn run_full_with_reason(config: &RunConfig, reason: Option<String>) -> Result<Stats, String> {
    let caches = RdfCaches::load(&config.rdf_cache)?;
    // A fresh coverage point, captured BEFORE the crawl; `None` when
    // versioning is absent. Rides the manifest, written last.
    let coverage = delta::capture_coverage_marker(&config.sql_uri);
    // Duplicate validation happens BEFORE any index is opened or written.
    let (duplicates, max_dataset_terms) =
        duplicate_prepass(&config.sql_uri, config.publish_filter)?;
    let red = red_mode();
    if !duplicates.is_empty() && red.as_deref() != Some("duplicate") {
        return Err(format!(
            "duplicate identity terms in crawl: {}{}",
            duplicates.join(", "),
            match &reason {
                Some(r) => format!(" (startup: {r})"),
                None => String::new(),
            }
        ));
    }

    livemem::reset();
    sidecar::mem::reset();
    sidecar::cleanup_temp(&config.index_path)?;
    let retain = livemem::red_retain();

    let temp = temp_index_path(&config.index_path);
    if temp.exists() {
        let _ = std::fs::remove_dir_all(&temp);
    }
    let mut build = SidecarBuild::new(
        &config.index_path,
        "full",
        caches.rif.hash.clone(),
        caches.wiki.hash.clone(),
        coverage.as_ref().map(|c| c.0.clone()),
        coverage.as_ref().map(|c| c.1.clone()),
    );
    let outcome = stream_full(&temp, config, &caches, &mut build, retain);
    let (index_doc_count, built, pacer_chunks, pacer_rss_peak) = match outcome {
        Ok(value) => value,
        Err(error) => {
            let _ = std::fs::remove_dir_all(&temp);
            let _ = std::fs::remove_dir_all(&build.stage_dir);
            return Err(error);
        }
    };
    if let Err(error) = swap_into_place(&temp, &config.index_path) {
        let _ = std::fs::remove_dir_all(&temp);
        return Err(error);
    }

    // Sidecar strictly after the index commit (and the swap): changed
    // shards first, then the manifest last.
    let shard_count = build.shard_count();
    let stage_dir = build.stage_dir.clone();
    if let Err(error) = build.finalize() {
        let _ = std::fs::remove_dir_all(&stage_dir);
        return Err(error);
    }

    let mut stats = Stats {
        mode: "full".to_string(),
        built,
        replaced: built,
        deleted: 0,
        unchanged: 0,
        index_doc_count,
        duplicate_identity_terms: Vec::new(),
        max_live_documents: livemem::max_live(),
        shards_written: shard_count,
        shards_deleted: 0,
        max_resident_sidecar_entries: sidecar::mem::peak_resident_entries(),
        max_resident_dataset_terms: max_dataset_terms,
        peak_open_shards: sidecar::mem::peak_open_shards(),
        shard_bytes_read: sidecar::mem::bytes_read(),
        commit_chunks: pacer_chunks,
        chunk_rss_peak_kib: pacer_rss_peak,
        startup: match reason {
            Some(reason) => format!("full ({reason})"),
            None => "full".to_string(),
        },
        candidates: BTreeMap::new(),
        affected_datasets: 0,
        marker_from: None,
        marker_to: coverage.as_ref().map(|c| c.0.clone()),
    };
    stats.mode = "full".to_string();
    Ok(stats)
}

struct FullSink<'a> {
    db: WritableDatabase,
    caches: &'a RdfCaches,
    red_hash: bool,
    retain: bool,
    kept: Vec<CountedDocument<BuiltDocument>>,
    pacer: CommitPacer,
    build: &'a mut SidecarBuild,
    shard: BTreeMap<String, String>,
    built: usize,
}

impl<'a> DatasetSink for FullSink<'a> {
    fn begin_dataset(&mut self, _label: &str) -> Result<(), String> {
        self.shard.clear();
        Ok(())
    }

    fn row(&mut self, row: CrawlRow) -> Result<(), String> {
        let Some(mut document) = build_row(row, self.caches) else {
            return Ok(());
        };
        if let Some(nonce) = red_nonce(self.red_hash) {
            document.source_hash.push_str(&nonce);
        }
        let counted = CountedDocument::new(document);
        let doc = counted.get();
        self.db.replace_document_by_term(&doc.idterm, &doc.document);
        self.shard
            .insert(doc.idterm.clone(), doc.source_hash.clone());
        self.built += 1;
        if self.retain {
            self.kept.push(counted);
        }
        self.pacer.write(&mut self.db)?;
        Ok(())
    }

    fn end_dataset(&mut self, label: &str) -> Result<(), String> {
        if !self.shard.is_empty() {
            self.build.stage(label, &self.shard)?;
        }
        self.shard.clear();
        Ok(())
    }
}

fn stream_full(
    temp: &Path,
    config: &RunConfig,
    caches: &RdfCaches,
    build: &mut SidecarBuild,
    retain: bool,
) -> Result<(u32, usize, usize, u64), String> {
    // One writer handle, opened into the fresh temp directory; the
    // transaction is opened lazily by the commit pacer.
    let db = WritableDatabase::open(
        temp,
        Some(DbAction::CreateOrOverwrite),
        Some(DbBackend::Glass),
        None,
        None,
    );
    let mut sink = FullSink {
        db,
        caches,
        red_hash: red_mode().as_deref() == Some("hash"),
        retain,
        kept: Vec::new(),
        pacer: CommitPacer::new(red_mode().as_deref() == Some("commit")),
        build,
        shard: BTreeMap::new(),
        built: 0,
    };
    let result = rows::stream_dataset_groups(&config.sql_uri, config.publish_filter, &mut sink);
    if let Err(error) = result {
        if sink.pacer.open {
            sink.db.cancel_transaction();
        }
        return Err(error);
    }
    sink.pacer.finish(&mut sink.db)?;
    let doc_count = sink.db.doc_count();
    let built = sink.built;
    let chunks = sink.pacer.chunks;
    let rss_peak = sink.pacer.rss_peak_kib;
    Ok((doc_count, built, chunks, rss_peak))
}

// ---------------------------------------------------------------------------
// INCREMENTAL
// ---------------------------------------------------------------------------

/// The outcome of the shared startup pre-flight (obj-3 cheap-first
/// checks). Both INCREMENTAL and DELTA use it; any doubt returns `Full`.
enum Pre {
    /// The manifest is trustworthy; proceed with the given manifest.
    Manifest(Manifest),
    /// Fall back to FULL for the given reason.
    Full(String),
}

/// Cheap-first startup shared by INCREMENTAL and DELTA: load the manifest
/// (missing/old-version => FULL), check the index exists, compare the
/// manifest `doc_count` to the index's own (O(1)), and verify every shard
/// (bit-rot). Any mismatch routes to FULL with a reason; the expensive
/// identity-term enumeration runs only on the doc-count mismatch path.
fn preflight(config: &RunConfig) -> Result<Pre, String> {
    let Some(manifest) = Manifest::load(&config.index_path) else {
        // Missing, unreadable, or old-version sidecar means FULL.
        return Ok(Pre::Full("no/old sidecar".to_string()));
    };
    if !index_exists(&config.index_path) {
        return Ok(Pre::Full("index missing".to_string()));
    }

    sidecar::cleanup_temp(&config.index_path)?;

    let probe = WritableDatabase::open(
        &config.index_path,
        Some(DbAction::CreateOrOpen),
        Some(DbBackend::Glass),
        None,
        None,
    );
    let index_doc_count = probe.doc_count();
    let red = red_mode();
    let no_recovery = red.as_deref() == Some("no-recovery");
    if !no_recovery && index_doc_count as usize != manifest.doc_count {
        let reason = format!(
            "doc_count index={} manifest={}",
            index_doc_count, manifest.doc_count
        );
        // Expensive path only here: enumerate index identity terms to
        // characterise the key-set mismatch.
        let index_terms = index_identity_terms(&probe);
        eprintln!(
            "gn-indexer: startup mismatch ({reason}); index identity terms = {}; falling back to FULL",
            index_terms.len()
        );
        drop(probe);
        return Ok(Pre::Full(reason));
    }
    drop(probe);

    // Verify every shard (bit-rot) and confirm it parses, before trusting
    // the manifest. Any mismatch routes to FULL. Each shard is loaded and
    // dropped, so the peak stays O(largest shard).
    if !no_recovery && !red_skip_shard_check() {
        let dir = sidecar::shard_dir(&config.index_path);
        for (label, entry) in &manifest.shards {
            if let Err(error) = ResidentShard::load(&dir, label, entry) {
                let reason = format!("shard {label}: {error}");
                eprintln!("gn-indexer: startup mismatch ({reason}); falling back to FULL");
                return Ok(Pre::Full(reason));
            }
        }
    }

    Ok(Pre::Manifest(manifest))
}

/// Run an INCREMENTAL pass: cheap-first startup check, then a dataset-
/// grouped streaming diff that replaces changed documents, deletes
/// vanished ones, and writes nothing when nothing changed.
pub fn run_incremental(config: &RunConfig) -> Result<Stats, String> {
    let manifest = match preflight(config)? {
        Pre::Manifest(manifest) => manifest,
        Pre::Full(reason) => return run_full_with_reason(config, Some(reason)),
    };

    // An incremental re-hashes every source, so it is a full coverage
    // point: capture a fresh marker before its crawl (versioning absent =>
    // None). On the `nothing_changed` skip below the manifest is not
    // written, so the previous marker stays (safe: the next delta window
    // is simply larger).
    let coverage = delta::capture_coverage_marker(&config.sql_uri);
    let red = red_mode();
    let no_recovery = red.as_deref() == Some("no-recovery");

    let caches = RdfCaches::load(&config.rdf_cache)?;
    // Duplicate validation before any write.
    let (duplicates, max_dataset_terms) =
        duplicate_prepass(&config.sql_uri, config.publish_filter)?;
    if !duplicates.is_empty() && red.as_deref() != Some("duplicate") {
        return Err(format!(
            "duplicate identity terms in crawl: {}",
            duplicates.join(", ")
        ));
    }

    livemem::reset();
    sidecar::mem::reset();
    let retain = livemem::red_retain();

    let db = WritableDatabase::open(
        &config.index_path,
        Some(DbAction::CreateOrOpen),
        Some(DbBackend::Glass),
        None,
        None,
    );

    let shard_dir = sidecar::shard_dir(&config.index_path);
    let by_label: HashMap<String, ShardEntry> = manifest
        .shards
        .iter()
        .map(|(label, entry)| (label.clone(), entry.clone()))
        .collect();

    let mut build = SidecarBuild::new(
        &config.index_path,
        "incremental",
        caches.rif.hash.clone(),
        caches.wiki.hash.clone(),
        coverage.as_ref().map(|c| c.0.clone()),
        coverage.as_ref().map(|c| c.1.clone()),
    );
    // Carry forward every existing shard entry; changed datasets replace
    // their entry, removed datasets are dropped below.
    for (label, entry) in &manifest.shards {
        build.keep(label, entry.clone());
    }

    let mut sink = IncrementalSink {
        db,
        caches: &caches,
        red_hash: red.as_deref() == Some("hash"),
        red_delete: red.as_deref() == Some("delete"),
        no_recovery,
        retain,
        kept: Vec::new(),
        pacer: CommitPacer::new(red.as_deref() == Some("commit")),
        build: &mut build,
        shard_dir,
        by_label,
        affected_scope: None,
        seen: BTreeSet::new(),
        previous: None,
        previous_label: String::new(),
        dataset_terms: BTreeSet::new(),
        dataset_map: BTreeMap::new(),
        retainer: sidecar::ResidentRetainer::new(),
        replaced: 0,
        deleted: 0,
        unchanged: 0,
        max_dataset_terms: 0,
        new_shards: 0,
        removed_shards: 0,
    };

    let result = rows::stream_dataset_groups(&config.sql_uri, config.publish_filter, &mut sink);
    if let Err(error) = result {
        if sink.pacer.open {
            sink.db.cancel_transaction();
        }
        return Err(error);
    }

    // Datasets in the manifest that the crawl never produced: every one
    // of their documents is gone.
    let leftovers = sink.leftover_labels();
    for label in leftovers {
        sink.delete_dataset(&label)?;
        sink.removed_shards += 1;
    }

    let nothing_changed = sink.replaced == 0 && sink.deleted == 0;
    sink.pacer.finish(&mut sink.db)?;
    let doc_count = sink.db.doc_count();
    let replaced = sink.replaced;
    let deleted = sink.deleted;
    let unchanged = sink.unchanged;
    let max_resident = sidecar::mem::peak_resident_entries();
    let peak_open = sidecar::mem::peak_open_shards();
    let bytes_read = sidecar::mem::bytes_read();
    let shards_written = sink.new_shards;
    let shards_deleted = sink.removed_shards;
    let sink_dataset_terms = sink.max_dataset_terms;
    let commit_chunks = sink.pacer.chunks;
    let chunk_rss_peak_kib = sink.pacer.rss_peak_kib;
    let max_live = livemem::max_live();
    drop(sink);

    // The finalized manifest's shard counts are the crawl's document
    // count (kept + staged, minus removed datasets).
    let built = build.total_count();

    // Sidecar after the commit; skipped when nothing changed so an
    // idempotent run touches neither the index nor the sidecar.
    if !nothing_changed {
        build.finalize()?;
    } else {
        let _ = std::fs::remove_dir_all(&build.stage_dir);
    }

    Ok(Stats {
        mode: "incremental".to_string(),
        built,
        replaced,
        deleted,
        unchanged,
        index_doc_count: doc_count,
        duplicate_identity_terms: Vec::new(),
        max_live_documents: max_live,
        shards_written,
        shards_deleted,
        max_resident_sidecar_entries: max_resident,
        max_resident_dataset_terms: max_dataset_terms.max(sink_dataset_terms),
        peak_open_shards: peak_open,
        shard_bytes_read: bytes_read,
        commit_chunks,
        chunk_rss_peak_kib,
        startup: "incremental".to_string(),
        candidates: BTreeMap::new(),
        affected_datasets: 0,
        marker_from: None,
        marker_to: coverage.as_ref().map(|c| c.0.clone()),
    })
}

/// The leftover-delete set: manifest labels the crawl never produced,
/// restricted to `scope` when set. Pure so the DELTA scoping is directly
/// testable (a scoped crawl must never delete an out-of-scope shard).
fn leftover_labels_in_scope<'a, I>(
    labels: I,
    seen: &BTreeSet<String>,
    scope: Option<&BTreeSet<String>>,
) -> Vec<String>
where
    I: Iterator<Item = &'a String>,
{
    labels
        .filter(|label| !seen.contains(*label))
        .filter(|label| scope.map_or(true, |scope| scope.contains(*label)))
        .cloned()
        .collect()
}

struct IncrementalSink<'a> {
    db: WritableDatabase,
    caches: &'a RdfCaches,
    red_hash: bool,
    red_delete: bool,
    no_recovery: bool,
    retain: bool,
    kept: Vec<CountedDocument<BuiltDocument>>,
    pacer: CommitPacer,
    build: &'a mut SidecarBuild,
    shard_dir: PathBuf,
    by_label: HashMap<String, ShardEntry>,
    /// DELTA only: when set, the leftover-delete path is restricted to
    /// these datasets (a scoped crawl cannot delete out-of-scope shards).
    affected_scope: Option<BTreeSet<String>>,
    seen: BTreeSet<String>,
    previous: Option<ResidentShard>,
    previous_label: String,
    dataset_terms: BTreeSet<String>,
    dataset_map: BTreeMap<String, String>,
    retainer: sidecar::ResidentRetainer,
    replaced: usize,
    deleted: usize,
    unchanged: usize,
    max_dataset_terms: usize,
    new_shards: usize,
    removed_shards: usize,
}

impl<'a> IncrementalSink<'a> {
    /// Manifest labels the crawl never produced (all their documents are
    /// gone), optionally restricted to `affected_scope`. Without a scope
    /// this is the INCREMENTAL semantics (every leftover); with one it is
    /// the DELTA semantics (only in-window datasets may be deleted).
    fn leftover_labels(&self) -> Vec<String> {
        leftover_labels_in_scope(
            self.by_label.keys(),
            &self.seen,
            self.affected_scope.as_ref(),
        )
    }

    /// Delete every document of a dataset (all its rows vanished) and
    /// drop its shard.
    fn delete_dataset(&mut self, label: &str) -> Result<(), String> {
        if self.no_recovery {
            // RED: trust the sidecar, skip the recovery work entirely.
            return Ok(());
        }
        if !self.red_delete {
            let entry = self.by_label.get(label).cloned();
            if let Some(entry) = entry {
                let shard = ResidentShard::load(&self.shard_dir, label, &entry)
                    .map_err(|e| format!("shard {label}: {e}"))?;
                for term in shard.map().keys() {
                    self.db.delete_document_by_term(term);
                    self.deleted += 1;
                    self.pacer.write(&mut self.db)?;
                }
            }
        }
        self.build.remove(label);
        Ok(())
    }
}

impl<'a> DatasetSink for IncrementalSink<'a> {
    fn begin_dataset(&mut self, label: &str) -> Result<(), String> {
        self.seen.insert(label.to_string());
        self.dataset_terms.clear();
        self.dataset_map.clear();
        self.previous = None;
        self.previous_label = label.to_string();
        if let Some(entry) = self.by_label.get(label).cloned() {
            // Verify-before-parse: the shard file's hash is re-checked
            // here (it was checked at startup too). A mismatch would mean
            // the file changed under us; treat it as a hard error.
            match ResidentShard::load(&self.shard_dir, label, &entry) {
                Ok(shard) => {
                    if sidecar::red_load_all() {
                        // Load-all RED: hold every shard resident so the peak
                        // resident counter reflects O(docs), not O(largest shard).
                        self.retainer.retain(shard);
                    } else {
                        self.previous = Some(shard);
                    }
                }
                Err(error) => {
                    if red_skip_shard_check() {
                        // RED: swallow the corruption as an empty shard, so
                        // the dataset is silently treated as new/empty.
                        eprintln!("gn-indexer: [RED skip-shard-check] ignoring {label}: {error}");
                        self.previous = None;
                    } else {
                        return Err(format!("shard {label}: {error}"));
                    }
                }
            }
        }
        Ok(())
    }

    fn row(&mut self, row: CrawlRow) -> Result<(), String> {
        let Some(idterm) = row_idterm(&row) else {
            return Ok(());
        };
        self.dataset_terms.insert(idterm.clone());
        self.max_dataset_terms = self.max_dataset_terms.max(self.dataset_terms.len());
        let Some(mut document) = build_row(row, self.caches) else {
            return Ok(());
        };
        if let Some(nonce) = red_nonce(self.red_hash) {
            document.source_hash.push_str(&nonce);
        }
        let counted = CountedDocument::new(document);
        let doc = counted.get();
        self.dataset_map
            .insert(doc.idterm.clone(), doc.source_hash.clone());
        let known = if self.no_recovery {
            // RED: pretend the source hash is unchanged, so no rewrite
            // ever happens and a crash cannot be recovered.
            Some(doc.source_hash.clone())
        } else {
            self.previous
                .as_ref()
                .and_then(|shard| shard.map().get(&doc.idterm).cloned())
        };
        match known {
            Some(hash) if hash == doc.source_hash => self.unchanged += 1,
            _ => {
                self.db.replace_document_by_term(&doc.idterm, &doc.document);
                self.replaced += 1;
                self.pacer.write(&mut self.db)?;
            }
        }
        if self.retain {
            self.kept.push(counted);
        }
        Ok(())
    }

    fn end_dataset(&mut self, label: &str) -> Result<(), String> {
        // Deletes: identity terms in the previous shard that the crawl no
        // longer produces, computed from this dataset's key set.
        if !self.red_delete && !self.no_recovery {
            if let Some(shard) = &self.previous {
                let vanished: Vec<String> = shard
                    .map()
                    .keys()
                    .filter(|term| !self.dataset_terms.contains(*term))
                    .cloned()
                    .collect();
                for term in vanished {
                    self.db.delete_document_by_term(&term);
                    self.deleted += 1;
                    self.pacer.write(&mut self.db)?;
                }
            }
        }

        // Decide whether the shard content changed.
        let unchanged_shard = match &self.previous {
            Some(shard) => shard.map() == &self.dataset_map,
            None => self.dataset_map.is_empty(),
        };
        if !unchanged_shard && !self.dataset_map.is_empty() {
            self.build.stage(label, &self.dataset_map)?;
            self.new_shards += 1;
        } else if unchanged_shard {
            if let Some(entry) = self.by_label.get(label).cloned() {
                self.build.keep(label, entry);
            }
        } else if self.dataset_map.is_empty() {
            // The dataset still appears in the crawl but produced no
            // documents: treat it as gone.
            self.build.remove(label);
        }
        self.previous = None;
        Ok(())
    }
}

/// Run a DELTA pass: name changed rows via system versioning, re-diff only
/// the affected datasets through the existing build+hash+compare path, and
/// advance the coverage marker. Every doubt (missing/old sidecar, index or
/// shard mismatch, versioning absent, NULL/future/non-monotonic marker,
/// purged history) falls back to FULL with a logged reason.
pub fn run_delta(config: &RunConfig) -> Result<Stats, String> {
    delta::log_prefix_warning();

    let manifest = match preflight(config)? {
        Pre::Manifest(manifest) => manifest,
        Pre::Full(reason) => return run_full_with_reason(config, Some(reason)),
    };

    // advisory A4: a manifest carrying an unknown timestamp flavour must not run
    // the delta (a future flavour's window semantics are not known). A
    // marker-less v3 manifest is the adoption state and is fine.
    if let Some(flavour) = manifest.versioning_flavour.as_deref() {
        if flavour != delta::FLAVOUR {
            return run_full_with_reason(
                config,
                Some(format!("unknown versioning flavour {flavour:?}")),
            );
        }
    }

    // Probe versioning presence BEFORE any `FOR SYSTEM_TIME` query (A1):
    // an unversioned table would error `Unknown column 'ROW_START'`.
    let mut conn = delta::connect(&config.sql_uri)?;
    if !delta::versioning_probe(&mut conn)? {
        return run_full_with_reason(config, Some("versioning absent".to_string()));
    }
    // Capture the new marker BEFORE any candidate query (design §1.3).
    let new_marker = delta::capture_now6(&mut conn)?;
    let oldest = delta::oldest_history(&mut conn)?;
    let floor = match delta::decide(
        manifest.versioning_marker.as_deref(),
        &new_marker,
        true,
        oldest.as_ref(),
    ) {
        delta::Startup::Full(reason) => return run_full_with_reason(config, Some(reason)),
        delta::Startup::Delta { floor, .. } => floor,
    };
    let candidates = delta::scan(&mut conn, &floor, &new_marker)?;
    drop(conn);

    let marker_from = manifest.versioning_marker.clone();
    let marker_to = Some(new_marker.as_str().to_string());
    let candidate_counts: BTreeMap<String, usize> = candidates
        .per_table
        .iter()
        .map(|(table, count)| (table.to_string(), *count))
        .collect();

    // No candidates: the window was scanned and nothing needs rebuilding,
    // so advance the marker only (an atomic manifest rewrite; the window
    // must not grow unboundedly).
    if candidates.datasets.is_empty() {
        let mut updated = manifest.clone();
        updated.version = sidecar::SIDECAR_VERSION;
        updated.mode = "delta".to_string();
        updated.versioning_marker = marker_to.clone();
        updated.versioning_flavour = Some(delta::FLAVOUR.to_string());
        updated.store(&config.index_path)?;
        return Ok(Stats {
            mode: "delta".to_string(),
            built: updated.doc_count,
            replaced: 0,
            deleted: 0,
            unchanged: updated.doc_count,
            index_doc_count: updated.doc_count as u32,
            duplicate_identity_terms: Vec::new(),
            max_live_documents: 0,
            shards_written: 0,
            shards_deleted: 0,
            max_resident_sidecar_entries: 0,
            max_resident_dataset_terms: 0,
            peak_open_shards: 0,
            shard_bytes_read: 0,
            commit_chunks: 0,
            chunk_rss_peak_kib: 0,
            startup: "delta".to_string(),
            candidates: candidate_counts,
            affected_datasets: 0,
            marker_from,
            marker_to,
        });
    }

    // A case-only dataset rename (or any two affected labels that differ
    // only by case) collapses in `identity_term` (which lowercases
    // `name:dataset`): the old label's shard terms are identical to the new
    // label's, so the leftover-delete path would delete the documents the
    // scoped crawl just wrote. FULL/INCREMENTAL treat such a pair as a hard
    // duplicate error; the delta must not silently diverge from FULL, so it
    // detects the collision and falls back to FULL (case-collision fallback).
    if let Some((a, b)) = delta::case_collision(&candidates.datasets) {
        return run_full_with_reason(
            config,
            Some(format!(
                "case-variant dataset labels in affected scope ({a:?} vs {b:?})"
            )),
        );
    }

    let caches = RdfCaches::load(&config.rdf_cache)?;

    // B1: run the duplicate check, SCOPED, with the cross-dataset label map
    // PRE-SEEDED from the manifest labels that are NOT already in scope
    // (advisory A1). In-scope labels are covered by the crawl's own map (the
    // scoped crawl yields them and their per-dataset sets are checked);
    // seeding them too would false-positive on a case-only dataset rename
    // (the seed holds the old case, the crawl yields the new one). The seed
    // still catches an in-scope x out-of-scope case-variant collision. This
    // is the only guard against within-dataset identity-term collisions and
    // cross-dataset case-variant labels; skipping it would let the delta
    // diverge from a fresh FULL.
    let (duplicates, max_dataset_terms) = duplicate_prepass_scoped(
        &config.sql_uri,
        config.publish_filter,
        Some(&candidates.datasets),
        seed_labels_excluding_scope(manifest.shards.keys(), &candidates.datasets),
    )?;
    if !duplicates.is_empty() {
        return Err(format!(
            "duplicate identity terms in affected datasets: {}",
            duplicates.join(", ")
        ));
    }

    livemem::reset();
    sidecar::mem::reset();
    let retain = livemem::red_retain();

    let db = WritableDatabase::open(
        &config.index_path,
        Some(DbAction::CreateOrOpen),
        Some(DbBackend::Glass),
        None,
        None,
    );
    let shard_dir = sidecar::shard_dir(&config.index_path);
    let by_label: HashMap<String, ShardEntry> = manifest
        .shards
        .iter()
        .map(|(label, entry)| (label.clone(), entry.clone()))
        .collect();

    let mut build = SidecarBuild::new(
        &config.index_path,
        "delta",
        caches.rif.hash.clone(),
        caches.wiki.hash.clone(),
        marker_to.clone(),
        Some(delta::FLAVOUR.to_string()),
    );
    // Carry forward every existing shard entry; affected datasets replace
    // or remove theirs below.
    for (label, entry) in &manifest.shards {
        build.keep(label, entry.clone());
    }

    let mut sink = IncrementalSink {
        db,
        caches: &caches,
        red_hash: false,
        red_delete: false,
        no_recovery: false,
        retain,
        kept: Vec::new(),
        pacer: CommitPacer::new(false),
        build: &mut build,
        shard_dir,
        by_label,
        affected_scope: Some(candidates.datasets.clone()),
        seen: BTreeSet::new(),
        previous: None,
        previous_label: String::new(),
        dataset_terms: BTreeSet::new(),
        dataset_map: BTreeMap::new(),
        retainer: sidecar::ResidentRetainer::new(),
        replaced: 0,
        deleted: 0,
        unchanged: 0,
        max_dataset_terms: 0,
        new_shards: 0,
        removed_shards: 0,
    };

    let result = rows::stream_dataset_groups_scoped(
        &config.sql_uri,
        config.publish_filter,
        Some(&candidates.datasets),
        &mut sink,
    );
    if let Err(error) = result {
        if sink.pacer.open {
            sink.db.cancel_transaction();
        }
        return Err(error);
    }

    // Affected datasets the scoped crawl never produced: every one of
    // their documents is gone (a withdrawn dataset). Out-of-scope shards
    // are never touched (`affected_scope`).
    let leftovers = sink.leftover_labels();
    for label in leftovers {
        sink.delete_dataset(&label)?;
        sink.removed_shards += 1;
    }

    sink.pacer.finish(&mut sink.db)?;
    let doc_count = sink.db.doc_count();
    let replaced = sink.replaced;
    let deleted = sink.deleted;
    let unchanged = sink.unchanged;
    let max_resident = sidecar::mem::peak_resident_entries();
    let peak_open = sidecar::mem::peak_open_shards();
    let bytes_read = sidecar::mem::bytes_read();
    let shards_written = sink.new_shards;
    let shards_deleted = sink.removed_shards;
    let sink_dataset_terms = sink.max_dataset_terms;
    let commit_chunks = sink.pacer.chunks;
    let chunk_rss_peak_kib = sink.pacer.rss_peak_kib;
    let max_live = livemem::max_live();
    drop(sink);

    let built = build.total_count();
    // Always finalize: even an idempotent re-examination must advance the
    // marker (`marker_to`), which is written in the manifest LAST.
    build.finalize()?;

    Ok(Stats {
        mode: "delta".to_string(),
        built,
        replaced,
        deleted,
        unchanged,
        index_doc_count: doc_count,
        duplicate_identity_terms: Vec::new(),
        max_live_documents: max_live,
        shards_written,
        shards_deleted,
        max_resident_sidecar_entries: max_resident,
        max_resident_dataset_terms: max_dataset_terms.max(sink_dataset_terms),
        peak_open_shards: peak_open,
        shard_bytes_read: bytes_read,
        commit_chunks,
        chunk_rss_peak_kib,
        startup: "delta".to_string(),
        candidates: candidate_counts,
        affected_datasets: candidates.datasets.len(),
        marker_from,
        marker_to,
    })
}

/// Where the sidecar lives for an index path.
pub fn sidecar_location(index_path: &Path) -> PathBuf {
    sidecar::sidecar_path(index_path)
}

/// The document type of an identity term (for reporting).
pub fn idterm_doctype(idterm: &str) -> &str {
    if idterm.starts_with("Qgene:") {
        schema::DOCTYPE_GENE
    } else {
        schema::DOCTYPE_PHENOTYPE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two dataset labels that differ only by case lowercase to the same
    /// `<dataset>` in `identity_term`, so their documents collide in the
    /// index's identity term space. The per-dataset `set` is cleared at
    /// every `begin_dataset`, so only the cross-dataset label map can catch
    /// this; it must flag the pair.
    #[test]
    fn duplicate_sink_flags_case_variant_dataset_labels() {
        let mut sink = DuplicateSink::new();
        sink.begin_dataset("FooPublish").unwrap();
        sink.begin_dataset("foopublish").unwrap();
        assert_eq!(
            sink.duplicates.len(),
            1,
            "duplicates = {:?}",
            sink.duplicates
        );
        let message = &sink.duplicates[0];
        assert!(message.contains("FooPublish") && message.contains("foopublish"));
        assert!(message.contains("cross-dataset"), "{message}");
    }

    /// Distinct labels, and a label the crawl repeats verbatim across the
    /// gene and phenotype streams, are not cross-dataset collisions.
    #[test]
    fn duplicate_sink_accepts_distinct_and_repeated_labels() {
        let mut sink = DuplicateSink::new();
        sink.begin_dataset("BXDPublish").unwrap();
        sink.begin_dataset("GITrMetPublish").unwrap();
        sink.begin_dataset("bxdpublish2").unwrap();
        sink.begin_dataset("BXDPublish").unwrap();
        assert!(
            sink.duplicates.is_empty(),
            "duplicates = {:?}",
            sink.duplicates
        );
    }

    /// A label pre-seeded from the manifest still collides with a
    /// case-variant seen in the scoped crawl (B1: the guard must reach
    /// out-of-scope shards).
    #[test]
    fn duplicate_sink_seeded_labels_catch_case_variants() {
        let mut sink = DuplicateSink::new();
        sink.seed_labels(["FooPublish".to_string()]);
        sink.begin_dataset("foopublish").unwrap();
        assert_eq!(sink.duplicates.len(), 1, "{:?}", sink.duplicates);
        // The same label verbatim is not a collision.
        let mut sink = DuplicateSink::new();
        sink.seed_labels(["BXDPublish".to_string()]);
        sink.begin_dataset("BXDPublish").unwrap();
        assert!(sink.duplicates.is_empty(), "{:?}", sink.duplicates);
    }

    /// advisory A1: the duplicate-check seed excludes already-in-scope labels,
    /// so a case-only rename (manifest old case, crawl new case) does not
    /// false-positive; out-of-scope labels are still seeded.
    #[test]
    fn seed_excludes_in_scope_labels() {
        let manifest: BTreeSet<String> = ["bxd", "KeepOut", "gone"]
            .into_iter()
            .map(String::from)
            .collect();
        let scope: BTreeSet<String> = ["bxd", "BXD", "gone"]
            .into_iter()
            .map(String::from)
            .collect();
        let mut seed = seed_labels_excluding_scope(manifest.iter(), &scope);
        seed.sort();
        assert_eq!(seed, vec!["KeepOut"]);
    }

    /// The DELTA leftover path may only name in-scope datasets; without a
    /// scope it is the full INCREMENTAL leftover set.
    #[test]
    fn leftover_labels_respect_the_affected_scope() {
        let labels: BTreeSet<String> = ["A", "B", "C", "D"].into_iter().map(String::from).collect();
        let seen: BTreeSet<String> = ["B"].into_iter().map(String::from).collect();

        // INCREMENTAL (no scope): every label the crawl did not see.
        let mut leftovers = leftover_labels_in_scope(labels.iter(), &seen, None);
        leftovers.sort();
        assert_eq!(leftovers, vec!["A", "C", "D"]);

        // DELTA (scope = {A, C}): only in-scope leftovers; D, though absent
        // from the crawl, is untouched because it is out of scope.
        let scope: BTreeSet<String> = ["A", "C"].into_iter().map(String::from).collect();
        let mut leftovers = leftover_labels_in_scope(labels.iter(), &seen, Some(&scope));
        leftovers.sort();
        assert_eq!(leftovers, vec!["A", "C"]);
    }
}
