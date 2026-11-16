//! An in-memory index, so every test in this crate needs nothing external.
//!
//! SCOUT sections implemented: SCOUT-NATIVE §4 (hits decoded from a
//! document's stored JSON — here the stored JSON is the recorded fixture
//! corpus, materialized as structs) and §5 (the `XT{type}` filter and the
//! retrieval contract: the match set from one offset, in full).
//!
//! # This is a test double, and it refuses to be anything else
//!
//! [`FixtureIndex::open`] answers an EMPTY path and nothing else. It is
//! compiled only under `--features fixture`, and it is not on
//! [`crate::search`]'s path: production opens the xapian index the engine
//! is given ([`crate::native`]). A double that accepted
//! `/var/genenetwork/xapian` and answered it with the five recorded alcohol
//! phenotypes below would fabricate search results — silent, plausible,
//! wrong data, wrapped by the route in a perfectly good envelope. So a
//! non-empty path is an [`SearchError::Internal`] naming the feature, never
//! a hit set.
//!
//! The five documents are the recorded production response for
//! `q=alcohol&type=phenotype` (GN4 `test/data/search_alcohol_phenotype.json`,
//! the 5-hit fixture SCOUT-NATIVE §7 relies on): dataset
//! `C57BL-6JxC57BL-6NJF2Publish`, traits `10199`, `10200`, `10201`,
//! `10210`, `10211`. Nothing here is invented, and no field is filled in
//! that production did not send — `lod`, `locus`, `abbreviation` and
//! `pubmed_id` stay `None` because the route, not the backend, computes
//! those (SCOUT-NATIVE §4).
//!
//! What the fixture matches, and what it does not:
//!
//! * the document type filter is exact (`XT{type}`, SCOUT-NATIVE §3);
//! * a boolean term matches the corresponding hit field, lowercased
//!   (this is what proves the grammar's lowercase-on-value rule reaches an
//!   index intact);
//! * a numeric or position range matches the hit's own value for that slot;
//! * free text and free-text fields are approximated with a
//!   case-insensitive substring test, because the fixture holds no term
//!   list and no stems (a real index stems and scores; SCOUT-NATIVE §3);
//! * operators are ignored — matching is conjunctive across the rendered
//!   terms, which is all this double needs (paging, type filtering, hit
//!   shape). Boolean structure is the backend's, and the fixture is not
//!   one.
//!
//! Fields the hit shape does not carry cannot be matched: `tissue:`,
//! `peakchr:`, `rif:`, `wiki:` and the peak-mb slot match nothing here
//! (SCOUT-NATIVE §4 lists no such fields; SCOUT-INDEXER §2 has the slots).

use crate::grammar::{ParsedQuery, RenderedTerm, TermKind, GRAMMAR};
use crate::index::{offset_for_page, to_usize, Index};
use gn_search_schema::SearchError;
use gn_search_schema::SearchHit;

/// The recorded corpus's dataset label.
const DATASET: &str = "C57BL-6JxC57BL-6NJF2Publish";
/// The recorded corpus's dataset name.
const DATASET_FULLNAME: &str = "C57BL/6JxC57BL/6NJ F2 CROSS Phenotypes";
/// The recorded corpus's group.
const GROUP: &str = "C57BL-6JxC57BL-6NJF2";
/// The recorded corpus's species.
const SPECIES: &str = "mouse";
/// The recorded corpus's InbredSet code.
const INBREDSETCODE: &str = "BRC";
/// The recorded corpus's publication year.
const YEAR: u16 = 2013;
/// The recorded corpus's author list (identical for all five hits).
const AUTHORS: [&str; 7] = [
    "Mulligan MK",
    "Adler A",
    "Ingles J",
    "Roy S",
    "Weerasekera L",
    "Lu L",
    "Williams R",
];

/// One index document: its type term and its stored hit.
struct FixtureDoc {
    /// The `XT{type}` document type (`phenotype`, `gene`; SCOUT-INDEXER §2).
    type_: &'static str,
    hit: SearchHit,
}

/// The fixture index: the five recorded hits, in the recorded order.
///
/// Order is the backend's (a real index returns relevance order); nothing
/// here re-sorts, because sorting is the route's job (SCOUT-NATIVE §5).
pub struct FixtureIndex {
    docs: Vec<FixtureDoc>,
}

impl Index for FixtureIndex {
    /// Open the fixture index — for an EMPTY path only (module doc).
    ///
    /// A non-empty path is a request for a real index, and this build has no
    /// backend that could answer one: it is [`SearchError::Internal`] (500
    /// on GN4's mapping) naming the missing feature, never a hit set.
    fn open(path: &str) -> Result<Self, SearchError> {
        if !path.is_empty() {
            return Err(SearchError::Internal(format!(
                "the in-memory fixture answers only an empty index path, and only in tests; the \
                 index at {path} must be opened by the xapian engine (build with --features \
                 fixture for the double, and pass an empty path)"
            )));
        }
        Ok(Self {
            docs: recorded_docs(),
        })
    }

    fn search(
        &self,
        q: &ParsedQuery,
        limit: u64,
        page: u64,
    ) -> Result<Vec<SearchHit>, SearchError> {
        // The full match set from the page's offset: `limit` places the
        // offset and truncation is the caller's (`Index::search`).
        let offset = offset_for_page(limit, page);
        let matching: Vec<SearchHit> = self
            .docs
            .iter()
            .filter(|doc| type_matches(doc, q))
            .filter(|doc| terms_match(&doc.hit, q))
            .map(|doc| doc.hit.clone())
            .collect();
        Ok(matching.into_iter().skip(to_usize(offset)).collect())
    }
}

/// Does the document's type term survive the query's type filter?
fn type_matches(doc: &FixtureDoc, q: &ParsedQuery) -> bool {
    match q.type_filter.as_ref() {
        Some(type_) => doc.type_ == type_.as_str(),
        None => true,
    }
}

/// Every rendered term has to match (conjunctive; operators are ignored —
/// see the module doc).
fn terms_match(hit: &SearchHit, q: &ParsedQuery) -> bool {
    q.terms.iter().all(|term| term_matches(hit, term))
}

fn term_matches(hit: &SearchHit, term: &RenderedTerm) -> bool {
    match term.kind {
        TermKind::Boolean => boolean_matches(hit, &term.prefix, term_value(term)),
        TermKind::Probabilistic => probabilistic_matches(hit, &term.prefix, term_value(term)),
        TermKind::FreeText => free_text_matches(hit, &term.term),
        TermKind::ValueRange { slot, begin, end } => range_matches(hit, slot, begin, end),
        // Structure is not modelled by the fixture (module doc).
        TermKind::Operator => true,
    }
}

/// The value a rendered term carries: for boolean and free-text fields
/// `term` is `prefix` + the value (see [`RenderedTerm`]); the prefixes are
/// ASCII, so the byte offset is a character boundary.
fn term_value(term: &RenderedTerm) -> &str {
    &term.term[term.prefix.len()..]
}

/// Match a boolean term against the hit field the grammar's prefix names.
///
/// The field is looked up through [`Grammar::boolean_by_prefix`] rather
/// than a second copy of the prefix table, so the two cannot drift.
fn boolean_matches(hit: &SearchHit, prefix: &str, value: &str) -> bool {
    match GRAMMAR.boolean_by_prefix(prefix).map(|entry| entry.field) {
        Some("author") => hit
            .authors
            .as_ref()
            .is_some_and(|authors| authors.iter().any(|a| a.to_lowercase() == value)),
        Some("species") => field_eq(hit.species.as_deref(), value),
        Some("group") => field_eq(hit.group.as_deref(), value),
        Some("dataset") => field_eq(hit.dataset.as_deref(), value),
        // A gene-symbol term answers to the record name; the hit shape has
        // no separate symbol field (SCOUT-NATIVE §4).
        Some("symbol") => field_eq(hit.name.as_deref(), value),
        Some("chr") => field_eq(hit.geno_chr.as_deref(), value),
        // `tissue` and `peakchr` have no hit field to match against.
        _ => false,
    }
}

/// Match a free-text field term against the hit field the grammar's prefix
/// names.
fn probabilistic_matches(hit: &SearchHit, prefix: &str, value: &str) -> bool {
    match GRAMMAR
        .probabilistic_by_prefix(prefix)
        .map(|entry| entry.field)
    {
        Some("description") => field_contains(hit.description.as_deref(), value),
        // RIF and wiki text live in the index's dictionaries, not in a hit.
        _ => false,
    }
}

fn free_text_matches(hit: &SearchHit, token: &str) -> bool {
    let fields = [
        hit.name.as_deref(),
        hit.dataset.as_deref(),
        hit.description.as_deref(),
        hit.group.as_deref(),
        hit.species.as_deref(),
    ];
    let needle = token.to_lowercase();
    fields
        .into_iter()
        .flatten()
        .any(|field| field.to_lowercase().contains(&needle))
}

/// Match a value-slot range against the hit's own value for that slot.
fn range_matches(hit: &SearchHit, slot: u32, begin: Option<f64>, end: Option<f64>) -> bool {
    if begin.is_none() && end.is_none() {
        // The both-open range matches nothing (SCOUT-NATIVE §3).
        return false;
    }
    let Some(value) = slot_value(hit, slot) else {
        return false;
    };
    begin.map_or(true, |low| value >= low) && end.map_or(true, |high| value <= high)
}

/// The hit field behind a value slot (SCOUT-INDEXER §2 numbers them: 0
/// mean, 1 peak/LRS, 2 position-Mb, 3 peak-Mb, 4 additive, 5 year).
///
/// Slot 3 (peak-Mb) and slot 6 (sample count) have no `SearchHit` field —
/// the index carries them from its own query, not from the served document
/// — so a `peakmb:` term matches nothing here.
fn slot_value(hit: &SearchHit, slot: u32) -> Option<f64> {
    match slot {
        0 => hit.mean,
        1 => hit.lrs,
        2 => hit.geno_mb,
        4 => hit.additive,
        5 => hit.year.map(f64::from),
        _ => None,
    }
}

/// Exact, case-insensitive comparison — boolean terms are stored lowercase
/// (SCOUT-NATIVE §3), so the field is lowercased before comparing.
fn field_eq(field: Option<&str>, value: &str) -> bool {
    field.is_some_and(|text| text.to_lowercase() == value)
}

/// Case-insensitive substring test: the fixture's stand-in for stemming.
fn field_contains(field: Option<&str>, value: &str) -> bool {
    let needle = value.to_lowercase();
    field.is_some_and(|text| text.to_lowercase().contains(&needle))
}

/// The five recorded hits (module doc for provenance).
fn recorded_docs() -> Vec<FixtureDoc> {
    vec![
        FixtureDoc {
            type_: "phenotype",
            hit: recorded_hit(
                "10199",
                "Central nervous system, behavior: Alcohol consumption, 5% alcohol \
                 consumption in the two-bottle choice paradigm measured alcohol consumed \
                 per kilogram body weight over 4 days [g/kg]",
                3.0200798533775886,
                0.377068891450584,
                6.62875131299756,
                "14",
                90.847095,
            ),
        },
        FixtureDoc {
            type_: "phenotype",
            hit: recorded_hit(
                "10200",
                "Central nervous system, behavior: Alcohol consumption, 10% alcohol \
                 consumption in the two-bottle choice paradigm measured alcohol consumed \
                 per kilogram body weight over 4 days [g/kg]",
                6.60806072090307,
                0.606294259223918,
                4.23305670639656,
                "6",
                145.24671,
            ),
        },
        FixtureDoc {
            type_: "phenotype",
            hit: recorded_hit(
                "10201",
                "Central nervous system, behavior: Alcohol consumption, 15% alcohol \
                 consumption in the two-bottle choice paradigm measured alcohol consumed \
                 per kilogram body weight over 4 days [g/kg]",
                9.884343530334856,
                -1.10100209590788,
                5.39335471040695,
                "4",
                65.944235,
            ),
        },
        FixtureDoc {
            type_: "phenotype",
            hit: recorded_hit(
                "10210",
                "Central nervous system, behavior: Alcohol consumption, 5% alcohol \
                 consumption in the two-bottle choice paradigm measured alcohol consumed \
                 per kilogram body weight over 4 days in females [g/kg]",
                3.418857609574822,
                0.465560827479829,
                4.60811099782063,
                "15",
                71.802121,
            ),
        },
        FixtureDoc {
            type_: "phenotype",
            hit: recorded_hit(
                "10211",
                "Central nervous system, behavior: Alcohol consumption, 10% alcohol \
                 consumption in the two-bottle choice paradigm measured alcohol consumed \
                 per kilogram body weight over 4 days in females [g/kg]",
                7.78438614240323,
                0.932973708033573,
                3.72495597970754,
                "17",
                26.854246,
            ),
        },
    ]
}

/// One recorded hit, with the corpus-wide fields filled in and every field
/// production did not send left `None`.
fn recorded_hit(
    name: &str,
    description: &str,
    mean: f64,
    additive: f64,
    lrs: f64,
    geno_chr: &str,
    geno_mb: f64,
) -> SearchHit {
    SearchHit {
        name: Some(name.to_string()),
        dataset: Some(DATASET.to_string()),
        dataset_fullname: Some(DATASET_FULLNAME.to_string()),
        group: Some(GROUP.to_string()),
        species: Some(SPECIES.to_string()),
        authors: Some(AUTHORS.iter().map(|author| author.to_string()).collect()),
        year: Some(YEAR),
        description: Some(description.to_string()),
        mean: Some(mean),
        additive: Some(additive),
        lrs: Some(lrs),
        lod: None,
        locus: None,
        abbreviation: None,
        geno_chr: Some(geno_chr.to_string()),
        geno_mb: Some(geno_mb),
        inbredsetcode: Some(INBREDSETCODE.to_string()),
        pubmed_id: None,
        extra: std::collections::BTreeMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index() -> FixtureIndex {
        FixtureIndex::open("").expect("the fixture opens on an empty path")
    }

    fn names(hits: &[SearchHit]) -> Vec<&str> {
        hits.iter().filter_map(|hit| hit.name.as_deref()).collect()
    }

    /// A non-empty index path is a request this build cannot serve: the
    /// fixture refuses it instead of answering with its five recorded hits
    /// (F1 — fabricated results are worse than a named error).
    #[test]
    fn a_non_empty_index_path_is_refused() {
        match FixtureIndex::open("/var/genenetwork/xapian") {
            Err(SearchError::Internal(message)) => {
                assert!(
                    message.contains("answers only an empty index path"),
                    "the error must name what the double answers, got {message:?}"
                );
                assert!(
                    message.contains("--features fixture"),
                    "the error must name the feature, got {message:?}"
                );
            }
            other => panic!("expected Internal, got {:?}", other.map(|_| "an index")),
        }
        // And through the path a caller actually takes: the engine, never the
        // fixture. An unset path is the moved engine's `Internal` (naming
        // `XAPIAN_DB_PATH`) and a path that is not there is `Unreachable` —
        // neither yields the five recorded hits.
        match crate::search("", "alcohol", "phenotype", 20, 1) {
            Err(SearchError::Internal(message)) => assert!(
                message.contains("XAPIAN_DB_PATH is not configured"),
                "an unset index path must name the config key, got {message:?}"
            ),
            other => panic!(
                "an unset index path must be Internal, got {:?}",
                other.map(|hits| hits.len())
            ),
        }
        match crate::search("/var/genenetwork/xapian", "alcohol", "phenotype", 20, 1) {
            Err(SearchError::Unreachable(_)) => {}
            Ok(hits) => panic!(
                "an unknown index path must not yield hits, got {} of them",
                hits.len()
            ),
            Err(other) => panic!("expected Unreachable, got {other:?}"),
        }
    }

    /// The whole recorded set, unfiltered, in recorded order.
    #[test]
    fn all_five_recorded_hits() {
        let hits = index()
            .search(&GRAMMAR.render("").expect("empty query renders"), 20, 1)
            .expect("search");
        assert_eq!(
            names(&hits),
            vec!["10199", "10200", "10201", "10210", "10211"]
        );
        assert_eq!(hits[0].dataset.as_deref(), Some(DATASET));
        assert_eq!(hits[0].geno_chr.as_deref(), Some("14"));
    }

    /// The `XT{type}` filter: the fixture holds phenotype documents only,
    /// so a gene query finds nothing and a phenotype query finds all five
    /// (SCOUT-NATIVE §3, §4).
    #[test]
    fn type_filter_is_applied() {
        let query = GRAMMAR.render("alcohol").expect("renders");
        let phenotype = index()
            .search(&query.clone().with_type("phenotype"), 20, 1)
            .expect("search");
        assert_eq!(phenotype.len(), 5);
        let gene = index()
            .search(&query.with_type("gene"), 20, 1)
            .expect("search");
        assert!(gene.is_empty(), "no gene documents in the fixture");
    }

    /// `page` shifts the offset and the whole remaining match set comes
    /// back — `limit` is not a page size (`Index::search`, F5). A caller
    /// that wants one page truncates what it gets.
    #[test]
    fn paging_shifts_the_offset_and_returns_the_rest() {
        let query = GRAMMAR.render("alcohol").expect("renders");
        let page1 = index().search(&query, 2, 1).expect("search");
        assert_eq!(
            names(&page1),
            vec!["10199", "10200", "10201", "10210", "10211"],
            "page 1 is the whole set from offset 0"
        );
        let page2 = index().search(&query, 2, 2).expect("search");
        assert_eq!(names(&page2), vec!["10201", "10210", "10211"]);
        let page3 = index().search(&query, 2, 3).expect("search");
        assert_eq!(names(&page3), vec!["10211"]);
        let page4 = index().search(&query, 2, 4).expect("search");
        assert!(page4.is_empty(), "past the end is an empty page");
    }

    /// `limit == 0` and `limit == u64::MAX` both mean "everything": the
    /// offset arithmetic gives 0 and nothing caps the result (GN4's
    /// `first_item = (page - 1) * limit`, `native.rs:511`).
    #[test]
    fn limit_zero_and_limit_max_return_the_whole_set() {
        let query = GRAMMAR.render("alcohol").expect("renders");
        let zero = index().search(&query, 0, 1).expect("search");
        assert_eq!(zero.len(), 5, "limit=0 is the unbounded fetch");
        let max = index().search(&query, u64::MAX, 1).expect("search");
        assert_eq!(max.len(), 5, "u64::MAX is the unbounded fetch");
    }

    /// A limit larger than the hit count is not an error: it is the
    /// ordinary full-match-set fetch (F5).
    #[test]
    fn a_limit_larger_than_the_hit_count_is_not_an_error() {
        let query = GRAMMAR.render("alcohol").expect("renders");
        let hits = index()
            .search(&query, 1_000, 1)
            .expect("a limit past the match count is a normal search");
        assert_eq!(hits.len(), 5);
    }

    /// A boolean term reaches the index lowercased, so the way a caller
    /// naturally writes a dataset label matches (SCOUT-NATIVE §3).
    #[test]
    fn boolean_term_matches_mixed_case_value() {
        let query = GRAMMAR
            .render("dataset:C57BL-6JxC57BL-6NJF2Publish")
            .expect("renders");
        let hits = index().search(&query, 20, 1).expect("search");
        assert_eq!(hits.len(), 5);
        let miss = GRAMMAR.render("dataset:not-a-dataset").expect("renders");
        assert!(index().search(&miss, 20, 1).expect("search").is_empty());
    }

    /// A numeric range reads the hit's own value for the slot: `lrs` is
    /// slot 1, and the recorded set has exactly two traits above 5.
    #[test]
    fn numeric_range_reads_the_slot() {
        let query = GRAMMAR.render("peak:5..").expect("renders");
        let hits = index().search(&query, 20, 1).expect("search");
        assert_eq!(names(&hits), vec!["10199", "10201"]);
    }

    /// A position range reads slot 2 (`geno_mb`, megabases): `position:65.9M`
    /// expands to 65.85–65.95 Mb, and only the 65.944 Mb trait matches.
    #[test]
    fn position_range_reads_position_slot() {
        let query = GRAMMAR.render("position:65.9M").expect("renders");
        let hits = index().search(&query, 20, 1).expect("search");
        assert_eq!(names(&hits), vec!["10201"]);
    }
}
