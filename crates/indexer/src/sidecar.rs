//! The incremental sidecar, sharded by dataset (format version 3).
//!
//! A small **manifest** at `<index>.sidecar.json` (version 3) holds the
//! run metadata and one entry per dataset shard:
//!
//! ```text
//! { version, mode, doc_count, rif_hash, wiki_hash,
//!   versioning_marker, versioning_flavour,
//!   shards: { "<dataset>": { file, count, content_sha256 } } }
//! ```
//!
//! `versioning_marker` is the DB `NOW(6)` coverage point for the delta
//! path (`src/indexer/delta.rs`), and `versioning_flavour` guards a
//! future change of timestamp flavour. Both are `#[serde(default)]` so a
//! marker-less v3 manifest parses as the "adoption" state (versioning
//! not yet observed); the strict version gate still rejects v1/v2.
//!
//! Each shard is a JSON map `identity-term -> source-hash` at
//! `<index>.sidecar.d/<file>`, where the filename is the dataset label
//! sanitized for the filesystem plus a 64-bit digest of the raw label (so
//! two labels that sanitize to the same string still get distinct files).
//!
//! Why sharding: the per-dataset shard is the unit the incremental diff
//! loads, so the resident idterm->hash map is O(largest shard), not
//! O(documents). The manifest is sharded so a crawl can rewrite one
//! dataset without touching the rest.
//!
//! Integrity and atomicity:
//!
//! * a shard's `content_sha256` is **recomputed from the shard file** at
//!   startup (bit-rot detection) — it is never blindly carried forward;
//! * a shard is **verified before it is parsed**, and a hash mismatch or
//!   an unparseable shard routes to the full-rebuild path — a corrupt
//!   shard is never swallowed as an empty one (which would silently drop
//!   its documents);
//! * changed shards are written temp + fsync + rename + directory fsync,
//!   and the manifest is written the same way **last**, so a crash leaves
//!   the previous, self-consistent sidecar in place (stray temp files are
//!   ignored and cleaned).
//!
//! An old version-1 monolithic sidecar (one `documents` map, no
//! `shards`) does not deserialize as a manifest, so it triggers FULL.

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use serde::{Deserialize, Serialize};

use super::sha256::{Sha256, hex_digest};

/// Sidecar format version. A manifest with any other version is ignored
/// and the run falls back to FULL (so a version-1 monolithic file, which
/// additionally fails to deserialize, is never mis-parsed).
pub const SIDECAR_VERSION: u32 = 3;

/// The shard directory suffix: `<index>.sidecar.d`.
const SHARD_DIR_SUFFIX: &str = ".sidecar.d";

/// One shard's manifest entry.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ShardEntry {
    /// Filename inside the shard directory.
    pub file: String,
    /// Document count of the shard.
    pub count: usize,
    /// SHA-256 of the shard file's bytes.
    pub content_sha256: String,
}

/// The on-disk manifest.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub version: u32,
    /// How the sidecar was last written (`full` or `incremental`).
    pub mode: String,
    /// Document count of the index the sidecar describes.
    pub doc_count: usize,
    /// Monolithic RIF cache hash at the time of writing.
    pub rif_hash: String,
    /// Monolithic wiki cache hash at the time of writing.
    pub wiki_hash: String,
    /// The schema this index was written with (prefixes, terms, slots), and the
    /// digest of that table. A reader compares it before it answers.
    /// `None` only for a manifest written before the stamp existed.
    #[serde(default)]
    pub schema_version: Option<u32>,
    #[serde(default)]
    pub schema_hash: Option<String>,
    /// When this manifest was finalised, so a reader can report the index's age.
    #[serde(default)]
    pub built_at: Option<String>,
    /// DB `NOW(6)` of the run whose results were committed (the delta
    /// coverage point, format `YYYY-MM-DD HH:MM:SS.ffffff`). `None` when
    /// system versioning was absent at the time of the run.
    #[serde(default)]
    pub versioning_marker: Option<String>,
    /// Timestamp flavour guard (`"timestamp6"`); `None` when no marker.
    #[serde(default)]
    pub versioning_flavour: Option<String>,
    /// Dataset label -> shard entry.
    pub shards: BTreeMap<String, ShardEntry>,
}

/// The manifest path for an index directory (`<index>.sidecar.json`).
pub fn sidecar_path(index_path: &Path) -> PathBuf {
    PathBuf::from(format!(
        "{}.sidecar.json",
        index_path.to_string_lossy().trim_end_matches('/')
    ))
}

/// The shard directory for an index directory (`<index>.sidecar.d`).
pub fn shard_dir(index_path: &Path) -> PathBuf {
    PathBuf::from(format!(
        "{}{}",
        index_path.to_string_lossy().trim_end_matches('/'),
        SHARD_DIR_SUFFIX
    ))
}

/// Sanitize a dataset label into a filesystem-safe stem: every byte
/// outside `[A-Za-z0-9._-]` becomes `_`, leading dots are stripped, and
/// an empty result becomes `dataset`.
pub fn sanitize_label(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    for ch in label.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    while out.starts_with('.') {
        out.remove(0);
    }
    if out.is_empty() {
        out.push_str("dataset");
    }
    out
}

/// The shard filename for a dataset label: the sanitized stem plus a
/// 64-bit digest of the raw label, so distinct labels cannot collide.
/// The digest is truncated to 16 hex chars (64 bits) — long enough that
/// two labels sharing a sanitized stem cannot silently merge into one
/// shard file.
pub fn shard_filename(label: &str) -> String {
    let stem = sanitize_label(label);
    let digest = hex_digest(label.as_bytes());
    format!("{}-{}.json", stem, &digest[..16])
}

impl Manifest {
    /// Load and validate the manifest. `None` when the file is missing,
    /// unreadable, fails to deserialize (an old version-1 file, or any
    /// other shape), or carries a version other than [`SIDECAR_VERSION`].
    pub fn load(index_path: &Path) -> Option<Manifest> {
        let text = std::fs::read_to_string(sidecar_path(index_path)).ok()?;
        let manifest: Manifest = serde_json::from_str(&text).ok()?;
        if manifest.version != SIDECAR_VERSION {
            return None;
        }
        Some(manifest)
    }

    /// Atomically replace the manifest (temp + fsync + rename + dir
    /// fsync). Callers write changed shards first; the manifest is last.
    pub fn store(&self, index_path: &Path) -> Result<(), String> {
        let path = sidecar_path(index_path);
        let json = serde_json::to_string(self).map_err(|e| format!("manifest encode: {e}"))?;
        atomic_write(&path, json.as_bytes())
    }
}

/// Write `bytes` to `path` atomically: sibling temp file, fsync the file,
/// rename over the target, then fsync the containing directory.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temp = PathBuf::from(format!("{}.tmp.{}", path.display(), std::process::id()));
    {
        let mut file =
            File::create(&temp).map_err(|e| format!("create {}: {e}", temp.display()))?;
        file.write_all(bytes)
            .map_err(|e| format!("write {}: {e}", temp.display()))?;
        file.sync_all()
            .map_err(|e| format!("fsync {}: {e}", temp.display()))?;
    }
    std::fs::rename(&temp, path).map_err(|e| format!("rename {}: {e}", path.display()))?;
    fsync_dir(path.parent().unwrap_or_else(|| Path::new(".")))
}

/// fsync a directory so a rename into it is durable.
pub fn fsync_dir(dir: &Path) -> Result<(), String> {
    let handle = File::open(dir).map_err(|e| format!("open dir {}: {e}", dir.display()))?;
    handle
        .sync_all()
        .map_err(|e| format!("fsync dir {}: {e}", dir.display()))
}

/// Stage a changed shard: serialize `map`, write it to a temp file in
/// `dir`, fsync it, and return the temp path plus the manifest entry it
/// will become after [`finalize_shard`]. The temp is **not** renamed yet,
/// so a crash mid-run leaves the previous sidecar untouched.
pub fn stage_shard(
    dir: &Path,
    label: &str,
    map: &BTreeMap<String, String>,
) -> Result<(PathBuf, ShardEntry), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("create shard dir {}: {e}", dir.display()))?;
    let file = shard_filename(label);
    let temp = dir.join(format!(".tmp-{}-{}", std::process::id(), file));
    let json = serde_json::to_vec(map).map_err(|e| format!("shard encode: {e}"))?;
    {
        let mut handle =
            File::create(&temp).map_err(|e| format!("create shard {}: {e}", temp.display()))?;
        handle
            .write_all(&json)
            .map_err(|e| format!("write shard {}: {e}", temp.display()))?;
        handle
            .sync_all()
            .map_err(|e| format!("fsync shard {}: {e}", temp.display()))?;
    }
    let entry = ShardEntry {
        file,
        count: map.len(),
        content_sha256: hex_digest(&json),
    };
    Ok((temp, entry))
}

/// Rename a staged shard temp into place and fsync the shard directory.
pub fn finalize_shard(temp: &Path, dir: &Path, entry: &ShardEntry) -> Result<(), String> {
    let target = dir.join(&entry.file);
    std::fs::rename(temp, &target)
        .map_err(|e| format!("rename shard {}: {e}", target.display()))?;
    fsync_dir(dir)
}

/// Why a shard was rejected at verification time.
#[derive(Debug)]
pub enum ShardError {
    /// The shard file could not be read.
    Read(String),
    /// The shard file's bytes hash to something other than its manifest
    /// `content_sha256`.
    HashMismatch { expected: String, actual: String },
    /// The shard file hashed correctly but did not parse. Treated exactly
    /// like a hash mismatch so a corrupt shard can never be read as empty.
    Unparseable(String),
}

impl std::fmt::Display for ShardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShardError::Read(e) => write!(f, "read: {e}"),
            ShardError::HashMismatch { expected, actual } => {
                write!(
                    f,
                    "content_sha256 mismatch (manifest {expected}, file {actual})"
                )
            }
            ShardError::Unparseable(e) => write!(f, "unparseable: {e}"),
        }
    }
}

/// Read a shard file's bytes and check them against `entry.content_sha256`
/// **without parsing**. This is the startup bit-rot pass: an unchanged
/// shard's hash is recomputed from the file, never trusted blindly.
pub fn verify_shard_bytes(dir: &Path, entry: &ShardEntry) -> Result<Vec<u8>, ShardError> {
    let path = dir.join(&entry.file);
    let bytes =
        std::fs::read(&path).map_err(|e| ShardError::Read(format!("{}: {e}", path.display())))?;
    mem::add_bytes_read(bytes.len() as u64);
    let actual = hex_digest(&bytes);
    if actual != entry.content_sha256 {
        return Err(ShardError::HashMismatch {
            expected: entry.content_sha256.clone(),
            actual,
        });
    }
    Ok(bytes)
}

/// Verify a shard and then parse it. The parse is only attempted after
/// the hash matches, and a parse failure is surfaced as a rejection
/// (routing to full) rather than as an empty shard.
pub fn read_verified_shard(
    dir: &Path,
    entry: &ShardEntry,
) -> Result<BTreeMap<String, String>, ShardError> {
    let bytes = verify_shard_bytes(dir, entry)?;
    let map: BTreeMap<String, String> = serde_json::from_slice(&bytes)
        .map_err(|e| ShardError::Unparseable(format!("{}: {e}", entry.file)))?;
    Ok(map)
}

/// A shard map whose entries are accounted as resident until dropped.
/// The high-water mark proves the diff loads O(largest shard), not all
/// shards: a RED that holds every shard alive makes the peak O(docs).
pub struct ResidentShard {
    /// Dataset label this shard belongs to.
    pub label: String,
    map: BTreeMap<String, String>,
}

impl ResidentShard {
    pub fn load(dir: &Path, label: &str, entry: &ShardEntry) -> Result<ResidentShard, ShardError> {
        let map = read_verified_shard(dir, entry)?;
        mem::note_load(map.len());
        Ok(ResidentShard {
            label: label.to_string(),
            map,
        })
    }

    pub fn map(&self) -> &BTreeMap<String, String> {
        &self.map
    }
}

impl Drop for ResidentShard {
    fn drop(&mut self) {
        mem::note_drop(self.map.len());
    }
}

/// Process-global observability counters for the sharded sidecar. They
/// exist so "load-all" cannot quietly pass: the run asserts
/// `max_resident_entries` is O(largest shard).
pub mod mem {
    use super::{AtomicUsize, Ordering};

    static RESIDENT: AtomicUsize = AtomicUsize::new(0);
    static PEAK_RESIDENT: AtomicUsize = AtomicUsize::new(0);
    static BYTES_READ: AtomicUsize = AtomicUsize::new(0);
    static OPEN_SHARDS: AtomicUsize = AtomicUsize::new(0);
    static PEAK_OPEN: AtomicUsize = AtomicUsize::new(0);

    /// Reset all counters before a run.
    pub fn reset() {
        RESIDENT.store(0, Ordering::SeqCst);
        PEAK_RESIDENT.store(0, Ordering::SeqCst);
        BYTES_READ.store(0, Ordering::SeqCst);
        OPEN_SHARDS.store(0, Ordering::SeqCst);
        PEAK_OPEN.store(0, Ordering::SeqCst);
    }

    pub(super) fn note_load(entries: usize) {
        let resident = RESIDENT.fetch_add(entries, Ordering::SeqCst) + entries;
        PEAK_RESIDENT.fetch_max(resident, Ordering::SeqCst);
        let open = OPEN_SHARDS.fetch_add(1, Ordering::SeqCst) + 1;
        PEAK_OPEN.fetch_max(open, Ordering::SeqCst);
    }

    pub(super) fn note_drop(entries: usize) {
        RESIDENT.fetch_sub(entries, Ordering::SeqCst);
        OPEN_SHARDS.fetch_sub(1, Ordering::SeqCst);
    }

    /// Count bytes read from shard files, whether or not parsed.
    pub fn add_bytes_read(bytes: u64) {
        BYTES_READ.fetch_add(bytes as usize, Ordering::SeqCst);
    }

    /// High-water mark of simultaneously resident shard-map entries.
    pub fn peak_resident_entries() -> usize {
        PEAK_RESIDENT.load(Ordering::SeqCst)
    }

    /// High-water mark of simultaneously open shards.
    pub fn peak_open_shards() -> usize {
        PEAK_OPEN.load(Ordering::SeqCst)
    }

    /// Cumulative shard-file bytes read.
    pub fn bytes_read() -> usize {
        BYTES_READ.load(Ordering::SeqCst)
    }
}

/// Keep an already-loaded shard resident (the "load-all" RED). Holding
/// every shard alive drives the peak-resident counter to O(docs).
pub struct ResidentRetainer {
    held: Vec<ResidentShard>,
}

impl ResidentRetainer {
    pub fn new() -> Self {
        ResidentRetainer { held: Vec::new() }
    }

    pub fn retain(&mut self, shard: ResidentShard) {
        self.held.push(shard);
    }
}

impl Default for ResidentRetainer {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether to hold every loaded shard alive (the load-all RED control).
pub fn red_load_all() -> bool {
    std::env::var("GN4_INDEXER_RED_SIDECAR").as_deref() == Ok("load-all")
}

/// Remove stray temp files a previous crash may have left: staged shard
/// temps and stage directories, manifest temps, and half-written index
/// temp/backup directories. They are never read.
pub fn cleanup_temp(index_path: &Path) -> Result<(), String> {
    let dir = shard_dir(index_path);
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with(".tmp-") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    let parent = index_path.parent().unwrap_or_else(|| Path::new("."));
    let manifest = sidecar_path(index_path);
    let stem = manifest
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let shard_stem = format!(
        "{}.tmp.",
        dir.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
    );
    let index_stem = index_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    if let Ok(entries) = std::fs::read_dir(parent) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(&format!("{stem}.tmp.")) || name.starts_with(&shard_stem) {
                let path = entry.path();
                if path.is_dir() {
                    let _ = std::fs::remove_dir_all(&path);
                } else {
                    let _ = std::fs::remove_file(&path);
                }
            }
            // Left-over FULL index temp dirs and backups from a crash.
            if name.starts_with(&format!(".{index_stem}.tmp."))
                || name.starts_with(&format!(".{index_stem}.old."))
            {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    }
    Ok(())
}

/// A SHA-256 helper mirroring the sidecar's hashing of a shard file, for
/// callers that want to (re)compute a manifest hash.
pub fn shard_content_hash(map: &BTreeMap<String, String>) -> String {
    let json = serde_json::to_vec(map).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(&json);
    hex_digest(&json)
}

/// Open a temp file for staging without rewriting an existing one.
#[allow(dead_code)]
fn open_new(path: &Path) -> Result<File, String> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("create {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shard filename carries a 64-bit (16 hex char) digest of the raw
    /// label, so two distinct labels that sanitize to the same stem get
    /// distinct files instead of silently merging.
    #[test]
    fn shard_filename_carries_64_bit_digest() {
        let a = shard_filename("Foo/Publish");
        let b = shard_filename("Foo_Publish");
        assert!(a.ends_with(".json") && b.ends_with(".json"));
        let digest_a = a.trim_end_matches(".json");
        let digest_a = digest_a.rsplit('-').next().unwrap();
        assert_eq!(digest_a.len(), 16, "filename = {a}");
        assert!(digest_a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b, "distinct labels must not share a shard file");
    }

    /// A pre-bump v2 manifest (no `versioning_marker`/`versioning_flavour`)
    /// deserializes but is rejected by the strict version gate, so it can
    /// never steer a delta.
    #[test]
    fn load_rejects_v2_manifest() {
        let index = temp_index_path("v2-reject");
        let v2 = r#"{
          "version": 2,
          "mode": "incremental",
          "doc_count": 3,
          "rif_hash": "aa",
          "wiki_hash": "bb",
          "shards": {}
        }"#;
        std::fs::write(sidecar_path(&index), v2).unwrap();
        assert!(Manifest::load(&index).is_none(), "v2 must route to FULL");
        cleanup(&index);
    }

    /// A v3 manifest round-trips with its marker/flavour, and a v3
    /// manifest written before versioning was observed (fields absent)
    /// parses as the marker-less adoption state.
    #[test]
    fn v3_manifest_round_trips_and_adopts_missing_marker() {
        let index = temp_index_path("v3-roundtrip");
        let manifest = Manifest {
            version: SIDECAR_VERSION,
            mode: "delta".to_string(),
            doc_count: 1,
            rif_hash: "aa".to_string(),
            wiki_hash: "bb".to_string(),
              schema_version: Some(gn_search_schema::SCHEMA_VERSION),
              schema_hash: Some(gn_search_schema::schema_digest()),
              built_at: Some("2026-11-16T12:00:00Z".to_string()),
            versioning_marker: Some("2026-10-13 19:37:47.469634".to_string()),
            versioning_flavour: Some("timestamp6".to_string()),
            shards: BTreeMap::new(),
        };
        manifest.store(&index).unwrap();
        let loaded = Manifest::load(&index).expect("v3 must load");
        assert_eq!(loaded, manifest);

        let no_marker = r#"{
          "version": 3,
          "mode": "full",
          "doc_count": 1,
          "rif_hash": "aa",
          "wiki_hash": "bb",
          "shards": {}
        }"#;
        std::fs::write(sidecar_path(&index), no_marker).unwrap();
        let loaded = Manifest::load(&index).expect("marker-less v3 must load");
        assert_eq!(loaded.versioning_marker, None);
        assert_eq!(loaded.versioning_flavour, None);
        cleanup(&index);
    }

    /// A unique scratch index path for a self-contained manifest test.
    fn temp_index_path(tag: &str) -> PathBuf {
        let pid = std::process::id();
        std::env::temp_dir().join(format!("gn4-sidecar-test-{tag}-{pid}"))
    }

    fn cleanup(index: &Path) {
        let _ = std::fs::remove_file(sidecar_path(index));
    }
}
