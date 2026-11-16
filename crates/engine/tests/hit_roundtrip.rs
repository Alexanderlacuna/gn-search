//! The hit round-trip: a recorded hit survives decode and re-encode, and an
//! unknown key survives with it.
//!
//! SCOUT sections implemented: SCOUT-NATIVE §4 (the field list) and
//! SCOUT-PROXY §3/§4.2 (unknown keys pass through `extra`; `pubmed_link` is
//! the key this exists for, because gn3 adds it to phenotype hits and GN4
//! has no named field for it).
//!
//! `fixtures/search_hit.json` is one recorded production hit
//! (`q=serotonin&type=phenotype&group=BXD`, trait 17985 — GN4's
//! `test/data/search_serotonin_midbrain_bxd.json`) with the two fields
//! GN4's route adds from PublishXRef (`locus`, `abbreviation`, quoted from
//! the recorded assertions in SCOUT-NATIVE §7) and the `lod` the route
//! computes (`lrs / 4.604`). Nothing in it is invented.

use gn_search_engine::SearchHit;

const FIXTURE: &str = include_str!("fixtures/search_hit.json");

fn fixture_value() -> serde_json::Value {
    serde_json::from_str(FIXTURE).expect("the fixture is valid JSON")
}

fn fixture_hit() -> SearchHit {
    serde_json::from_str(FIXTURE).expect("the fixture is a search hit")
}

/// The whole hit survives a decode/re-encode cycle: same fields, same
/// values, and no key invented or dropped. Comparing `serde_json::Value`s
/// makes this key-order independent (the `extra` map is sorted).
#[test]
fn recorded_hit_round_trips() {
    let hit = fixture_hit();
    let re_encoded = serde_json::to_value(&hit).expect("a hit serializes");
    assert_eq!(re_encoded, fixture_value());
}

/// Every named field is actually read from the fixture — a renamed or
/// reordered field would land in `extra` and this fails.
#[test]
fn every_named_field_is_read() {
    let hit = fixture_hit();
    assert_eq!(hit.name.as_deref(), Some("17985"));
    assert_eq!(hit.dataset.as_deref(), Some("BXDPublish"));
    assert_eq!(
        hit.dataset_fullname.as_deref(),
        Some("BXD Published Phenotypes")
    );
    assert_eq!(hit.group.as_deref(), Some("BXD"));
    assert_eq!(hit.species.as_deref(), Some("mouse"));
    assert_eq!(hit.authors.as_ref().map(Vec::len), Some(9));
    assert_eq!(hit.year, Some(2014));
    assert!(hit
        .description
        .as_deref()
        .is_some_and(|description| description.contains("serotonin")));
    let lrs = hit.lrs.expect("lrs is recorded");
    assert!((lrs - 11.999469156515026).abs() < f64::EPSILON);
    let lod = hit.lod.expect("lod is recorded");
    assert!(
        (lod - lrs / 4.604).abs() < 1e-12,
        "lod must be lrs / 4.604 (SCOUT-NATIVE §4)"
    );
    assert_eq!(hit.locus.as_deref(), Some("rsm10000012461"));
    assert_eq!(hit.abbreviation.as_deref(), Some("5HT_MB_M"));
    assert_eq!(hit.geno_chr.as_deref(), Some("16"));
    assert_eq!(hit.inbredsetcode.as_deref(), Some("BXD"));
    assert_eq!(hit.pubmed_id, Some(24102824));
    assert!(hit.mean.is_some() && hit.additive.is_some() && hit.geno_mb.is_some());
    // Nothing the fixture carries is unaccounted for except the one key
    // with no named field.
    assert_eq!(hit.extra.len(), 1, "only pubmed_link is unknown");
}

/// The proxy passthrough: `pubmed_link` has no named field and must survive
/// decode, re-encode and the `extra` map (SCOUT-PROXY §4.2).
#[test]
fn unknown_key_survives_in_extra() {
    let hit = fixture_hit();
    let link = hit.extra.get("pubmed_link").expect("pubmed_link is kept");
    assert_eq!(
        link.as_str(),
        Some(
            "http://www.ncbi.nlm.nih.gov/entrez/query.fcgi?cmd=Retrieve&db=PubMed&list_uids=24102824&dopt=Abstract"
        )
    );
    let re_encoded = serde_json::to_value(&hit).expect("a hit serializes");
    assert_eq!(re_encoded["pubmed_link"], *link);
}

/// An absent named field is not dropped silently: it comes back as `null`,
/// exactly as GN4's own struct re-serializes it (SCOUT-PROXY §3).
#[test]
fn absent_named_fields_re_serialize_as_null() {
    let hit: SearchHit = serde_json::from_str(r#"{"name":"1"}"#).expect("a minimal hit");
    let re_encoded = serde_json::to_value(&hit).expect("a hit serializes");
    assert_eq!(
        re_encoded["name"],
        serde_json::Value::String("1".to_string())
    );
    assert_eq!(re_encoded["dataset"], serde_json::Value::Null);
    assert_eq!(re_encoded["pubmed_id"], serde_json::Value::Null);
    assert!(
        re_encoded.get("extra").is_none(),
        "extra is flattened, never a key of its own"
    );
}

/// Inside `extra`, absent and `null` stay distinguishable: a key that was
/// present with a null value keeps its null (SCOUT-PROXY §3).
#[test]
fn null_unknown_key_stays_null() {
    let hit: SearchHit = serde_json::from_str(r#"{"name":"1","pubmed_link":null}"#).expect("a hit");
    assert_eq!(hit.extra.get("pubmed_link"), Some(&serde_json::Value::Null));
    let re_encoded = serde_json::to_value(&hit).expect("a hit serializes");
    assert!(re_encoded.get("pubmed_link").is_some());
    assert_eq!(re_encoded["pubmed_link"], serde_json::Value::Null);
}

/// `extra` is a `BTreeMap`, GN4's own type, so its keys serialise in sorted
/// order in every build — `serde_json::Map` would follow whatever
/// `serde_json/preserve_order` feature unification decided for the whole
/// binary (F7).
#[test]
fn extra_keys_serialise_in_sorted_order() {
    let hit: SearchHit =
        serde_json::from_str(r#"{"name":"1","zeta":1,"alpha":2,"mu":3}"#).expect("a hit");
    assert_eq!(hit.extra.len(), 3);
    let encoded = serde_json::to_string(&hit).expect("a hit serializes");
    let alpha = encoded.find("\"alpha\"").expect("alpha is kept");
    let mu = encoded.find("\"mu\"").expect("mu is kept");
    let zeta = encoded.find("\"zeta\"").expect("zeta is kept");
    assert!(
        alpha < mu && mu < zeta,
        "extra keys must be sorted, got {encoded}"
    );
}
