//! The index schema: how a document is written and read.
//!
//! One table, shared by the writer (`gn-search-indexer`) and the reader
//! (`gn-search-engine`). Both depend on this crate, so writer/reader drift is a
//! compile error rather than a search bug.
//!
//! Changing the MEANING of an entry — moving a number to another slot, reusing a
//! prefix for a different field — is a schema change: bump `SCHEMA_VERSION` with
//! it and rebuild. A reader that implements the old version refuses the new
//! index by name instead of answering from the wrong slot.


/// Document type for expression (ProbeSet) records; the type term is
/// `XTgene` (`write_document`, index-genenetwork).
pub const DOCTYPE_GENE: &str = "gene";
/// Document type for published phenotype records; the type term is
/// `XTphenotype`.
pub const DOCTYPE_PHENOTYPE: &str = "phenotype";

/// Prefixed term prefixes, exactly the set the indexer emits
/// (`index_species`..`index_peakchr`, `index_from_dictionary`).
pub mod prefix {
    /// Authors (indexed WITH positions, wdf increment 0).
    pub const AUTHOR: &str = "A";
    /// Species (boolean, no positions).
    pub const SPECIES: &str = "XS";
    /// Group (boolean, no positions).
    pub const GROUP: &str = "XG";
    /// Tissue (WITH positions, wdf increment 0).
    pub const TISSUE: &str = "XI";
    /// Dataset (WITH positions, wdf increment 0).
    pub const DATASET: &str = "XDS";
    /// Symbol (boolean, no positions).
    pub const SYMBOL: &str = "XY";
    /// Chromosome (boolean, no positions).
    pub const CHR: &str = "XC";
    /// Peak chromosome (boolean, no positions).
    pub const PEAK_CHR: &str = "XPC";
    /// Free-text description (probabilistic prefix, weighted by BM25).
    pub const DESCRIPTION: &str = "XD";
    /// RIF dictionary text (no positions, wdf increment 0).
    pub const RIF: &str = "XRF";
    /// Wiki dictionary text (no positions, wdf increment 0).
    pub const WIKI: &str = "XWK";
}

/// Numeric value slots (`add_mean`..`add_nsamples`).
pub mod slot {
    /// Trait mean (`add_mean`).
    pub const MEAN: u32 = 0;
    /// Peak / LRS (`add_peak`).
    pub const PEAK: u32 = 1;
    /// Position in megabases (`add_mb`).
    pub const POSITION: u32 = 2;
    /// Peak position in megabases (`add_peakmb`).
    pub const PEAK_MB: u32 = 3;
    /// Additive effect (`add_additive`).
    pub const ADDITIVE: u32 = 4;
    /// Publication year (`add_year`).
    pub const YEAR: u32 = 5;
    /// Number of samples (`add_nsamples`).
    pub const NSAMPLES: u32 = 6;
}

/// The document-type term `XT<doctype>` written by `write_document`.
pub fn type_term(doctype: &str) -> String {
    format!("XT{doctype}")
}

/// The stable identity term `Q<doctype>:<name>:<dataset>` (the whole
/// `name:dataset` identifier lowercased), matching `write_document` in
/// the Python indexer.
pub fn identity_term(doctype: &str, name: &str, dataset: &str) -> String {
    format!(
        "Q{}:{}",
        doctype,
        format!("{name}:{dataset}").to_lowercase()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_term_is_lowercased_with_dataset() {
        assert_eq!(
            identity_term(DOCTYPE_PHENOTYPE, "10583", "BXDPublish"),
            "Qphenotype:10583:bxdpublish"
        );
        assert_eq!(
            identity_term(DOCTYPE_GENE, "100001_at", "HC_U_0304_R"),
            "Qgene:100001_at:hc_u_0304_r"
        );
    }

    #[test]
    fn type_term_matches_reference() {
        assert_eq!(type_term(DOCTYPE_GENE), "XTgene");
        assert_eq!(type_term(DOCTYPE_PHENOTYPE), "XTphenotype");
    }
}

/// The schema's version: bump this when the MEANING of a prefix or a slot
/// changes, and rebuild the index.
///
/// Adding a field in a slot nobody used can keep the version; changing what an
/// existing entry means cannot.
pub const SCHEMA_VERSION: u32 = 1;

/// A canonical rendering of the whole table, and the input to its digest.
///
/// Entries are emitted in a fixed order, so two builds of the same table produce
/// the same string and any change to the table changes it.
pub fn canonical() -> String {
    let mut out = format!("version={SCHEMA_VERSION}\n");
    out.push_str(&format!("doctype_gene={DOCTYPE_GENE}\n"));
    out.push_str(&format!("doctype_phenotype={DOCTYPE_PHENOTYPE}\n"));
    for (name, value) in [
        ("author", prefix::AUTHOR), ("species", prefix::SPECIES), ("group", prefix::GROUP),
        ("tissue", prefix::TISSUE), ("dataset", prefix::DATASET), ("symbol", prefix::SYMBOL),
        ("chr", prefix::CHR), ("peakchr", prefix::PEAK_CHR), ("description", prefix::DESCRIPTION),
        ("rif", prefix::RIF), ("wiki", prefix::WIKI),
    ] {
        out.push_str(&format!("prefix {name}={value}\n"));
    }
    for (name, value) in [
        ("mean", slot::MEAN), ("peak", slot::PEAK), ("position", slot::POSITION),
        ("peak_mb", slot::PEAK_MB), ("additive", slot::ADDITIVE), ("year", slot::YEAR),
        ("nsamples", slot::NSAMPLES),
    ] {
        out.push_str(&format!("slot {name}={value}\n"));
    }
    out
}

/// The digest a build stamps into the index and a reader checks on open.
pub fn digest() -> String {
    crate::sha256::hex_digest(canonical().as_bytes())
}
