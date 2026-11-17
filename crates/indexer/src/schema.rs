//! The index schema — now declared once, in `gn-search-schema`.
//!
//! The writer and the reader both depend on that crate, so the prefixes, the
//! document-type terms and the numeric value slots have one definition site
//! instead of two hand-kept copies. This module re-exports it so the writer's
//! call sites read the same as they always did.

pub use gn_search_schema::schema::*;

#[cfg(test)]
mod tests {
    /// The digest is stable: the same table always renders to the same string.
    #[test]
    fn canonical_rendering_is_deterministic() {
        assert_eq!(
            gn_search_schema::canonical_schema(),
            gn_search_schema::canonical_schema()
        );
    }

    /// Every prefix and slot a document is written with appears in the digest,
    /// so a change to any of them changes the stamp.
    #[test]
    fn the_digest_covers_prefixes_and_slots() {
        let text = gn_search_schema::canonical_schema();
        for needle in [
            "prefix species=XS", "prefix dataset=XDS", "prefix symbol=XY", "prefix chr=XC",
            "prefix peakchr=XPC", "slot mean=0", "slot position=2", "slot nsamples=6",
        ] {
            assert!(text.contains(needle), "canonical() is missing {needle:?}:\n{text}");
        }
    }
}
