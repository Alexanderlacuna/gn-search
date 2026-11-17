//! The sidecar manifest: what the indexer says about the index it built.
//!
//! SCOUT sections implemented: SCOUT-INDEXER §3 (the on-disk format —
//! `<index>.sidecar.json`, format version 3, mode/doc_count/hashes plus one
//! entry per dataset shard, written LAST so a crash leaves the previous
//! self-consistent manifest) and §8 (the freshness fields a reader wants:
//! `build_id` and `built_at`, which v3 does not carry).
//!
//! The manifest is advisory. Nothing here can fail a search: a manifest
//! this reader cannot use is reported exactly like a missing one, because
//! freshness metadata must never turn a working index into an error
//! (SYNTHESIS §5 step 4: "old manifests -> absent, not an error"). The two
//! things that ARE errors are an unreadable file (an I/O failure, 502 on
//! GN4's mapping) and bytes that are not a manifest at all (a corrupt
//! sidecar is a build problem worth reporting, 502 `backend_misbehaving`).
//!
//! Why the version gate is authoritative: the version is read before any
//! field is, so a manifest from a newer indexer is ignored BY ITS VERSION
//! and not by whether its field set happens to still parse. A v4 manifest
//! (v3 plus `build_id`/`built_at`, SYNTHESIS §5 step 3) loads; a v3
//! manifest that already carries those two fields also loads, with the
//! fields simply populated; anything else is "no manifest". Unknown FIELDS
//! are always ignored, in every version — that is how the format grows.

use std::collections::BTreeMap;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::SearchError;

/// The manifest format GN4's indexer writes today (SCOUT-INDEXER §3).
pub const MANIFEST_VERSION_V3: u32 = 3;
/// The next format: v3 plus `build_id` and `built_at` (SYNTHESIS §5 step 3,
/// SCOUT-INDEXER §8).
pub const MANIFEST_VERSION_V4: u32 = 4;
/// The versions this reader understands. A manifest of any other version is
/// treated as absent (module doc).
pub const SUPPORTED_VERSIONS: [u32; 2] = [MANIFEST_VERSION_V3, MANIFEST_VERSION_V4];

/// One shard's manifest entry: where its diff map lives and what it hashes
/// to (SCOUT-INDEXER §3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShardEntry {
    /// Filename inside `<index>.sidecar.d`.
    pub file: String,
    /// Documents in the shard.
    pub count: u64,
    /// SHA-256 of the shard file, recomputed from the file by the indexer.
    pub content_sha256: String,
}

/// The manifest itself.
///
/// `u64` rather than GN4's `usize` for the counts, so the shape a client
/// sees does not depend on the pointer width of the machine that read it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Format version; see [`SUPPORTED_VERSIONS`].
    pub version: u32,
    /// How the sidecar was last written (`full`, `incremental`, `delta`).
    pub mode: String,
    /// Documents in the index this manifest describes.
    pub doc_count: u64,
    /// RIF dictionary hash at the time of writing.
    pub rif_hash: String,
    /// Wiki dictionary hash at the time of writing.
    pub wiki_hash: String,
    /// The schema the index was written with, and the digest of its table.
    ///
    /// Absent on indexes built before the stamp existed; a reader must treat
    /// "absent" as "unknown", not as "matches".
    #[serde(default)]
    pub schema_version: Option<u32>,
    #[serde(default)]
    pub schema_hash: Option<String>,
    /// DB `NOW(6)` coverage point of the run (the delta path's watermark,
    /// not a build time; SCOUT-INDEXER §5, §8).
    #[serde(default)]
    pub versioning_marker: Option<String>,
    /// Timestamp flavour guard for the marker (`timestamp6`).
    #[serde(default)]
    pub versioning_flavour: Option<String>,
    /// Unique id of the run that wrote this manifest (v4 only; the field
    /// v3 lacks — SCOUT-INDEXER §8).
    #[serde(default)]
    pub build_id: Option<String>,
    /// UTC wall-clock time the build completed (v4 only; SCOUT-INDEXER §8).
    /// Reported as the indexer wrote it: this reader does not parse
    /// timestamps (see [`age_seconds`] for the parsed alternative).
    #[serde(default)]
    pub built_at: Option<String>,
    /// Dataset label -> shard entry.
    pub shards: BTreeMap<String, ShardEntry>,
}

/// The manifest path for an index directory: `<index>.sidecar.json`, with
/// any trailing slash on the index path trimmed (GN4's own derivation,
/// SCOUT-INDEXER §3).
pub fn sidecar_path(index_path: &str) -> String {
    format!("{}.sidecar.json", index_path.trim_end_matches('/'))
}

/// Read the manifest of the index at `index_path`.
///
/// * a missing file is `Ok(None)` — an index built before the sidecar
///   existed, or one whose sidecar a FULL rebuild is about to replace;
/// * an unsupported version is `Ok(None)` as well (module doc);
/// * an unreadable file is [`SearchError::Unreachable`], and bytes that are
///   not a manifest are [`SearchError::Misbehaving`].
pub fn read(index_path: &str) -> Result<Option<Manifest>, SearchError> {
    let path = sidecar_path(index_path);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(SearchError::Unreachable(format!(
                "sidecar manifest at {path} could not be read: {error}"
            )))
        }
    };
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|error| {
        SearchError::Misbehaving(format!("sidecar manifest at {path} is not JSON: {error}"))
    })?;
    let version = value
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| {
            SearchError::Misbehaving(format!("sidecar manifest at {path} has no numeric version"))
        })?;
    if !SUPPORTED_VERSIONS
        .iter()
        .any(|supported| u64::from(*supported) == version)
    {
        return Ok(None);
    }
    let manifest: Manifest = serde_json::from_value(value).map_err(|error| {
        SearchError::Misbehaving(format!(
            "sidecar manifest at {path} is not a manifest: {error}"
        ))
    })?;
    Ok(Some(manifest))
}

/// The age of the index's manifest, in whole seconds: how long ago the
/// sidecar was written.
///
/// The sidecar's mtime is used rather than `built_at` because the manifest
/// is written LAST in every build (SCOUT-INDEXER §3), which makes its mtime
/// a lower bound on when the build completed — and because parsing a
/// timestamp by hand is a wrongness risk this crate does not need to take
/// (a client that wants the exact string reads [`Manifest::built_at`]).
///
/// `None` when there is no manifest, or when the clock is behind its mtime
/// (a skewed filesystem: better no age than a wrong one).
pub fn age_seconds(index_path: &str) -> Option<u64> {
    let modified = std::fs::metadata(sidecar_path(index_path))
        .ok()?
        .modified()
        .ok()?;
    SystemTime::now()
        .duration_since(modified)
        .ok()
        .map(|age| age.as_secs())
}
