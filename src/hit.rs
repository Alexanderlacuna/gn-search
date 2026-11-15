//! The hit shape: one search result.
//!
//! SCOUT section implemented: SCOUT-NATIVE §4 (the field list, taken
//! verbatim from GN4's `SearchHit` at `src/search/mod.rs:66-95`), with the
//! passthrough contract of SCOUT-PROXY §3/§4.2 (unknown keys survive in
//! [`SearchHit::extra`] — that is how `pubmed_link`, which gn3 adds for
//! phenotype hits and GN4 has no named field for, reaches the client).
//!
//! Two properties matter to GN4 and are easy to break:
//!
//! * every named field is `Option`, because gn3 omits computed values
//!   (`lrs`, `geno_chr`/`geno_mb`) for traits that do not have them. A
//!   named field that is absent upstream deserializes to `None` and
//!   re-serializes as `null` (serde does not skip by default), which is
//!   exactly what GN4's own struct does (SCOUT-PROXY §3).
//! * unknown keys are NOT dropped: with `#[serde(flatten)]` on
//!   [`extra`](SearchHit::extra) they round-trip verbatim, so a proxy
//!   path that adds fields upstream keeps working without a schema change.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One search hit, in gn3's shape (SCOUT-NATIVE §4).
///
/// Field order and names follow GN4's struct so a diff against
/// `src/search/mod.rs` is mechanical. `extra` is not part of that struct's
/// 18 named fields — it is the catch-all that keeps the proxy transparent.
///
/// The `utoipa` derive and the `extra` attribute are behind the `schema`
/// feature: GN4 re-exports this type and documents it inside
/// `Envelope<T: Serialize + ToSchema>`, which makes `ToSchema` a
/// type-formation requirement there (`src/response.rs:61`), and a derive is
/// the only way to supply it for a type this crate defines. The attribute
/// text is GN4's own (`src/search/mod.rs:150-152`), moved with the field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
pub struct SearchHit {
    /// Trait or record name ("10199", "17985").
    pub name: Option<String>,
    /// Dataset label (`C57BL-6JxC57BL-6NJF2Publish`, `BXDPublish`).
    pub dataset: Option<String>,
    /// Dataset's human-readable name.
    pub dataset_fullname: Option<String>,
    /// Group / InbredSet name (`BXD`).
    pub group: Option<String>,
    /// Species name (`mouse`, `rat`, `human`).
    pub species: Option<String>,
    /// Author list, as published.
    pub authors: Option<Vec<String>>,
    /// Publication year.
    pub year: Option<u16>,
    /// Free-text trait description.
    pub description: Option<String>,
    /// Trait mean.
    pub mean: Option<f64>,
    /// Additive effect size.
    pub additive: Option<f64>,
    /// Likelihood ratio statistic of the peak.
    pub lrs: Option<f64>,
    /// LOD score, `lrs / 4.604`; GN4's route computes it from `lrs`
    /// (SCOUT-NATIVE §4).
    pub lod: Option<f64>,
    /// Peak marker name (`rsm10000012461`), looked up by GN4's route in
    /// PublishXRef; null for non-phenotype hits or when the database is
    /// unavailable (SCOUT-NATIVE §4).
    pub locus: Option<String>,
    /// The trait's short name (`5HT_MB_M`), looked up like `locus`.
    pub abbreviation: Option<String>,
    /// Peak chromosome.
    pub geno_chr: Option<String>,
    /// Peak position, megabases.
    pub geno_mb: Option<f64>,
    /// InbredSet code (`BRC`, `BXD`).
    pub inbredsetcode: Option<String>,
    /// PubMed id of the publication behind a phenotype hit.
    pub pubmed_id: Option<u32>,
    /// Every key the backend sends that has no named field above.
    ///
    /// `BTreeMap`, exactly GN4's type (`src/search/mod.rs:150-152`), so
    /// key order is sorted and stable in every build. (`serde_json::Map`
    /// would agree today, but it becomes insertion-ordered for the whole
    /// binary the moment any crate in GN4's workspace enables
    /// `serde_json/preserve_order` — feature unification is workspace-wide.)
    /// Values are kept verbatim: `pubmed_link` is the case that matters
    /// today (SCOUT-PROXY §4.2), and the native backend's doc data may
    /// grow others.
    #[serde(flatten)]
    #[cfg_attr(
        feature = "schema",
        schema(value_type = Option<std::collections::BTreeMap<String, String>>)
    )]
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `extra` is empty, not absent, for a hit with only named fields:
    /// serialization must not invent an `extra` key, and deserialization
    /// must not require one.
    #[test]
    fn extra_is_empty_not_required() {
        let json = r#"{"name":"10199","dataset":null}"#;
        let hit: SearchHit = serde_json::from_str(json).expect("hit with named keys only");
        assert_eq!(hit.name.as_deref(), Some("10199"));
        assert!(hit.extra.is_empty(), "no unknown keys, so extra is empty");
        assert_eq!(hit.dataset, None);
        assert_eq!(hit.group, None);
        // `gen_chr` is not a named field (the named one is `geno_chr`), so
        // it must land in `extra` rather than vanish.
        let json = r#"{"name":"10199","gen_chr":"14"}"#;
        let hit: SearchHit = serde_json::from_str(json).expect("hit with an unknown key");
        assert_eq!(
            hit.extra.get("gen_chr"),
            Some(&serde_json::Value::String("14".to_string()))
        );
    }
}
