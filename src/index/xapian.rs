//! The xapian backend: a STUB.
//!
//! SCOUT section implemented: SCOUT-NATIVE §2 (the pre-checks an index
//! open needs before the binding is called) and §8 (why the binding is not
//! here). Everything past the pre-checks — the query parser, the mset, the
//! stored-document decode — is NOT implemented, and this module stops with
//! a loud `unimplemented!` instead of pretending otherwise.
//!
//! Why a stub rather than a dependency: the binding GN4's native engine is
//! written against is `xapian_rs`, a fork of torrancew/xapian-rs pinned by
//! git rev, not a crates.io release, and the whole grammar depends on
//! fork-only API — `parse_query_checked`, the `NumberRangeProcessor` f64
//! bound fix, the decline→`MatchNothing` bridge, and a custom
//! `FieldProcessor` that `Box::leak`s (SCOUT-NATIVE §8). Pulling it in
//! would make this crate unbuildable anywhere that fork is not vendored,
//! so it is behind the `xapian` feature and stops here.
//!
//! The pre-checks are not decoration. The binding's `Database::open`
//! aborts the PROCESS on failure instead of returning an error, so every
//! failure an operator can hit has to be caught before the call: an unset
//! path, a path that cannot be stat'ed, a path that is not a directory, a
//! directory with no `iamglass` marker, and a directory whose marker is there
//! but whose `postlist.glass` is not — the 8 KB `iamglass`+`flintlock` shell
//! a half-compacted or half-deleted index leaves behind (SCOUT-NATIVE §2).
//! The messages below are GN4's, verbatim, because they are what operators
//! already see in a 500/502 body; the `postlist.glass` check is not GN4's and
//! says so.
//!
//! The module is gated once, by `#[cfg(feature = "xapian")]` on its
//! declaration (`src/index/mod.rs`); a second `#![cfg(...)]` here was
//! redundant and clippy flags it as a duplicated attribute.

use crate::grammar::ParsedQuery;
use crate::hit::SearchHit;
use crate::index::Index;
use crate::SearchError;

/// The xapian-backed index (unimplemented; see the module doc).
pub struct XapianIndex {
    /// The index directory an implementation would open.
    path: String,
}

impl Index for XapianIndex {
    fn open(path: &str) -> Result<Self, SearchError> {
        check_index_path(path)?;
        // Reaching here means the path looks like a glass index, so the
        // only thing missing is the binding itself.
        unimplemented!(
            "the xapian backend is a stub: no Rust xapian binding is wired into this crate. \
             The index at {path} passed the pre-checks; the next step is Database::open. \
             GN4 uses the `xapian_rs` fork (parse_query_checked, the NumberRangeProcessor \
             f64 fix, the decline->MatchNothing bridge, a custom FieldProcessor that \
             Box::leaks) — SCOUT-NATIVE §8 — so implementing this backend means wiring that \
             fork in as an optional dependency and filling in Index::search here."
        )
    }

    fn search(
        &self,
        _q: &ParsedQuery,
        _limit: u64,
        _page: u64,
    ) -> Result<Vec<SearchHit>, SearchError> {
        unimplemented!(
            "the xapian backend is a stub: Index::search against {path} is not implemented \
             (see this module's header; SCOUT-NATIVE §8 names the missing binding)",
            path = self.path
        )
    }
}

/// The checks that must pass before a binding is asked to open `path`
/// (SCOUT-NATIVE §2).
///
/// Order matters: an unset path is a local misconfiguration (500), while a
/// path that exists but is not a usable index is an unreachable backend
/// (502). GN4's mapping, not this crate's (SCOUT-NATIVE §6).
fn check_index_path(path: &str) -> Result<(), SearchError> {
    if path.is_empty() {
        return Err(SearchError::Internal(
            "XAPIAN_DB_PATH is not configured; set it to the xapian index directory to \
             use the native search engine (SEARCH_ENGINE=native)"
                .to_string(),
        ));
    }
    let metadata = std::fs::metadata(path).map_err(|error| {
        SearchError::Unreachable(format!(
            "xapian index at {path} could not be opened: {error}"
        ))
    })?;
    if !metadata.is_dir() {
        return Err(SearchError::Unreachable(format!(
            "xapian index at {path} is not a directory"
        )));
    }
    // The glass backend writes this marker into every index directory; it
    // is what distinguishes a real index from an arbitrary directory.
    if !std::path::Path::new(path).join("iamglass").is_file() {
        return Err(SearchError::Unreachable(format!(
            "xapian index at {path} does not look like a glass index (no iamglass marker)"
        )));
    }
    // But the marker alone is not an index: an aborted `xapian-compact`, or a
    // half-deleted database, leaves `iamglass` and `flintlock` behind with
    // every table gone — 8 KB directories that `xapian-delve` refuses to open
    // (`DatabaseOpeningError: Couldn't open …/postlist.glass`). `Database::open`
    // aborts the process on exactly that failure, so it is caught here. This
    // check is NOT in GN4's pre-checks (`native.rs:551-556`); it is the one
    // place this module is stricter, because the alternative is a process abort.
    if !std::path::Path::new(path).join("postlist.glass").is_file() {
        return Err(SearchError::Unreachable(format!(
            "xapian index at {path} has no postlist.glass: the iamglass marker is present but \
             the database is not openable (an interrupted compact or a partially deleted index)"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A scratch path unique to this process and this test.
    fn scratch(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("gn-search-xapian-{}-{name}", std::process::id()))
    }

    #[test]
    fn empty_path_is_a_misconfiguration() {
        let error = open_path("");
        assert!(
            matches!(&error, SearchError::Internal(message) if message.contains("XAPIAN_DB_PATH is not configured")),
            "unset path is Internal, got {error:?}"
        );
    }

    #[test]
    fn missing_path_is_unreachable() {
        let path = scratch("missing");
        let error = open_path(path.to_str().expect("utf-8 temp path"));
        assert!(
            matches!(&error, SearchError::Unreachable(message) if message.contains("could not be opened")),
            "missing path is Unreachable, got {error:?}"
        );
    }

    #[test]
    fn a_file_is_not_a_directory() {
        let path = scratch("a-file");
        std::fs::write(&path, b"not an index").expect("write scratch file");
        let error = open_path(path.to_str().expect("utf-8 temp path"));
        let _ = std::fs::remove_file(&path);
        assert!(
            matches!(&error, SearchError::Unreachable(message) if message.contains("is not a directory")),
            "a file path is Unreachable, got {error:?}"
        );
    }

    #[test]
    fn a_directory_without_iamglass_is_not_a_glass_index() {
        let path = scratch("no-marker");
        std::fs::create_dir_all(&path).expect("create scratch directory");
        let error = open_path(path.to_str().expect("utf-8 temp path"));
        let _ = std::fs::remove_dir_all(&path);
        assert!(
            matches!(&error, SearchError::Unreachable(message) if message.contains("no iamglass marker")),
            "a bare directory is Unreachable, got {error:?}"
        );
    }

    /// With the marker and a `postlist.glass` present the pre-checks pass and
    /// the stub stops loudly, naming the missing binding.
    ///
    /// The scratch directory removes itself through [`ScratchDir`]: the stub
    /// panics past the pre-checks, so cleanup cannot be a statement after the
    /// call, and a leftover `iamglass` directory is exactly the false positive
    /// [`check_index_path`] now refuses.
    #[test]
    #[should_panic(expected = "no Rust xapian binding is wired into this crate")]
    fn a_glass_shaped_directory_reaches_the_stub() {
        let path = ScratchDir(scratch("glass"));
        let path = path.0.as_path();
        std::fs::create_dir_all(path).expect("create scratch directory");
        std::fs::write(path.join("iamglass"), b"").expect("write marker");
        std::fs::write(path.join("postlist.glass"), b"").expect("write postlist table");
        let _ = XapianIndex::open(path.to_str().expect("utf-8 temp path"));
    }

    /// A marker with no tables is not an index: the shell a half-compacted or
    /// half-deleted glass database leaves (`VERIFY-FINAL.md` §3 — every
    /// `iamglass` directory on this machine is one, and `xapian-delve` opens
    /// none of them). It must be `Unreachable` here, not a process abort in
    /// `Database::open`.
    #[test]
    fn an_iamglass_marker_without_postlist_is_not_an_index() {
        let path = ScratchDir(scratch("shell"));
        let dir = path.0.as_path();
        std::fs::create_dir_all(dir).expect("create scratch directory");
        std::fs::write(dir.join("iamglass"), b"").expect("write marker");
        std::fs::write(dir.join("flintlock"), b"").expect("write lock file");
        let error = open_path(dir.to_str().expect("utf-8 temp path"));
        assert!(
            matches!(&error, SearchError::Unreachable(message) if message.contains("no postlist.glass")),
            "a marker without a postlist table is Unreachable, got {error:?}"
        );
    }

    /// `Index::open`'s error for `path`, keeping the scratch-directory
    /// tests out of the panic arm.
    fn open_path(path: &str) -> SearchError {
        match check_index_path(path) {
            Ok(()) => panic!("expected {path:?} to fail the pre-checks"),
            Err(error) => error,
        }
    }

    /// A scratch directory that removes itself on the way out, panic or not.
    struct ScratchDir(PathBuf);

    impl Drop for ScratchDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}
