//! The sidecar manifest reader.
//!
//! SCOUT sections implemented: SCOUT-INDEXER §3 (the v3 manifest and its
//! shard map) and §8 (the freshness fields a v4 manifest adds). Four cases
//! matter to a consumer: a v3 manifest loads, a missing manifest is not an
//! error, a v4-ish manifest with `build_id`/`built_at` loads with unknown
//! fields ignored, and a manifest of a version this reader does not know is
//! treated as absent rather than guessed at.

use std::path::{Path, PathBuf};

use gn_search_engine::manifest::{self, MANIFEST_VERSION_V3, MANIFEST_VERSION_V4};
use gn_search_engine::SearchError;

/// A scratch index path (`<dir>/index`, so its sidecar is
/// `<dir>/index.sidecar.json`), unique to this test.
fn scratch_index(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("gn-search-manifest-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create the scratch directory");
    dir.join("index")
}

/// Write `manifest_json` as the sidecar of `index_path`.
fn write_sidecar(index_path: &Path, manifest_json: &str) {
    let path = manifest::sidecar_path(index_path.to_str().expect("utf-8 temp path"));
    std::fs::write(&path, manifest_json).expect("write the sidecar");
}

fn read(index_path: &Path) -> Option<gn_search_engine::manifest::Manifest> {
    manifest::read(index_path.to_str().expect("utf-8 temp path"))
        .expect("reading a well-formed sidecar cannot fail")
}

/// The v3 manifest GN4's indexer writes (SCOUT-INDEXER §3). The numbers are
/// ones the reports quote (the recorded Q02 match set, and BXDPublish's
/// trait count); the hashes are placeholders.
const V3: &str = r#"{
  "version": 3,
  "mode": "full",
  "doc_count": 57632,
  "rif_hash": "aaaa",
  "wiki_hash": "bbbb",
  "versioning_marker": "2026-11-13 10:11:12.000000",
  "versioning_flavour": "timestamp6",
  "shards": {
    "BXDPublish": {
      "file": "BXDPublish-0123456789abcdef.json",
      "count": 15060,
      "content_sha256": "cccc"
    }
  }
}"#;

/// A v3 manifest loads with every field it carries (SCOUT-INDEXER §3).
#[test]
fn v3_manifest_loads() {
    let index_path = scratch_index("v3");
    write_sidecar(&index_path, V3);
    let manifest = read(&index_path).expect("a v3 manifest loads");

    assert_eq!(manifest.version, MANIFEST_VERSION_V3);
    assert_eq!(manifest.mode, "full");
    assert_eq!(manifest.doc_count, 57632);
    assert_eq!(manifest.rif_hash, "aaaa");
    assert_eq!(manifest.wiki_hash, "bbbb");
    assert_eq!(
        manifest.versioning_marker.as_deref(),
        Some("2026-11-13 10:11:12.000000")
    );
    assert_eq!(manifest.versioning_flavour.as_deref(), Some("timestamp6"));
    // A v3 manifest has no build identity to report.
    assert_eq!(manifest.build_id, None);
    assert_eq!(manifest.built_at, None);
    let shard = manifest.shards.get("BXDPublish").expect("one shard");
    assert_eq!(shard.count, 15060);
    assert_eq!(shard.content_sha256, "cccc");
    assert_eq!(shard.file, "BXDPublish-0123456789abcdef.json");
}

/// A missing manifest is `Ok(None)`, never an error: freshness metadata
/// must not turn a working index into a failure (SYNTHESIS §5 step 4).
#[test]
fn missing_manifest_is_not_an_error() {
    let index_path = scratch_index("missing");
    assert_eq!(read(&index_path), None);
    assert_eq!(
        manifest::age_seconds(index_path.to_str().expect("utf-8")),
        None
    );
}

/// A manifest that already carries `build_id`/`built_at` loads, and fields
/// this crate does not model (`source_revision`) are ignored — the format
/// grows by adding fields.
#[test]
fn v4_manifest_with_build_identity_loads() {
    let index_path = scratch_index("v4");
    let v4 = r#"{
      "version": 4,
      "build_id": "01JD3ZK9V8T0R5C2M6H7Q4W1BX",
      "built_at": "2026-11-13T10:11:12Z",
      "mode": "full",
      "doc_count": 57632,
      "rif_hash": "aaaa",
      "wiki_hash": "bbbb",
      "source_revision": "a89781c",
      "shards": {}
    }"#;
    write_sidecar(&index_path, v4);
    let manifest = read(&index_path).expect("a v4 manifest loads");

    assert_eq!(manifest.version, MANIFEST_VERSION_V4);
    assert_eq!(
        manifest.build_id.as_deref(),
        Some("01JD3ZK9V8T0R5C2M6H7Q4W1BX")
    );
    assert_eq!(manifest.built_at.as_deref(), Some("2026-11-13T10:11:12Z"));
    // The unmodelled field is dropped, not fatal.
    assert_eq!(manifest.doc_count, 57632);
    assert!(manifest.shards.is_empty());

    // Age comes from the file, not from the timestamp string: the sidecar
    // was written just now, so the age is small and present.
    let age = manifest::age_seconds(index_path.to_str().expect("utf-8"))
        .expect("a manifest that exists has an age");
    assert!(age < 60, "freshly written sidecar, got {age}s");
}

/// A version this reader does not understand is treated exactly like a
/// missing manifest — and by VERSION, not by shape: this body would not
/// even parse as a v3 manifest (no hashes, no shards).
#[test]
fn unknown_version_is_treated_as_absent() {
    let index_path = scratch_index("v5");
    write_sidecar(
        &index_path,
        r#"{"version": 5, "doc_count": 57632, "something_new": {"a": 1}}"#,
    );
    assert_eq!(read(&index_path), None);
}

/// Bytes that are not JSON are reported, not swallowed: a corrupt sidecar
/// is a build problem worth seeing (502 `backend_misbehaving` on GN4's
/// mapping).
#[test]
fn corrupt_manifest_is_reported() {
    let index_path = scratch_index("corrupt");
    write_sidecar(&index_path, "{not json");
    match manifest::read(index_path.to_str().expect("utf-8")) {
        Err(SearchError::Misbehaving(message)) => {
            assert!(message.contains("is not JSON"), "got {message:?}")
        }
        other => panic!("expected Misbehaving, got {other:?}"),
    }
}

/// A JSON document with no version at all is not a manifest either.
#[test]
fn versionless_manifest_is_reported() {
    let index_path = scratch_index("noversion");
    write_sidecar(&index_path, r#"{"doc_count": 1}"#);
    match manifest::read(index_path.to_str().expect("utf-8")) {
        Err(SearchError::Misbehaving(message)) => {
            assert!(message.contains("no numeric version"), "got {message:?}")
        }
        other => panic!("expected Misbehaving, got {other:?}"),
    }
}

/// The sidecar path derivation trims a trailing slash, so an index path
/// given as a directory URL-ified with a slash still finds its manifest
/// (GN4's own derivation, SCOUT-INDEXER §3).
#[test]
fn sidecar_path_trims_a_trailing_slash() {
    assert_eq!(
        manifest::sidecar_path("/var/genenetwork/xapian/"),
        "/var/genenetwork/xapian.sidecar.json"
    );
    assert_eq!(
        manifest::sidecar_path("/var/genenetwork/xapian"),
        "/var/genenetwork/xapian.sidecar.json"
    );
}
