//! The native engine: a Rust port of gn3's xapian search path
//! (`gn3/gn3/api/search.py`), querying the same python-built xapian
//! index directly instead of proxying gn3.
//!
//! # Moved here, not rewritten
//!
//! This file is GN4's `src/search/native.rs` (1003 lines) moved into the
//! crate as the crate's engine: `build_query_parser` and every
//! prefix/slot registration, [`guard_numeric_ranges`], the sentinel
//! guard, [`search_blocking`], [`open_database`] and its pre-checks, and
//! the `serde_json` decode of the stored doc data are that file's logic
//! verbatim. Four things could not travel unchanged, and nothing else
//! did: the module now says `crate::error`/`crate::hit` instead of
//! `super::`, it carries no `impl Engine` / no `tokio` (that async shell
//! is GN4's, in `src/search/gnindex.rs`), [`search_blocking`] takes the
//! index path as a parameter instead of reading
//! `crate::config::Config::load().xapian_db_path` (the crate has no
//! config), and one test drives [`search_blocking`] directly instead of
//! a tokio runtime. Every `native.rs:NNN` / `search.py:NNN` citation
//! below is preserved as it was and still refers to GN4's file.
//!
//! # Query grammar
//!
//! The [`QueryParser`] is built once per process with the gn3 parser
//! configuration (see the seam's grammar doc and
//! `~/spikes/port-design/PORT-DESIGN.md` §2). Custom registration is
//! `Box::leak`-based inside the binding, so building a parser per
//! request would leak per call; the parser is therefore built at
//! startup and cached behind a mutex (binding-report §5 item 3, the
//! round-3 R2 skeleton pattern).
//!
//! # Per-page invariant
//!
//! The engine retrieves the FULL match set and ignores the forwarded
//! `limit`, exactly as the parity recording did (`per_page=50000`;
//! PORT-DESIGN §5). The route keeps its validation/limit logic
//! unchanged; `search_native.rs` pins the invariant with a test that
//! forwards `limit=10` and asserts the full set still comes back.
//!
//! # Error contract
//!
//! gn3's order is preserved: the query is parsed first, the index
//! opened second (search.py parses at :291 before opening at :296), so
//! a malformed query reports as [`SearchError::InvalidQuery`] (422
//! `invalid_query`, carrying the parser's diagnostic) even when the
//! index is misconfigured. Other mappings per the seam's vocabulary
//! (DESIGN-NOTES S3): an unset index path is a local misconfiguration
//! ([`SearchError::Internal`], mirroring the bad-GN3_URL rule); an
//! index that cannot be opened is [`SearchError::Unreachable`]; a
//! document whose stored data is not a search hit is
//! [`SearchError::Misbehaving`] (the doc-data analogue of a response
//! body that fails to decode).
//!
//! The binding's `Database::open` aborts the process on failure (the
//! C++ exception is not translated across the FFI boundary), so
//! [`open_database`] pre-validates the path and maps failures onto the
//! seam vocabulary before any xapian call — the same app-level-guard
//! pattern PORT-DESIGN C3.6 pins for the mset-time error. A corrupt
//! index that passes the file checks would still abort; gn3 crashes on
//! such an index too, and no gate exercises it.

use gn_search_schema::SearchError;
use gn_search_schema::SearchHit;
use std::sync::{Mutex, OnceLock};
use xapian_rs::{
    Database, Enquire, FieldProcessor, NumberRangeProcessor, Operator, Query, QueryParser, Stem,
    StemStrategy, ToValue,
};

/// The numeric range prefixes and their value slots (search.py:234-243,
/// the loop's `enumerate` with `position` carved out for slot 2):
/// mean→0, peak→1, peakmb→3, additive→4, year→5. The slots are
/// verified literal on the indexer side (index-genenetwork:293-299).
/// The markers carry the colon, exactly like gn3's
/// `NumberRangeProcessor(i, prefix + ":")`.
const NUMERIC_RANGE_PREFIXES: [(&str, u32); 5] = [
    ("mean:", 0),
    ("peak:", 1),
    ("peakmb:", 3),
    ("additive:", 4),
    ("year:", 5),
];

/// The value slot holding gene positions, in MEGABASES (slot 2 —
/// `range_prefixes.index("position")`, search.py:234-235; the indexer's
/// `add_mb` writes slot 2, index-genenetwork:295).
const POSITION_SLOT: u32 = 2;
/// A point location expands to ± 50 000 bases (search.py:151-155).
const HALF_WINDOW_BASES: f64 = 50_000.0;
/// Bases per megabase (search.py:162-165: `Decimal(val)/10**6`).
const BASES_PER_MB: f64 = 1_000_000.0;

/// MSet bound for full-match-set retrieval: deliberately large so the
/// whole match set comes back regardless of the forwarded `limit`
/// (PORT-DESIGN §5; the recorded Q02 set alone is 57 632 hits, above
/// any limit the gn4 surface can express). Xapian stops at the actual
/// match count.
const FULL_MATCH_SET_MAX_ITEMS: u32 = u32::MAX;

/// A sendable wrapper around the process-cached parser.
struct Parser(QueryParser);

// SAFETY: `QueryParser` holds a raw C++ handle that the binding does
// not mark `Send`, and the handle itself is not safe to use from
// several threads concurrently — but every access goes through the
// enclosing `Mutex`, so the handle is only ever touched by one thread
// at a time. Xapian objects may be used from multiple threads when
// access is serialized; that is exactly what the mutex guarantees.
unsafe impl Send for Parser {}

/// The process-cached parser, built once with the gn3 parser
/// configuration (see [`build_query_parser`]).
fn parser() -> &'static Mutex<Parser> {
    static PARSER: OnceLock<Mutex<Parser>> = OnceLock::new();
    PARSER.get_or_init(|| Mutex::new(Parser(build_query_parser())))
}

/// Build the gn3-equivalent QueryParser configuration
/// (search.py:213-273). The parser is query- and index-independent, so
/// one per process serves every request.
fn build_query_parser() -> QueryParser {
    let mut qp = QueryParser::default();
    // search.py:216-217: English stemmer, STEM_ALL_Z (free-text terms
    // are stemmed and matched case-insensitively).
    qp.set_stemmer(Stem::for_language("en"));
    qp.set_stemming_strategy(StemStrategy::AllZ);
    // Boolean prefixes (search.py:220-228; r1-test:95-96, R2
    // skeleton). Values are lowercased before the index term is formed
    // (see [`LowercaseProcessor`]): the index stores every boolean term
    // in lowercase, so `dataset:BXDPublish` or `author:Williams` would
    // otherwise match nothing. gn3 forces the lowering for `group` only
    // (search.py:204-209); this does it for all of them, which changes
    // no graded parity shape (they all already use lowercase values).
    for (field, prefix) in [
        ("author", "A"),
        ("species", "XS"),
        ("group", "XG"),
        ("tissue", "XI"),
        ("dataset", "XDS"),
        ("symbol", "XY"),
        ("chr", "XC"),
        ("peakchr", "XPC"),
    ] {
        qp.add_custom_boolean_prefix::<_, &str>(field, LowercaseProcessor { prefix }, None);
    }
    // Probabilistic prefixes (search.py:229-233): weighted (BM25-scored)
    // text fields, freely mixable with free text and operators; rif and
    // wiki also answer to their uppercase aliases (search.py:232-233).
    qp.add_prefix::<&str>("description", Some("XD"));
    qp.add_prefix::<&str>("rif", Some("XRF"));
    qp.add_prefix::<&str>("wiki", Some("XWK"));
    qp.add_prefix::<&str>("RIF", Some("XRF"));
    qp.add_prefix::<&str>("WIKI", Some("XWK"));
    // Numeric range processors (search.py:234-243; C3.4). The fork's
    // NumberRangeProcessor parses bounds as f64 (the 2673115 fix) and
    // its bridge maps empty sides onto gn3's one-sided VALUE_LE /
    // VALUE_GE (measured: `mean:..15` → VALUE_LE 0, `mean:10..` →
    // VALUE_GE 0), superseding the skeleton's 0.0/1e18 substitution
    // for these registrations.
    for (marker, slot) in NUMERIC_RANGE_PREFIXES {
        qp.add_rangeprocessor(marker, slot, NumberRangeProcessor, false, false, None);
    }
    // `position` + its `pos` alias (search.py:237-241; C3.5): the custom
    // SI-suffix FieldProcessor on slot 2. Both registrations share one
    // processor type; each registration Box::leaks its own instance
    // inside the binding (the one-per-process parser absorbs that).
    qp.add_custom_boolean_prefix::<_, &str>("position", PositionProcessor, None);
    qp.add_custom_boolean_prefix::<_, &str>("pos", PositionProcessor, None);
    // Hs / Mm species-location shorthands (search.py:246-272; C3.6):
    // XShuman / XSmouse. WITHOUT the Hs liftover OR-branch — OPEN-4,
    // owner-decided; a C3.6b commit would add it. The invalid-location
    // leaf these processors may emit is detected by the engine guard
    // below (see INVALID_LOCATION_SENTINEL).
    qp.add_custom_boolean_prefix::<_, &str>(
        "Hs",
        LocationProcessor {
            species_term: "XShuman",
        },
        None,
    );
    qp.add_custom_boolean_prefix::<_, &str>(
        "Mm",
        LocationProcessor {
            species_term: "XSmouse",
        },
        None,
    );
    qp
}

/// Boolean-prefix values are lowercased before the index term is formed.
///
/// Both the dev index and a full production index store every boolean
/// term in lowercase — `XDSbxdpublish`, `Awilliams`, `XSmouse`, `XCx`,
/// `XYbdnf`, `XImrna`, `XPC1` — and the mixed-case variants do not exist
/// in the database at all (`XDSBXDPublish`, `AWilliams`, `XSMouse`,
/// `XCX`, `XYBDNF`, `XIMRNA` all report "not in database"). Matching the
/// value exactly therefore rejects the way a caller naturally writes it:
/// `dataset:BXDPublish` returned 0 hits against an index holding 15 060
/// `BXDPublish` traits. Lowercasing here is what gn3 already does for
/// `group` (search.py:204-209, the r1-test `LowercaseGroupProcessor`
/// pattern); this applies the same rule to the rest.
#[derive(Clone)]
struct LowercaseProcessor {
    prefix: &'static str,
}

impl FieldProcessor for LowercaseProcessor {
    fn process(&self, term: &str) -> Option<Query> {
        Some(Query::term(
            format!("{}{}", self.prefix, term.to_lowercase()),
            None,
            None,
        ))
    }
}

/// gn3's `apply_si_suffix` (search.py:136-142): an optional trailing
/// k/m/g multiplies by 10^3/6/9 (case-insensitive — gn3 lowercases the
/// last character before the lookup); an unsuffixed value must be an
/// INTEGER (`int(location)`, search.py:142 — so `1.5` without a
/// suffix is a gn3 ValueError, not a base-pair coordinate). Returns
/// `None` exactly where gn3 raises `ValueError` — the caller decides
/// what that means (position: MatchNothing per the parity-metric
/// amendment; Hs/Mm: the invalid-location sentinel).
fn apply_si_suffix(value: &str) -> Option<f64> {
    let (digits, multiplier) = match value.chars().last()?.to_ascii_lowercase() {
        'k' => (&value[..value.len() - 1], 1e3),
        'm' => (&value[..value.len() - 1], 1e6),
        'g' => (&value[..value.len() - 1], 1e9),
        _ => return value.parse::<i64>().ok().map(|n| n as f64),
    };
    digits.parse::<f64>().ok().map(|n| n * multiplier)
}

/// gn3's `parse_position` (search.py:145-155): a `..` spec is a range
/// with either side optionally empty; a point expands to ±50 kb clamped
/// at 0. Sides are in BASES.
fn parse_position(spec: &str) -> Option<(Option<f64>, Option<f64>)> {
    if let Some((begin, end)) = spec.split_once("..") {
        let begin = if begin.is_empty() {
            None
        } else {
            Some(apply_si_suffix(begin)?)
        };
        let end = if end.is_empty() {
            None
        } else {
            Some(apply_si_suffix(end)?)
        };
        Some((begin, end))
    } else {
        let point = apply_si_suffix(spec)?;
        Some((
            Some((point - HALF_WINDOW_BASES).max(0.0)),
            Some(point + HALF_WINDOW_BASES),
        ))
    }
}

/// The slot-2 value query for already-parsed position sides (gn3's
/// `parse_position_field`, search.py:158-165, minus the NumberRange-
/// Processor indirection): each side converts bases→megabases; an
/// empty side keeps gn3's one-sided C++ NumberRangeProcessor semantics
/// (VALUE_GE / VALUE_LE — the same forms the fork's bridge produces for
/// the registered numeric ranges); both empty is MatchNothing, the
/// degenerate form binding-report R3 documents as diverging (no
/// production shape uses it).
fn position_value_query(begin: Option<f64>, end: Option<f64>) -> Query {
    let megabases = |bases: f64| (bases / BASES_PER_MB).serialize();
    match (begin, end) {
        (Some(b), Some(e)) => Query::value_range(POSITION_SLOT, megabases(b), megabases(e)),
        (Some(b), None) => Query::value_ge(POSITION_SLOT, megabases(b)),
        (None, Some(e)) => Query::value_le(POSITION_SLOT, megabases(e)),
        (None, None) => Query::match_nothing(),
    }
}

/// gn3's `position` / `pos` FieldProcessor (search.py:237-241; C3.5):
/// SI-suffixed base-pair coordinates become a slot-2 megabase value
/// query, points expanding to ±50 kb clamped at 0. An unparseable spec
/// declines (`None`), which the fork's bridge maps to MatchNothing
/// (commit aa915af, Amendment A) — the parity-metric amendment's
/// blessed 200-empty default. gn3's own behavior for that class is an
/// escaping `ValueError` (a latent 500 crash, curl-verified live),
/// explicitly NOT a contract; no recorded fixture pins it (Q53
/// `position:` is an empty value and never reaches the processor).
#[derive(Clone)]
struct PositionProcessor;

impl FieldProcessor for PositionProcessor {
    fn process(&self, term: &str) -> Option<Query> {
        let (begin, end) = parse_position(term)?;
        Some(position_value_query(begin, end))
    }
}

/// The invalid Hs/Mm leaf marker (C3.6): the Hs/Mm processors emit this
/// TERM where gn3's `parse_location_field` raises `ValueError`
/// (search.py:178-183, 199 — a value that does not split into exactly
/// `chr<chromosome>:<position-spec>`, a chromosome not starting with
/// `chr`, or an unparseable position spec). gn3 maps that leaf to
/// `OP_INVALID`, which `Enquire` then rejects with an uncatchable
/// `InvalidOperationError` — production's recorded Q42 500 (the mset
/// crash), a process-abort hazard under the binding. The blessed 422
/// contract (AMENDMENT E) instead surfaces the condition as a typed
/// invalid query BEFORE the mset call — the app-level guard,
/// evidence-backed option (b) from PORT-DESIGN C3.6; the C++ try/catch
/// shim is deliberately NOT extended. The engine detects the term in
/// the parsed query's unique terms and reports InvalidQuery. The term
/// matches nothing (verified: absent from the dev index) and free text
/// cannot produce it under STEM_ALL_Z (free text is stemmed and
/// lowercase-prefixed with Z).
const INVALID_LOCATION_SENTINEL: &str = "GN4InvalidLocationLeaf";

/// The invalid-location leaf (see [`INVALID_LOCATION_SENTINEL`]).
fn invalid_location_leaf() -> Query {
    Query::term(INVALID_LOCATION_SENTINEL, None, None)
}

/// gn3's `parse_location_field` (search.py:168-202) WITHOUT the Hs
/// liftover OR-branch (OPEN-4 — liftover stays out; a C3.6b commit
/// would add it): an AND of the species term (XShuman / XSmouse), the
/// chromosome term (XC + chromosome number) and the slot-2 position
/// range. The whole value is lowercased before splitting, exactly like
/// gn3's `split_query` (search.py:178-183).
#[derive(Clone)]
struct LocationProcessor {
    /// The species boolean term: `XShuman` for Hs, `XSmouse` for Mm
    /// (search.py:246-247).
    species_term: &'static str,
}

impl FieldProcessor for LocationProcessor {
    fn process(&self, term: &str) -> Option<Query> {
        let value = term.to_lowercase();
        let mut parts = value.split(':');
        let (chromosome, position_spec) = match (parts.next(), parts.next(), parts.next()) {
            (Some(chromosome), Some(position_spec), None) => (chromosome, position_spec),
            // gn3's `chromosome, position_spec = query.split(":")`
            // unpack raises ValueError on any other part count.
            _ => return Some(invalid_location_leaf()),
        };
        let Some(chrom_number) = chromosome.strip_prefix("chr") else {
            // search.py:179-180: the chromosome must start with "chr".
            return Some(invalid_location_leaf());
        };
        let Some((begin, end)) = parse_position(position_spec) else {
            // The ValueError escapes gn3's split_query into the same
            // except-ValueError arm (search.py:197-201).
            return Some(invalid_location_leaf());
        };
        Some(Query::combine(
            Operator::And,
            Query::term(self.species_term, None, None),
            Query::combine(
                Operator::And,
                Query::term(format!("XC{chrom_number}"), None, None),
                position_value_query(begin, end),
            ),
        ))
    }
}

/// Guard the gn3 numeric-range contract at the app level (C3.4).
///
/// Xapian's C++ `NumberRangeProcessor` rejects a range whose non-empty
/// sides are not valid floating-point numbers, and the query parser then
/// throws `QueryParserError("Unknown range operation")` — the recorded
/// production diagnostic for `mean:abc..def` (Q52, a 400 at gn3, graded
/// 422 `invalid_query` here). The fork's decline→MatchNothing bridge
/// (commit aa915af, AMENDMENT A) turns those declines into accepted
/// empty-match queries instead (measured: `mean:abc..def` parses Ok to
/// MatchNothing), so the engine re-checks the contract itself — the same
/// app-level-guard pattern the port pins for the Hs/Mm invalid leaf
/// (PORT-DESIGN C3.6, evidence-backed option (b); no shim extension).
///
/// The scan is deliberately conservative: it only looks at unquoted
/// `<prefix>:` token runs that contain `..` (xapian consults range
/// processors only for range syntax; a quoted range is phrase content
/// and is skipped, matching the parser). Markers are case-sensitive, as
/// xapian's are. Known residuals, neither graded: exotic forms accepted
/// by Rust's `f64` parser but not C++ (`inf`/`nan`) pass this guard, and
/// the both-empty degenerate range (`mean:..`) stays accepted (the
/// fork's MatchNothing — binding-report R3's documented `("","")`
/// divergence; no production shape uses it).
fn guard_numeric_ranges(q: &str) -> Result<(), SearchError> {
    let invalid_range = || SearchError::InvalidQuery {
        // gn3's recorded production diagnostic for this class (Q52:
        // xapian's QueryParserError "Unknown range operation").
        message: "Unknown range operation".to_string(),
        xapian_error_type: Some("QueryParserError".to_string()),
    };
    let bytes = q.as_bytes();
    let mut in_quotes = false;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            in_quotes = !in_quotes;
            i += 1;
            continue;
        }
        if !in_quotes {
            if let Some(&(marker, _slot)) = NUMERIC_RANGE_PREFIXES
                .iter()
                .find(|&&(marker, _)| q[i..].starts_with(marker))
            {
                // A marker only starts a field at a token boundary:
                // `xmean:` never reaches the range processor either.
                let token_start =
                    i == 0 || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_');
                if token_start {
                    // The value runs to whitespace, a parenthesis or a
                    // quote — the same characters that end a field value
                    // for the parser.
                    let value = q[i + marker.len()..]
                        .split(|c: char| c.is_whitespace() || matches!(c, '(' | ')' | '"'))
                        .next()
                        .unwrap_or("");
                    if let Some((begin, end)) = value.split_once("..") {
                        for side in [begin, end] {
                            if !side.is_empty() && side.parse::<f64>().is_err() {
                                return Err(invalid_range());
                            }
                        }
                    }
                }
            }
        }
        i += 1;
    }
    Ok(())
}

/// The synchronous search path: parse, open, filter, retrieve, map.
///
/// GN4's file wrapped this in `Native::search`, an async shell that ran
/// it on `spawn_blocking` because xapian calls are synchronous C++ work.
/// That shell stays in GN4 (`src/search/gnindex.rs`); here it is the
/// function itself, and [`crate::search`] is the thin entry over it.
/// `index_path` is GN4's `XAPIAN_DB_PATH`, which this file used to read
/// from `crate::config`.
pub(crate) fn search_blocking(
    index_path: &str,
    q: &str,
    type_: &str,
    limit: u64,
    page: u64,
) -> Result<Vec<SearchHit>, SearchError> {
    // 1. Parse first (search.py:291 parses before :296 opens the
    //    index), through the checked-parse API: a malformed query is a
    //    typed error, not a panic (the fork's parse_query_checked).
    let parsed = {
        let mut parser = parser().lock().expect("native parser mutex poisoned");
        parser
            .0
            .parse_query_checked::<&str>(q, None, None)
            .map_err(|e| SearchError::InvalidQuery {
                message: e.message,
                xapian_error_type: Some(e.error_type),
            })?
    };

    // 2. Detect an invalid Hs/Mm leaf BEFORE the mset call (C3.6): the
    //    processors mark gn3's ValueError class (bad split, chromosome
    //    not starting with "chr", unparseable position) with a sentinel
    //    term, which surfaces here as InvalidQuery — the blessed 422 for
    //    the recorded production mset crash (Q42; AMENDMENT E). gn3's
    //    own leaf is OP_INVALID, which aborts under the binding; the
    //    app-level guard is the evidence-backed option (b) (PORT-DESIGN
    //    C3.6, OPEN-31 closed) and deliberately does NOT extend the C++
    //    try/catch shim. An invalid leaf anywhere in the tree poisons
    //    the whole query, exactly as gn3's mset-time rejection did.
    if parsed
        .unique_terms()
        .any(|term| term.to_string() == INVALID_LOCATION_SENTINEL)
    {
        return Err(SearchError::InvalidQuery {
            message: "InvalidOperationError: Query is invalid (invalid Hs/Mm location field)"
                .to_string(),
            // The xapian exception class production actually threw for
            // this shape (the recorded Q42 body's own classification).
            xapian_error_type: Some("InvalidOperationError".to_string()),
        });
    }

    // 3. Guard the numeric-range contract the fork's decline bridge no
    //    longer enforces at the parser level (C3.4; see
    //    [`guard_numeric_ranges`]). Parser syntax errors keep
    //    precedence, as in gn3; the guard runs before the index is
    //    opened so `mean:abc..def` reports as a query fault (Q52) even
    //    on a misconfigured deployment.
    guard_numeric_ranges(q)?;

    // 3. Open the index.
    let db_path = index_path;
    let db = open_database(db_path)?;

    // 4. Filter to the document type: OP_FILTER(parsed, "XT" + type)
    //    (search.py:298-300; XT terms are written at index time by the
    //    production indexer).
    let filtered = Query::combine(
        Operator::Filter,
        parsed,
        Query::term(format!("XT{type_}"), None, None),
    );

    // 5. Retrieve the full match set — the forwarded `limit` is
    //    deliberately not applied to the fetch size (per-page
    //    invariant, §5); `page` shifts the retrieval window for
    //    unsorted paging, exactly as gn3's per_page/page pair does.
    //    Sorted calls pass page=1 (the route ranks and slices).
    let first_item = (page.saturating_sub(1).saturating_mul(limit)).min(u32::MAX as u64) as u32;
    let mut enquire = Enquire::new(&db);
    enquire.set_query(&filtered, None);
    let mset = enquire.mset(first_item, FULL_MATCH_SET_MAX_ITEMS, None, None);

    // 6. Map each hit's stored doc data (the indexer's JSON, the
    //    schema gn3 serves verbatim: json.loads(document.get_data()),
    //    search.py:302) onto SearchHit; unknown fields flow through
    //    `extra`.
    let mut hits = Vec::with_capacity(mset.size() as usize);
    for mset_item in mset.matches() {
        let data = mset_item.document().data();
        hits.push(hit_from_doc_data(&data)?);
    }
    Ok(hits)
}

/// Open the xapian index at `path`, with the pre-validation the binding
/// needs (its `Database::open` aborts on failure instead of returning
/// an error). The file checks cover the misconfiguration classes an
/// operator can actually hit; see the module doc for the corrupt-index
/// residual.
fn open_database(path: &str) -> Result<Database, SearchError> {
    if path.is_empty() {
        return Err(SearchError::Internal(
            "XAPIAN_DB_PATH is not configured; set it to the xapian index directory to \
             use the native search engine (SEARCH_ENGINE=native)"
                .to_string(),
        ));
    }
    let metadata = std::fs::metadata(path).map_err(|e| {
        SearchError::Unreachable(format!("xapian index at {path} could not be opened: {e}"))
    })?;
    if !metadata.is_dir() {
        return Err(SearchError::Unreachable(format!(
            "xapian index at {path} is not a directory"
        )));
    }
    // The glass backend writes this marker into every index directory;
    // it distinguishes a real index from an arbitrary directory.
    if !std::path::Path::new(path).join("iamglass").is_file() {
        return Err(SearchError::Unreachable(format!(
            "xapian index at {path} does not look like a glass index (no iamglass marker)"
        )));
    }
    Ok(Database::open(path, None))
}

/// Map one hit's stored document data onto [`SearchHit`]. The data is
/// the indexer's own JSON (`doc.set_data(json.dumps(trait.data))`,
/// index-genenetwork:361/408), so decode failures mean the index does
/// not hold what gn3 would serve — `Misbehaving`, the doc-data analogue
/// of an undecodable backend response body.
fn hit_from_doc_data(data: &[u8]) -> Result<SearchHit, SearchError> {
    serde_json::from_slice::<SearchHit>(data)
        .map_err(|e| SearchError::Misbehaving(format!("xapian doc data is not a search hit: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hermetic parser tests: no index, no environment, just the
    /// grammar configuration and the checked-parse contract.
    #[test]
    fn checked_parse_rejects_and_accepts_like_gn3() {
        let mut qp = build_query_parser();

        // search.py:291: a truncated AND expression is a
        // QueryParserError; the checked API surfaces it typed (the
        // fork's round-3 R1 gate) instead of panicking.
        let err = qp
            .parse_query_checked::<&str>("alcohol AND", None, None)
            .expect_err("alcohol AND must be rejected");
        assert_eq!(err.error_type, "QueryParserError");
        assert_eq!(err.message, "Syntax: <expression> AND <expression>");

        // The && shapes: production (and libxapian 1.4.29) serve these
        // as 200s (Q47/Q54, harness-report §4 F2/F4), so they must
        // parse Ok here too.
        qp.parse_query_checked::<&str>("a&&b", None, None)
            .expect("a&&b parses (Q47)");
        qp.parse_query_checked::<&str>("alcohol &&", None, None)
            .expect("alcohol && parses (Q54)");

        // An unregistered prefix is plain text (Q50, foo:bar).
        qp.parse_query_checked::<&str>("foo:bar", None, None)
            .expect("unregistered prefixes parse as text (Q50)");
    }

    /// The boolean-prefix registrations (C3.2): the registered
    /// prefixes must leave the free-text path and produce their exact
    /// index terms, `group` lowercasing its value on the way. Hermetic:
    /// assertions read the parsed query terms, no index involved.
    #[test]
    fn boolean_prefixes_parse_to_exact_index_terms() {
        let mut qp = build_query_parser();
        let mut terms = |query: &str| -> std::collections::BTreeSet<String> {
            qp.parse_query_checked::<&str>(query, None, None)
                .unwrap_or_else(|e| panic!("{query} must parse: {e:?}"))
                .unique_terms()
                .map(|t| t.to_string())
                .collect()
        };

        // search.py:220-228: prefix -> index term (lowercase values
        // concatenate, the case the graded shapes use).
        assert!(terms("author:mulligan").contains("Amulligan"));
        assert!(terms("species:mouse").contains("XSmouse"));
        assert!(terms("tissue:brain").contains("XIbrain"));
        assert!(terms("dataset:bxdpublish").contains("XDSbxdpublish"));
        assert!(terms("symbol:insr").contains("XYinsr"));
        assert!(terms("chr:1").contains("XC1"));
        assert!(terms("peakchr:1").contains("XPC1"));
        // search.py:204-209, 222-223: group values lowercase (xapian
        // does not stem boolean fields starting with a capital):
        // B6 -> b6, concatenated behind XG. (The r1-test G4 smoke's
        // XGbxd was the same rule for value BXD, not a stem.)
        let b6 = terms("group:B6");
        assert!(
            b6.contains("XGb6"),
            "group:B6 must lowercase to XGb6, got {b6:?}"
        );
        assert!(
            !b6.contains("XGB6"),
            "group must not emit the raw-case term"
        );

        // Every boolean prefix lowercases its value (LowercaseProcessor):
        // the index holds no mixed-case terms at all, so a caller writing
        // `dataset:BXDPublish` or `author:Williams` must still hit them.
        // Pinned per prefix so a registration that goes back to an exact
        // `add_boolean_prefix` fails here rather than silently returning
        // zero hits in production.
        for (query, term) in [
            ("author:Williams", "Awilliams"),
            ("species:Mouse", "XSmouse"),
            ("tissue:mRNA", "XImrna"),
            ("dataset:BXDPublish", "XDSbxdpublish"),
            ("symbol:BDNF", "XYbdnf"),
            ("chr:X", "XCx"),
            ("peakchr:X", "XPCx"),
        ] {
            let parsed = terms(query);
            assert!(
                parsed.contains(term),
                "{query} must lowercase to {term}, got {parsed:?}"
            );
        }
    }

    /// The probabilistic-prefix registrations (C3.3): description, rif,
    /// wiki and the uppercase aliases must parse as weighted field
    /// terms, not free text (search.py:229-233). With STEM_ALL_Z the
    /// field term is Z+<prefix>+stem — ZXDinsulin, ZXRF…, ZXWK… — the
    /// exact shape the production indexer's TermGenerator emits for
    /// these fields (confirmed against the dev index: term
    /// ZXWKdiabet exists with 24 gene documents, ZXRF-prefixed terms
    /// abound; d.v. the delve probes in LANE-REPORT).
    #[test]
    fn probabilistic_prefixes_and_aliases_parse_as_field_terms() {
        let mut qp = build_query_parser();
        let mut prefixed = |query: &str, prefix: &str| {
            let set = qp
                .parse_query_checked::<&str>(query, None, None)
                .unwrap_or_else(|e| panic!("{query} must parse: {e:?}"))
                .unique_terms()
                .map(|t| t.to_string())
                .collect::<std::collections::BTreeSet<_>>();
            set.iter().any(|t| t.starts_with(prefix))
        };
        assert!(prefixed("description:insulin", "ZXD"));
        assert!(prefixed("rif:insulin", "ZXRF"));
        assert!(prefixed("RIF:insulin", "ZXRF"));
        assert!(prefixed("wiki:diabetes", "ZXWK"));
        assert!(prefixed("WIKI:diabetes", "ZXWK"));
    }

    /// The numeric-range registrations (C3.4): each prefix must produce a
    /// value leaf on its OWN slot (search.py:234-243; slots verified
    /// literal at index-genenetwork:293-299), with one-sided ranges on
    /// gn3's C++ NumberRangeProcessor semantics (VALUE_LE / VALUE_GE —
    /// the fork's bridge, measured in the lane's probe). The slot in the
    /// query description is the pin: a registration wired to the wrong
    /// slot goes red here.
    #[test]
    fn numeric_ranges_parse_to_value_leaves_on_their_slots() {
        let mut qp = build_query_parser();
        let description = |qp: &mut QueryParser, q: &str| {
            qp.parse_query_checked::<&str>(q, None, None)
                .unwrap_or_else(|e| panic!("{q} must parse: {e:?}"))
                .to_string()
        };

        for (marker, slot) in NUMERIC_RANGE_PREFIXES {
            let query = format!("{marker}1..2");
            let expected = format!("VALUE_RANGE {slot} ");
            let got = description(&mut qp, &query);
            assert!(
                got.contains(&expected),
                "{query} must parse to `{expected}…` on slot {slot}, got {got:?}"
            );
        }

        // One-sided and open forms keep gn3's one-sided C++ semantics
        // (probe: VALUE_LE 0 / VALUE_GE 0), not a 0.0/1e18 pair.
        assert!(description(&mut qp, "mean:..15").contains("VALUE_LE 0 "));
        assert!(description(&mut qp, "mean:10..").contains("VALUE_GE 0 "));
        assert!(description(&mut qp, "year:..2020").contains("VALUE_LE 5 "));

        // A non-range use of the prefix is plain text (gn3 measures the
        // same: `mean:5` is a `mean 5` phrase, never a range).
        let mean5 = description(&mut qp, "mean:5");
        assert!(!mean5.contains("VALUE"), "mean:5 is text, got {mean5:?}");

        // The fork's decline→MatchNothing bridge: an unparseable range
        // PARSES (the bridge prevents the OP_INVALID abort); the typed
        // rejection is the engine-level guard's job (next test).
        let declined = description(&mut qp, "mean:abc..def");
        assert!(
            !declined.contains("VALUE"),
            "declined range must be MatchNothing"
        );
    }

    /// The C3.4 app-level guard: `mean:abc..def` must surface as
    /// InvalidQuery with gn3's recorded production diagnostic
    /// ("Unknown range operation", Q52), while every valid form —
    /// closed, one-sided, exponent, mixed boolean queries — passes.
    #[test]
    fn numeric_range_guard_rejects_unparseable_bounds() {
        for query in [
            "mean:abc..def",
            "(mean:abc..def)",
            "year:twenty..thirty",
            "peak:abc..",
            "additive:..def",
            "peakmb:1.5..x",
            "mean:10..20..30",
        ] {
            match guard_numeric_ranges(query) {
                Err(SearchError::InvalidQuery {
                    message,
                    xapian_error_type: Some(xapian_error_type),
                }) => {
                    assert_eq!(message, "Unknown range operation", "for {query:?}");
                    assert_eq!(xapian_error_type, "QueryParserError", "for {query:?}");
                }
                other => {
                    panic!("{query:?} must be rejected with a typed diagnostic, got {other:?}")
                }
            }
        }
        // Q52's exact recorded shape, once more on its own.
        assert!(matches!(
            guard_numeric_ranges("mean:abc..def"),
            Err(SearchError::InvalidQuery {
                ref message,
                ref xapian_error_type,
            }) if message == "Unknown range operation"
                && xapian_error_type.as_deref() == Some("QueryParserError")
        ));

        for query in [
            "mean:10..20",
            "mean:..15",
            "mean:10..",
            "mean:..",
            "mean:1e3..2e3",
            "mean:99999..100000",
            "author:mulligan AND year:2010..2015",
            "year:2010..2020 AND peak:5..15",
            // Not range syntax: no "..", so xapian treats it as text.
            "mean:abc",
            // Case-sensitive markers, as in xapian.
            "Mean:abc..def",
            // Mid-token occurrences never reach the processor.
            "xmean:abc..def",
            // Quoted content is phrase text, never a range.
            "\"mean:abc..def\"",
            // Unregistered prefixes are free text (Q50).
            "foo:abc..def",
        ] {
            assert!(
                guard_numeric_ranges(query).is_ok(),
                "{query:?} must pass the range guard"
            );
        }
    }

    /// The position/pos registrations (C3.5): SI-suffixed base-pair
    /// coordinates parse to slot-2 MEGABASE value queries, points expand
    /// to ±50 kb clamped at 0, and unparseable specs decline to the
    /// bridge's MatchNothing (verified end-to-end in the dev-index gate:
    /// `position:abc` serves 200 with 0 hits, never a process abort).
    #[test]
    fn position_prefixes_parse_si_suffixed_slot_two_queries() {
        let mut qp = build_query_parser();
        let parse = |qp: &mut QueryParser, q: &str| {
            qp.parse_query_checked::<&str>(q, None, None)
                .unwrap_or_else(|e| panic!("{q} must parse: {e:?}"))
        };

        // Q32/Q34's shape: 1.05M..1.06M bases -> 1.05..1.06 Mb on slot 2.
        for q in ["position:1.05M..1.06M", "pos:1.05M..1.06M"] {
            let query = parse(&mut qp, q);
            assert!(
                query.to_string().contains("VALUE_RANGE 2 "),
                "{q} must be a slot-2 value range, got {query}"
            );
        }

        // Q33's shape: a point expands to ±50 kb (7 500 000 bases ->
        // 7.45..7.55 Mb).
        assert!(parse(&mut qp, "position:7500000")
            .to_string()
            .contains("VALUE_RANGE 2 "));

        // SI suffixes are case-insensitive (search.py:136-142 lowercases
        // the last character).
        assert!(parse(&mut qp, "position:1.05m..1.06m")
            .to_string()
            .contains("VALUE_RANGE 2 "));
        assert!(parse(&mut qp, "position:250k..500k")
            .to_string()
            .contains("VALUE_RANGE 2 "));

        // One-sided position ranges keep gn3's one-sided semantics.
        assert!(parse(&mut qp, "position:1M..")
            .to_string()
            .contains("VALUE_GE 2 "));
        assert!(parse(&mut qp, "position:..1M")
            .to_string()
            .contains("VALUE_LE 2 "));

        // The clamp: a point below 50 kb clamps its lower bound at 0
        // (search.py:151-155) — pinned by the formula's parse form, the
        // same slot-2 range.
        assert!(parse(&mut qp, "position:25000")
            .to_string()
            .contains("VALUE_RANGE 2 "));

        // The MatchNothing bridge (C3.5's verification requirement): an
        // unparseable spec parses to an empty-match query — never an
        // <INVALID> leaf, never a process abort (fork commit aa915af,
        // tested end-to-end against the dev index in the shape gate and
        // the route scenarios). `position:1.5` declines too: gn3's
        // unsuffixed values must be integers (int(location),
        // search.py:142).
        for q in ["position:abc", "pos:abc", "position:1.5", "position:1e3"] {
            let query = parse(&mut qp, q);
            assert!(
                !query.to_string().contains("VALUE"),
                "{q} must decline to MatchNothing, got {query}"
            );
        }
        // Xapian documents MatchNothing as boolean false in OR trees:
        // `position:abc OR alcohol` still parses and keeps its real arm.
        let combined = parse(&mut qp, "position:abc OR alcohol");
        let terms: Vec<String> = combined.unique_terms().map(|t| t.to_string()).collect();
        assert!(
            terms.iter().any(|t| t.starts_with("Zalcohol")),
            "the real OR arm must survive, got {terms:?}"
        );
    }

    /// The Hs/Mm registrations (C3.6): valid locations expand to the
    /// gn3 triplet — species term AND chromosome term AND slot-2 range
    /// (search.py:185-193, no liftover); every gn3 ValueError condition
    /// produces the sentinel leaf instead (search.py:178-183, 197-201),
    /// which the engine guard maps to InvalidQuery before any mset call.
    #[test]
    fn hs_mm_locations_parse_to_species_chromosome_ranges() {
        let mut qp = build_query_parser();
        let parse = |qp: &mut QueryParser, q: &str| {
            qp.parse_query_checked::<&str>(q, None, None)
                .unwrap_or_else(|e| panic!("{q} must parse: {e:?}"))
        };
        let terms = |query: &Query| {
            query
                .unique_terms()
                .map(|t| t.to_string())
                .collect::<std::collections::BTreeSet<_>>()
        };

        // Q40's shape: Hs:chr2:1M..1.2M -> XShuman AND XC2 AND a slot-2
        // range (r2-grammar's measured parse form, without liftover).
        let hs = parse(&mut qp, "Hs:chr2:1M..1.2M");
        let hs_terms = terms(&hs);
        assert!(hs_terms.contains("XShuman"), "got {hs_terms:?}");
        assert!(hs_terms.contains("XC2"), "got {hs_terms:?}");
        assert!(hs.to_string().contains("VALUE_RANGE 2 "), "got {hs}");

        // Q41's shape: Mm:chr1:5M -> XSmouse AND XC1 AND a ±50 kb range.
        let mm = parse(&mut qp, "Mm:chr1:5M");
        let mm_terms = terms(&mm);
        assert!(mm_terms.contains("XSmouse"), "got {mm_terms:?}");
        assert!(mm_terms.contains("XC1"), "got {mm_terms:?}");
        assert!(mm.to_string().contains("VALUE_RANGE 2 "), "got {mm}");

        // The whole value lowercases before splitting (search.py:178).
        let upper = parse(&mut qp, "Mm:CHR1:5M");
        let upper_terms = terms(&upper);
        assert!(upper_terms.contains("XC1"), "got {upper_terms:?}");

        // Every gn3 ValueError condition marks the query with the
        // sentinel leaf — the engine guard's detection signal.
        let invalid = [
            "Hs:foo:1M",
            "Mm:foo:5M",
            "Hs:chr1",
            "Mm:chr1",
            "Hs:chr1:5M:extra",
            "Hs:chr1:abc",
            "Mm:chr1:1.5",
        ];
        for q in invalid {
            let query = parse(&mut qp, q);
            assert!(
                terms(&query).contains(INVALID_LOCATION_SENTINEL),
                "{q} must carry the invalid-location sentinel"
            );
        }

        // An invalid leaf poisons the whole query, as gn3's mset-time
        // OP_INVALID rejection did (search.py:293-301 has no try/except).
        let combined = parse(&mut qp, "Hs:foo:1M OR alcohol");
        assert!(terms(&combined).contains(INVALID_LOCATION_SENTINEL));

        // And valid locations never carry it.
        let clean = parse(&mut qp, "Mm:chr1:5M OR alcohol");
        assert!(!terms(&clean).contains(INVALID_LOCATION_SENTINEL));
    }

    /// The C3.6 engine guard: the sentinel anywhere in the parsed query
    /// surfaces as InvalidQuery BEFORE the index is opened — so the
    /// invalid-location class reports as a query fault (the blessed 422)
    /// whatever `XAPIAN_DB_PATH` holds, mirroring gn3's parse-before-open
    /// order (search.py:291 vs :296). The error VARIANT is the proof:
    /// an unset path would surface as Internal and an unopenable one as
    /// Unreachable if the query ever reached the index. No environment
    /// is touched: this test runs in the lib binary next to the config
    /// tests, which mutate `XAPIAN_DB_PATH` on their own threads.
    #[test]
    fn hs_mm_engine_guard_fires_before_the_index() {
        // Moved: GN4 drove this through `Native::search` on a tokio runtime.
        // The async shell stayed there, so the guard is driven directly here
        // — an empty index path would be `Internal` and an unopenable one
        // `Unreachable` if the query ever reached the index.
        let outcome = search_blocking("", "Hs:foo:1M", "phenotype", 5, 1);
        assert!(
            matches!(outcome, Err(SearchError::InvalidQuery { .. })),
            "the guard must report InvalidQuery before the index is touched"
        );
    }

    /// The index-open guards, without touching a real index.
    #[test]
    fn open_database_maps_failures_onto_the_seam_vocabulary() {
        assert!(matches!(open_database(""), Err(SearchError::Internal(_))));
        assert!(matches!(
            open_database("/nonexistent/gn4-native-test-index"),
            Err(SearchError::Unreachable(_))
        ));
        // A real directory that is not an index.
        let not_an_index =
            std::env::temp_dir().join(format!("gn4-not-an-index-{}", std::process::id()));
        std::fs::create_dir_all(&not_an_index).unwrap();
        assert!(matches!(
            open_database(not_an_index.to_str().unwrap()),
            Err(SearchError::Unreachable(_))
        ));
        std::fs::remove_dir_all(&not_an_index).unwrap();
    }
}
