//! The index schema shared by the writer (this module tree) and the
//! native reader (`crate::search::native`).
//!
//! Writer and reader compile against the same constants, so
//! writer/reader drift is a compile error rather than a search bug:
//! the identity-term shape, the document-type terms, the prefixed
//! field terms and the numeric value slots are defined once here.

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
