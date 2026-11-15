#![cfg(feature = "fixture")]
//! The parity corpus runner: GN4's 54 recorded query shapes, replayed
//! through this crate and graded — with GN4 never compiling.
//!
//! FIXTURE-BACKED, NOT PARITY. This target needs `--features fixture`: the
//! fixture double (`src/index/fixture.rs`) is what answers the match-set
//! shapes, and no real xapian index is involved anywhere. The error
//! classes, by contrast, are graded through the production entry point
//! (`gn_search::search`), because the engine decides them before it opens
//! an index.
//!
//! SCOUT sections implemented: SCOUT-NATIVE §3 (the grammar those shapes
//! exercise), §6 (the error classes they record) and SCOUT-PROXY §4 (what
//! the parity metric grades: full-set identity of `(dataset, name)` pairs,
//! and a Jaccard floor of 0.8 on the top ten).
//!
//! # What this is for
//!
//! `BASELINE.md` §4: the acceptance test for this crate cannot be GN4's own
//! parity harness, because that needs a full cold Rust build of GN4. It
//! needs the CORPUS plus this crate. That is this file: it reads the
//! committed recordings (`test/data/parity-fixtures/Q*.json`, reduced-v1),
//! re-renders every shape's `raw_query` through [`GRAMMAR`], runs it against
//! the in-memory fixture index and reports one line per shape.
//!
//! # What it grades, and what it refuses to grade
//!
//! Three things are index-independent and always graded:
//!
//! * **the query class** — a shape recorded as an error (400/500) must come
//!   back as the crate's `InvalidQuery`, carrying the exception class GN4
//!   maps to 422 `invalid_query` (Q51/Q52 `QueryParserError`, Q42
//!   `InvalidOperationError` per AMENDMENT E);
//! * **no false rejection** — a shape recorded as a 200 must never be
//!   rejected by the grammar (that would be a 422 where production answered;
//!   the failure mode F2 in `docs/REVIEW-CRATE.md` was exactly this);
//! * **the match set, where the fixture can actually answer it** — see
//!   [`fixture_blocker`].
//!
//! The fixture index holds the five recorded alcohol phenotypes and nothing
//! else, and its matching is a double: substring instead of stemming, no
//! term dictionary, no boolean structure, one hit field per slot. A shape
//! whose terms fall outside that is reported `SKIP` with the reason — never
//! `PASS`, and never `FAIL`, because a difference there would grade the
//! double rather than the crate. Every `SKIP` here is attributable to one
//! absence and [`SKIP_ATTRIBUTION`] names it: the fixture double has five
//! documents and no term dictionary, so the shapes it cannot answer need the
//! real xapian index this runner does not have.
//!
//! # Running it
//!
//! ```sh
//! cargo test --test parity_corpus -- --nocapture
//! # a different corpus (same variable GN4's harness honours):
//! GN4_PARITY_FIXTURES=/path/to/parity-fixtures cargo test --test parity_corpus -- --nocapture
//! ```

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use gn_search::grammar::{ParsedQuery, TermKind, GRAMMAR};
use gn_search::index::fixture::FixtureIndex;
use gn_search::{Index, SearchError, SearchHit};

/// The corpus's own clause-(b) floor (GN4 `tests/search_parity.rs:64`).
const JACCARD_FLOOR: f64 = 0.8;
/// The evidence floor: how many shapes must complete a set-equality or
/// Jaccard comparison before this run counts as evidence about the grammar.
///
/// A lower bound, not a target. On the committed corpus five shapes compare a
/// NON-EMPTY match set (Q15, Q29, Q31, Q36, Q37) and the other seven
/// set-equality shapes are `VACUOUS` — the expected set is empty because the
/// five-document fixture holds none of the recorded documents — so a thinned
/// corpus, a thinned fixture or an all-SKIP run falls below the floor and the
/// test fails instead of printing green (`REVIEW-FIXES.md` §b(ii), §d
/// blocker 2). Raise it as the fixture grows; never lower it to make a run
/// pass.
///
/// The count is a coarse backstop; [`MUST_GRADE`] names the shapes it is made
/// of, because "five comparisons" is not the same claim as "these five".
const MIN_GRADED: usize = 5;
/// The shapes that must grade a NON-VACUOUS set-equality comparison and
/// `PASS` on the committed corpus — the five `MIN_GRADED` is made of today.
///
/// A count cannot protect the evidence it counts: the documented growth path
/// is a sixth gradable shape, and the moment it lands, one of these five
/// degrading from `set-eq` to `SKIP` (a grammar regression that turns its
/// terms into free text skips rather than FAILs) leaves the count at five and
/// the run green while it lost a third of its real evidence. So each of these
/// names must be present with `graded.is_match_set()` AND `Verdict::Pass`
/// (`REVIEW-BLOCKERS.md` §5 blocker 3).
///
/// Only Q31 (`peak:5..15`) and Q36 (`additive:0.5..1.0`) are two-sided;
/// Q15/Q29/Q37 expect the fixture's whole inventory, so they catch an
/// under-matching renderer only (`docs/CORPUS-RUNNER.md`).
const MUST_GRADE: [&str; 5] = ["Q15", "Q29", "Q31", "Q36", "Q37"];
/// Where the committed corpus lives; `GN4_PARITY_FIXTURES` overrides it
/// (GN4's own harness reads the same variable).
const DEFAULT_CORPUS: &str = "/home/kabui/project5/genenetwork4/test/data/parity-fixtures";
/// The override variable.
const CORPUS_ENV: &str = "GN4_PARITY_FIXTURES";
/// The index path the fixture backend answers: an EMPTY one, because a
/// non-empty path is refused by design (`src/index/fixture.rs`) — this
/// runner must not pretend to have a real index.
const FIXTURE_PATH: &str = "";

/// What the summary line attributes this run's `SKIP`s to: the absence THIS
/// runner has. There is one such absence now — no real index — because the
/// fixture backend is compiled in by the file's own `--features fixture` gate
/// and the engine is always compiled in. It used to be per-build (the old
/// `xapian` feature build had no fixture inventory), which made a summary name
/// the absence of the very thing that build compiled in (`VERIFY-REMAINING.md`
/// F2).
const SKIP_ATTRIBUTION: &str =
    "the fixture double's: these shapes need a real xapian index, and this runner has none";

/// The `(dataset, name)` identity of a hit, both sides optional so an absent
/// field is a distinct key and never coerced to `""` — the corpus's own
/// `HitKey` (`tests/search_parity.rs:459-461`).
type HitKey = (Option<String>, Option<String>);

// ---------------------------------------------------------------------------
// The corpus
// ---------------------------------------------------------------------------

/// One recorded shape, as the reduced-v1 corpus stores it.
#[derive(Debug)]
struct Shape {
    id: String,
    raw_query: String,
    type_: String,
    recorded_status: u64,
    /// Absent on error shapes (they carry a diagnostic, not a hit set).
    recorded_hits: Option<u64>,
    identifiers: BTreeSet<HitKey>,
    top10: Vec<HitKey>,
}

impl Shape {
    fn is_error(&self) -> bool {
        self.recorded_status != 200
    }

    /// The exception class this shape's recorded error maps to. A recorded
    /// 4xx is xapian's parser rejecting the query; the one recorded 500
    /// (Q42) is the invalid-location class, re-baselined to 422 by
    /// AMENDMENT E (`tests/search_parity.rs:82-88`).
    fn expected_error_type(&self) -> &'static str {
        match self.recorded_status {
            500 => "InvalidOperationError",
            _ => "QueryParserError",
        }
    }
}

fn corpus_dir() -> PathBuf {
    match std::env::var(CORPUS_ENV) {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => PathBuf::from(DEFAULT_CORPUS),
    }
}

/// Load every `Q*.json` in `dir`, sorted by id.
///
/// Loud on anything unexpected: a missing corpus, a file that is not
/// reduced-v1, or an unparsable file. A silently smaller corpus would grade
/// less than it claims.
fn load_shapes(dir: &Path) -> Vec<Shape> {
    let entries = std::fs::read_dir(dir).unwrap_or_else(|error| {
        panic!(
            "cannot read the parity corpus at {}: {error}. The corpus is GN4's \
             `test/data/parity-fixtures`; point {CORPUS_ENV} at a copy if the GN4 tree \
             is not at {DEFAULT_CORPUS}",
            dir.display()
        )
    });
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension().and_then(|extension| extension.to_str()) == Some("json")
                && path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .is_some_and(|stem| stem.starts_with('Q'))
        })
        .collect();
    paths.sort();
    let mut shapes: Vec<Shape> = paths.iter().map(|path| load_shape(path)).collect();
    shapes.sort_by(|a, b| a.id.cmp(&b.id));
    shapes
}

fn load_shape(path: &Path) -> Shape {
    let bytes = std::fs::read(path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("{} is not JSON: {error}", path.display()));
    assert!(
        value.is_object(),
        "{} is a full recording (a top-level array), not reduced-v1; this runner reads the \
         reduced corpus",
        path.display()
    );
    let field = |name: &str| {
        value.get(name).cloned().unwrap_or_else(|| {
            panic!(
                "{} has no {name:?}; it is not a reduced-v1 fixture",
                path.display()
            )
        })
    };
    let keys = |name: &str| -> Vec<HitKey> {
        value
            .get(name)
            .and_then(serde_json::Value::as_array)
            .map(|pairs| {
                pairs
                    .iter()
                    .map(|pair| {
                        let pair = pair.as_array().unwrap_or_else(|| {
                            panic!("{}: {name} entry is not a pair: {pair}", path.display())
                        });
                        let read = |index: usize| {
                            pair.get(index)
                                .and_then(serde_json::Value::as_str)
                                .map(str::to_string)
                        };
                        (read(0), read(1))
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    Shape {
        id: field("id")
            .as_str()
            .unwrap_or_else(|| panic!("{}: id is not a string", path.display()))
            .to_string(),
        raw_query: field("raw_query")
            .as_str()
            .unwrap_or_else(|| panic!("{}: raw_query is not a string", path.display()))
            .to_string(),
        type_: field("type")
            .as_str()
            .unwrap_or_else(|| panic!("{}: type is not a string", path.display()))
            .to_string(),
        recorded_status: field("recorded_status")
            .as_u64()
            .unwrap_or_else(|| panic!("{}: recorded_status is not a number", path.display())),
        recorded_hits: value.get("hit_count").and_then(serde_json::Value::as_u64),
        identifiers: keys("identifiers").into_iter().collect(),
        top10: keys("top10"),
    }
}

// ---------------------------------------------------------------------------
// The metric (the corpus's own: set equality, and Jaccard >= 0.8 on the top ten)
// ---------------------------------------------------------------------------

fn hit_key(hit: &SearchHit) -> HitKey {
    (hit.dataset.clone(), hit.name.clone())
}

fn key_set(hits: &[SearchHit]) -> BTreeSet<HitKey> {
    hits.iter().map(hit_key).collect()
}

fn top_ten(hits: &[SearchHit]) -> Vec<HitKey> {
    hits.iter().take(10).map(hit_key).collect()
}

/// `|A ∩ B| / |A ∪ B|`, with an empty union defined as 1.0 — the corpus's
/// own definition (`tests/search_parity.rs:587-593`).
fn jaccard(a: &BTreeSet<HitKey>, b: &BTreeSet<HitKey>) -> f64 {
    let union = a.union(b).count();
    if union == 0 {
        return 1.0;
    }
    a.intersection(b).count() as f64 / union as f64
}

/// Clause (b) applies only where the recorded set has at least ten hits
/// (`tests/search_parity.rs:68-76`); `None` means it does not apply.
fn clause_b_pass(recorded_hits: u64, j: f64) -> Option<bool> {
    if recorded_hits >= 10 {
        Some(j >= JACCARD_FLOOR)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// What the fixture index can and cannot answer
// ---------------------------------------------------------------------------

/// Why this shape's match set cannot be graded on the fixture index, or
/// `None` when it can.
///
/// Each answer is a limit of the DOUBLE, not of the shape: the fixture holds
/// five recorded alcohol phenotype documents, it approximates free text with
/// a substring test, it has no term dictionary and no boolean structure, and
/// it has one hit field per value slot (`src/index/fixture.rs`, module doc).
/// A shape that needs more than that needs the real xapian index, which this
/// runner does not have.
fn fixture_blocker(shape: &Shape, parsed: &ParsedQuery) -> Option<String> {
    let needs_xapian = "the fixture double cannot answer it, and no xapian index is configured";
    let recorded_count = match shape.recorded_hits {
        Some(count) => count.to_string(),
        None => "no".to_string(),
    };
    if is_phrase_shape(parsed) {
        return Some(format!(
            "phrase-not-expressible: the rendered terms say these words are ADJACENT \
             (`RenderedTerm::phrase` — a quoted region, or a field token whose value the grammar \
             does not consume, e.g. `year:2014` is xapian's phrase `year 2014`), and the fixture \
             double can only AND its five documents. GN4's index carries no positions, so it \
             records 0 hits for a phrase and for NEAR/ADJ (genenetwork4/docs/search-engines.md:98-102; \
             DECISIONS rows 35, 36) ({needs_xapian})"
        ));
    }
    if shape.type_ != "phenotype" {
        return Some(format!(
            "the fixture index holds the five recorded alcohol phenotype documents only; this \
             shape recorded {recorded_count} {} documents in the production index \
             ({needs_xapian})",
            shape.type_
        ));
    }
    if parsed.terms.is_empty() {
        return Some(format!(
            "GN4's route rejects an empty q with 400 before the engine ever sees it \
             (SYNTHESIS §2d), so there is nothing at this seam to grade ({needs_xapian} for \
             the recorded gn3 200)"
        ));
    }
    for term in &parsed.terms {
        let blocker = match term.kind {
            TermKind::FreeText => Some(
                "free text is stemmed and scored by the real index; the fixture's stand-in is a \
                 case-insensitive substring test over five documents"
                    .to_string(),
            ),
            TermKind::Operator => Some(
                "boolean structure is the backend parser's job, and the fixture ANDs every term \
                 and drops operators"
                    .to_string(),
            ),
            TermKind::Probabilistic => Some(format!(
                "the {} text lives in the index's dictionaries and on no SearchHit field",
                term.prefix
            )),
            TermKind::ValueRange { slot, .. } => match slot {
                // Slots 0, 1, 2, 4 and 5 are hit fields (mean, lrs, geno_mb,
                // additive, year); 3 (peak-Mb) and 6 (sample count) are not.
                3 | 6 => Some(format!(
                    "value slot {slot} is not carried on SearchHit, so the fixture cannot \
                     compare it"
                )),
                _ => None,
            },
            TermKind::Boolean => {
                let field = GRAMMAR
                    .boolean_by_prefix(&term.prefix)
                    .map(|prefix| prefix.field)
                    .unwrap_or("(unregistered)");
                match field {
                    // The three the fixture compares exactly, against the
                    // field of the same name.
                    "dataset" | "group" | "species" => None,
                    "author" => Some(
                        "production matches an author term against the index's author terms \
                         (Q14: 765 hits for a surname); the fixture compares whole author names"
                            .to_string(),
                    ),
                    "chr" | "peakchr" => Some(
                        "which document field the index's chromosome term comes from is not \
                         visible in a SearchHit, so the fixture's geno_chr is not the same thing"
                            .to_string(),
                    ),
                    other => Some(format!(
                        "the fixture has no {other} hit field to match a {} term against",
                        term.prefix
                    )),
                }
            }
        };
        if let Some(reason) = blocker {
            return Some(format!("{reason} ({needs_xapian})"));
        }
    }
    None
}

/// Whether the shape needs phrase or positional structure the fixture double
/// cannot express: any `phrase`-marked term (the words of a quoted region, or
/// the label and value of a field token the grammar does not consume —
/// `year:2014` at GN4 is the phrase `year 2014`, `native.rs:721-724`), or a
/// `NEAR`/`ADJ` operator.
///
/// GN4's index carries no positions, so both classes parse and then match zero
/// documents (`genenetwork4/docs/search-engines.md:98-102`; Q04, Q11 and Q12
/// each recorded 0 hits). The renderer says "adjacent" with
/// [`RenderedTerm::phrase`](gn_search::grammar::RenderedTerm::phrase); the
/// fixture double can only AND, so these shapes are skipped BY NAME rather
/// than graded or passed (DECISIONS rows 35, 36).
fn is_phrase_shape(parsed: &ParsedQuery) -> bool {
    parsed.terms.iter().any(|term| {
        term.phrase
            || (term.kind == TermKind::Operator && matches!(term.term.as_str(), "NEAR" | "ADJ"))
    })
}

// ---------------------------------------------------------------------------
// Grading
// ---------------------------------------------------------------------------

/// One shape's outcome, in the shape the report prints.
#[derive(Debug)]
struct Row {
    id: String,
    raw_query: String,
    type_: String,
    recorded: String,
    ours: String,
    graded: Graded,
    verdict: Verdict,
}

/// What a shape's line actually graded — the `graded` column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Graded {
    /// Nothing (`-`): the shape was skipped, or its match set could not be
    /// compared.
    Nothing,
    /// The 422 exception class only.
    ErrorClass,
    /// Set equality of the `(dataset, name)` match set.
    SetEquality,
    /// Set equality plus clause (b), the top-ten Jaccard.
    SetEqualityTop10,
    /// The expected set was empty after the inventory restriction, so
    /// equality proves nothing (`VACUOUS`).
    Vacuous,
}

impl Graded {
    /// Whether this line graded a set-equality or Jaccard comparison — the
    /// only kind of line that is match-set evidence.
    fn is_match_set(self) -> bool {
        matches!(self, Self::SetEquality | Self::SetEqualityTop10)
    }
}

impl std::fmt::Display for Graded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::Nothing => "-",
            Self::ErrorClass => "error-class",
            Self::SetEquality => "set-eq",
            Self::SetEqualityTop10 => "set-eq+top10",
            Self::Vacuous => "vacuous",
        };
        f.write_str(label)
    }
}

#[derive(Debug)]
enum Verdict {
    Pass,
    Fail(String),
    /// The comparison ran but cannot fail: the expected set is empty because
    /// the fixture holds none of the documents the shape recorded. Printed
    /// as its own verdict so a reader cannot mistake it for a PASS, and
    /// never counted as graded evidence (`REVIEW-FIXES.md` §b(ii)).
    Vacuous(String),
    Skip(String),
}

impl Verdict {
    fn is_fail(&self) -> bool {
        matches!(self, Verdict::Fail(_))
    }
}

impl std::fmt::Display for Verdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Verdict::Pass => write!(f, "PASS"),
            Verdict::Fail(reason) => write!(f, "FAIL({reason})"),
            Verdict::Vacuous(reason) => write!(f, "VACUOUS({reason})"),
            Verdict::Skip(reason) => write!(f, "SKIP({reason})"),
        }
    }
}

impl Row {
    /// Whether this line is match-set evidence: a set-equality or Jaccard
    /// comparison that passed. A `VACUOUS` line, an error-class check and a
    /// SKIP are all excluded.
    fn is_graded_evidence(&self) -> bool {
        self.graded.is_match_set() && matches!(self.verdict, Verdict::Pass)
    }
}

/// The counts the summary line prints, and the numbers the evidence floor
/// is enforced on.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Counts {
    /// Shapes the corpus held.
    seen: usize,
    /// Shapes whose set equality or top-ten Jaccard passed.
    graded: usize,
    /// Shapes whose 422 exception class was graded.
    error_class: usize,
    /// Shapes that compared empty-vs-empty: no evidence either way.
    vacuous: usize,
    /// Shapes the fixture cannot answer at all.
    skipped: usize,
    passed: usize,
    failed: usize,
}

fn summarize(rows: &[Row]) -> Counts {
    let mut counts = Counts {
        seen: rows.len(),
        ..Counts::default()
    };
    for row in rows {
        match &row.verdict {
            Verdict::Pass => counts.passed += 1,
            Verdict::Fail(_) => counts.failed += 1,
            Verdict::Vacuous(_) => counts.vacuous += 1,
            Verdict::Skip(_) => counts.skipped += 1,
        }
        if row.is_graded_evidence() {
            counts.graded += 1;
        }
        if row.graded == Graded::ErrorClass && !row.verdict.is_fail() {
            counts.error_class += 1;
        }
    }
    counts
}

/// Why this run is not evidence, or `None` when it is.
///
/// Two layers, because they fail differently (`REVIEW-BLOCKERS.md` §5
/// blocker 3):
///
/// * the NAMED layer — every id in [`MUST_GRADE`] must be in the corpus, must
///   grade a match-set comparison and must `PASS`. A named shape that turns
///   into a `SKIP` (or a `VACUOUS`, or a `FAIL`) fails the run even when the
///   count below is satisfied;
/// * the COUNT layer — [`MIN_GRADED`] comparisons — is the coarser backstop
///   for a thinned corpus, a thinned fixture or an all-SKIP run, and is the
///   only layer that notices a sixth gradable shape appearing and then
///   disappearing.
fn evidence_shortfall(rows: &[Row]) -> Option<String> {
    let mut shortfalls: Vec<String> = Vec::new();
    for id in MUST_GRADE {
        match rows.iter().find(|row| row.id == id) {
            None => shortfalls.push(format!("{id} is not in this corpus")),
            Some(row) if !row.is_graded_evidence() => shortfalls.push(format!(
                "{id} ({}) must grade a set-equality comparison and PASS, got {} | {}",
                row.raw_query, row.graded, row.verdict
            )),
            Some(_) => {}
        }
    }
    let counts = summarize(rows);
    if counts.graded < MIN_GRADED {
        shortfalls.push(format!(
            "{} of {} shapes graded a set-equality or Jaccard comparison, below the evidence floor \
             MIN_GRADED = {}: {} VACUOUS, {} SKIP, {} error-class. A thinned corpus (or a fixture \
             that no longer holds the recorded documents) grades less than this runner claims, so \
             it fails instead of printing green (docs/CORPUS-RUNNER.md)",
            counts.graded,
            counts.seen,
            MIN_GRADED,
            counts.vacuous,
            counts.skipped,
            counts.error_class
        ));
    }
    if shortfalls.is_empty() {
        None
    } else {
        Some(shortfalls.join("; "))
    }
}

/// The fixture's whole document set: the match set of the empty query, with
/// no type filter. This is what `recorded ∩ inventory` is taken against.
fn fixture_inventory() -> BTreeSet<HitKey> {
    let hits = fixture_search("", "", 0, 1)
        .expect("the fixture answers an empty query with an empty index path");
    key_set(&hits)
}

/// The fixture backend's answer for one shape: render the query with the
/// double's selector ([`GRAMMAR`] — see `src/grammar.rs`, which production
/// does not use) and ask the fixture index for the match set from the page's
/// offset. This is the ONLY place the renderer is consulted, and it is a
/// test target.
fn fixture_search(
    q: &str,
    type_: &str,
    limit: u64,
    page: u64,
) -> Result<Vec<SearchHit>, SearchError> {
    let parsed = GRAMMAR
        .render(q)
        .map_err(|error| SearchError::InvalidQuery {
            message: error.message().to_string(),
            xapian_error_type: Some(error.xapian_error_type().to_string()),
        })?;
    let index = FixtureIndex::open(FIXTURE_PATH)?;
    index.search(&parsed.with_type(type_), limit, page)
}

fn grade(shape: &Shape, inventory: &BTreeSet<HitKey>) -> Row {
    let recorded = match shape.recorded_hits {
        Some(count) => count.to_string(),
        None => format!("error {}", shape.recorded_status),
    };
    let row = |ours: String, graded: Graded, verdict: Verdict| Row {
        id: shape.id.clone(),
        raw_query: shape.raw_query.clone(),
        type_: shape.type_.clone(),
        recorded: recorded.clone(),
        ours,
        graded,
        verdict,
    };

    // The answer under test. An error shape is graded through the PRODUCTION
    // entry point — the engine parses before it opens an index, so the class
    // is decided with no index at all. A 200 shape is put to the fixture
    // double instead: a real match set needs a real index, which this target
    // does not have. `limit = 0` is the unbounded fetch (`Index::search`).
    let searched = if shape.is_error() {
        gn_search::search(FIXTURE_PATH, &shape.raw_query, &shape.type_, 0, 1)
    } else {
        fixture_search(&shape.raw_query, &shape.type_, 0, 1)
    };

    if shape.is_error() {
        // The error class is index-independent: it is decided by the
        // grammar, before any backend is touched.
        return match &searched {
            Err(SearchError::InvalidQuery {
                message,
                xapian_error_type,
            }) => {
                let expected = shape.expected_error_type();
                let ours = format!(
                    "InvalidQuery({})",
                    xapian_error_type.as_deref().unwrap_or("no class")
                );
                let verdict = if xapian_error_type.as_deref() == Some(expected) {
                    // The message text is not part of the graded class, but
                    // printing it is what makes a wrong message visible.
                    if message.trim().is_empty() {
                        Verdict::Fail("the 422 would carry an empty message".to_string())
                    } else {
                        Verdict::Pass
                    }
                } else {
                    Verdict::Fail(format!(
                        "recorded {} ({}), crate answers {ours}",
                        shape.recorded_status, expected
                    ))
                };
                row(ours, Graded::ErrorClass, verdict)
            }
            Err(other) => row(
                format!("{other:?}"),
                Graded::ErrorClass,
                Verdict::Fail(format!(
                    "recorded {}, crate answers a non-InvalidQuery error ({other})",
                    shape.recorded_status
                )),
            ),
            Ok(hits) => row(
                format!("{} hits", hits.len()),
                Graded::ErrorClass,
                Verdict::Fail(format!(
                    "recorded {}, crate answers 200 with {} hits",
                    shape.recorded_status,
                    hits.len()
                )),
            ),
        };
    }

    // A 200 shape: the crate must not reject it (a 422 where production
    // answered is a real divergence, index or no index).
    let parsed = match GRAMMAR.render(&shape.raw_query) {
        Ok(parsed) => parsed,
        Err(error) => {
            return row(
                format!("InvalidQuery({})", error.xapian_error_type()),
                Graded::Nothing,
                Verdict::Fail(format!(
                    "recorded 200, the grammar rejects the query: {:?} ({})",
                    error,
                    error.message()
                )),
            )
        }
    };
    let hits = match &searched {
        Ok(hits) => hits,
        Err(error) => {
            return row(
                format!("{error:?}"),
                Graded::Nothing,
                Verdict::Fail(format!("recorded 200, the crate errors: {error}")),
            )
        }
    };

    if let Some(reason) = fixture_blocker(shape, &parsed) {
        return row(
            format!("{} hits", hits.len()),
            Graded::Nothing,
            Verdict::Skip(reason),
        );
    }

    // Gradeable: every term is one the fixture resolves the same way the
    // index does, so the comparison is restricted to the documents both the
    // recording and the fixture can speak about.
    let expected: BTreeSet<HitKey> = shape.identifiers.intersection(inventory).cloned().collect();
    let actual = key_set(hits);
    let missing: Vec<&HitKey> = expected.difference(&actual).collect();
    let extra: Vec<&HitKey> = actual.difference(&expected).collect();
    // A non-empty `extra` is a real divergence whatever the expected set
    // holds, so the empty-expected case is only VACUOUS when our side is
    // empty too.
    let mut verdict = if !missing.is_empty() || !extra.is_empty() {
        Verdict::Fail(format!(
            "set equality: {} missing, {} extra (expected {:?}, got {:?})",
            missing.len(),
            extra.len(),
            expected.iter().collect::<Vec<_>>(),
            actual.iter().collect::<Vec<_>>()
        ))
    } else if expected.is_empty() {
        // Nothing to be right or wrong about: the fixture holds none of the
        // recorded documents. Production may have answered thousands here.
        Verdict::Vacuous(format!(
            "expected set empty: none of the {} recorded hits is one of the fixture's {} \
             documents, so equality proves nothing",
            shape.recorded_hits.unwrap_or(0),
            inventory.len()
        ))
    } else {
        Verdict::Pass
    };
    let vacuous = matches!(verdict, Verdict::Vacuous(_));
    // Clause (b): the first ten of the recorded order against our first ten.
    // It needs ten hits from the real index; with a five-document fixture it
    // does not apply, and saying so is better than passing vacuously.
    let mut graded = if vacuous {
        Graded::Vacuous
    } else {
        Graded::SetEquality
    };
    if !vacuous {
        if let Some(recorded_hits) = shape.recorded_hits {
            let our_top: BTreeSet<HitKey> = top_ten(hits).into_iter().collect();
            let recorded_top: BTreeSet<HitKey> = shape.top10.iter().cloned().collect();
            // The two sides are NOT restricted the same way, and this path is
            // dead while the fixture holds five documents (`>= 10` on both
            // sides). When it fires it compares the UNRESTRICTED recorded
            // top ten against our top ten, while clause (a) above compares
            // inventory-restricted sets: on a PARTIALLY widened fixture (the
            // documented next step) the recorded top ten can hold documents
            // the fixture does not have, so a correct renderer scores a
            // Jaccard below the floor and this line becomes a spurious FAIL.
            // Whoever widens the fixture must restrict `recorded_top` to the
            // inventory here too (`REVIEW-BLOCKERS.md` §5, non-blocking (c)).
            if recorded_top.len() >= 10 && our_top.len() >= 10 {
                let j = jaccard(&recorded_top, &our_top);
                graded = Graded::SetEqualityTop10;
                if clause_b_pass(recorded_hits, j) == Some(false) {
                    verdict = Verdict::Fail(format!(
                        "top-10 Jaccard {j:.3} is below the corpus floor {JACCARD_FLOOR}"
                    ));
                }
            }
        }
    }
    row(format!("{} hits", hits.len()), graded, verdict)
}

// ---------------------------------------------------------------------------
// The report
// ---------------------------------------------------------------------------

/// Replay the whole corpus and print one line per shape.
///
/// The lines are the deliverable (`docs/CORPUS-RUNNER.md` documents them):
/// `id | query | type | recorded | ours | graded | verdict`. The test fails
/// if any shape FAILs. In the default build it also fails if fewer than
/// [`MIN_GRADED`] shapes graded a set-equality or Jaccard comparison — a
/// thinned corpus or an all-SKIP run is not evidence, however green it looks.
/// Under `--features fixture` (the only build this file compiles in) the
/// fixture is the inventory, so the floor always applies.
#[test]
fn corpus_report() {
    let corpus = corpus_dir();
    let shapes = load_shapes(&corpus);
    assert!(
        !shapes.is_empty(),
        "no Q*.json fixtures in {}",
        corpus.display()
    );
    let inventory = fixture_inventory();
    assert!(
        !inventory.is_empty(),
        "the fixture index answered an empty query with nothing; the runner cannot grade anything"
    );

    let mut rows = Vec::new();
    for shape in &shapes {
        rows.push(grade(shape, &inventory));
    }

    let backend = format!(
        "fixture index: {} documents, fixture-backed, not parity",
        inventory.len()
    );
    println!(
        "\nparity corpus: {} shapes from {} ({backend})\n",
        shapes.len(),
        corpus.display()
    );
    for row in &rows {
        println!(
            "{} | {} | {} | {} | {} | {} | {}",
            row.id, row.raw_query, row.type_, row.recorded, row.ours, row.graded, row.verdict
        );
    }

    let counts = summarize(&rows);
    println!(
        "\n{} shapes seen: {} graded a set-equality/Jaccard comparison, {} graded an error \
         class, {} VACUOUS (expected set empty: no evidence), {} SKIP ({SKIP_ATTRIBUTION}, see \
         docs/CORPUS-RUNNER.md); {} PASS, {} FAIL",
        counts.seen,
        counts.graded,
        counts.error_class,
        counts.vacuous,
        counts.skipped,
        counts.passed,
        counts.failed
    );
    println!(
        "exit rule: any FAIL fails the test, and so does fewer than MIN_GRADED = {MIN_GRADED} \
         graded comparisons; PASS, VACUOUS and SKIP alone are not evidence\n"
    );

    let failed: Vec<&Row> = rows.iter().filter(|row| row.verdict.is_fail()).collect();
    assert!(
        failed.is_empty(),
        "{} shape(s) diverged: {:?}",
        failed.len(),
        failed
            .iter()
            .map(|row| format!("{}: {}", row.id, row.verdict))
            .collect::<Vec<_>>()
    );
    if let Some(shortfall) = evidence_shortfall(&rows) {
        panic!("{shortfall}");
    }
}

// ---------------------------------------------------------------------------
// The metric itself, checked without a corpus
// ---------------------------------------------------------------------------

#[cfg(test)]
mod metric_tests {
    use super::*;

    fn key(dataset: &str, name: &str) -> HitKey {
        (Some(dataset.to_string()), Some(name.to_string()))
    }

    fn set(keys: &[HitKey]) -> BTreeSet<HitKey> {
        keys.iter().cloned().collect()
    }

    #[test]
    fn jaccard_is_the_corpus_definition() {
        let a = set(&[key("D", "1"), key("D", "2")]);
        let b = set(&[key("D", "2"), key("D", "3")]);
        assert!((jaccard(&a, &a) - 1.0).abs() < f64::EPSILON);
        assert!((jaccard(&a, &b) - 1.0 / 3.0).abs() < 1e-12);
        assert!((jaccard(&a, &set(&[]))).abs() < f64::EPSILON);
        assert_eq!(jaccard(&set(&[]), &set(&[])), 1.0, "empty union is 1.0");
    }

    #[test]
    fn clause_b_needs_ten_recorded_hits() {
        assert_eq!(clause_b_pass(9, 0.1), None);
        assert_eq!(clause_b_pass(10, 0.8), Some(true));
        assert_eq!(clause_b_pass(1197, 0.79), Some(false));
    }

    /// The restriction that makes a fixture comparison honest: only
    /// documents the fixture actually holds are compared.
    #[test]
    fn the_comparison_is_restricted_to_the_inventory() {
        let inventory = set(&[key("D", "1"), key("D", "2")]);
        let recorded = set(&[key("D", "1"), key("Other", "9")]);
        let expected: BTreeSet<HitKey> = recorded.intersection(&inventory).cloned().collect();
        assert_eq!(expected, set(&[key("D", "1")]));
    }

    /// A hit with an absent `dataset` or `name` is its own key, not an empty
    /// string: the corpus keeps both sides optional for exactly this reason.
    #[test]
    fn an_absent_field_is_not_an_empty_string() {
        let hit = SearchHit {
            name: None,
            dataset: Some("D".to_string()),
            dataset_fullname: None,
            group: None,
            species: None,
            authors: None,
            year: None,
            description: None,
            mean: None,
            additive: None,
            lrs: None,
            lod: None,
            locus: None,
            abbreviation: None,
            geno_chr: None,
            geno_mb: None,
            inbredsetcode: None,
            pubmed_id: None,
            extra: std::collections::BTreeMap::new(),
        };
        assert_eq!(hit_key(&hit), (Some("D".to_string()), None));
        assert_ne!(hit_key(&hit), (Some("D".to_string()), Some(String::new())));
    }

    // -----------------------------------------------------------------------
    // The evidence floor (REVIEW-FIXES §d blocker 2)
    // -----------------------------------------------------------------------

    fn reported(id: &str, graded: Graded, verdict: Verdict) -> Row {
        Row {
            id: id.to_string(),
            raw_query: String::new(),
            type_: "phenotype".to_string(),
            recorded: String::new(),
            ours: String::new(),
            graded,
            verdict,
        }
    }

    /// A 200 shape recording `identifiers`, for the two tests that grade one
    /// against the fixture's inventory (hence the gate).
    fn recorded_shape(
        id: &str,
        raw_query: &str,
        recorded_hits: u64,
        identifiers: BTreeSet<HitKey>,
    ) -> Shape {
        Shape {
            id: id.to_string(),
            raw_query: raw_query.to_string(),
            type_: "phenotype".to_string(),
            recorded_status: 200,
            recorded_hits: Some(recorded_hits),
            identifiers,
            top10: Vec::new(),
        }
    }

    /// The floor is what stops a thinned run from being green: at
    /// `MIN_GRADED` comparisons the run is evidence, one comparison fewer is
    /// not, and an error class never substitutes for a match-set comparison.
    ///
    /// Before the floor existed this assertion had no counterpart at all —
    /// `corpus_report` asserted only that no shape FAILed, so a 53-file
    /// corpus, an all-SKIP run or a fixture that had lost its documents all
    /// exited 0.
    #[test]
    fn a_thinned_run_falls_below_the_evidence_floor() {
        let mut rows: Vec<Row> = MUST_GRADE
            .iter()
            .map(|id| reported(id, Graded::SetEquality, Verdict::Pass))
            .collect();
        // Error-class evidence is real but distinct, and cannot top the floor
        // up: the floor is on match-set comparisons.
        rows.push(reported("E1", Graded::ErrorClass, Verdict::Pass));
        let counts = summarize(&rows);
        assert_eq!(counts.graded, MIN_GRADED, "{counts:?}");
        assert_eq!(counts.error_class, 1, "{counts:?}");
        assert_eq!(evidence_shortfall(&rows), None, "{rows:?}");

        // One graded shape thinned away from the corpus.
        rows.truncate(MIN_GRADED - 1);
        let counts = summarize(&rows);
        assert_eq!(counts.graded, MIN_GRADED - 1);
        assert!(
            evidence_shortfall(&rows).is_some(),
            "a thinned corpus must fail: {counts:?}"
        );

        // And the shape count proves nothing on its own: 54 shapes, nothing
        // graded, no evidence.
        let all_skip: Vec<Row> = (0..54)
            .map(|index| {
                reported(
                    &format!("Q{:02}", index + 1),
                    Graded::Nothing,
                    Verdict::Skip("the double cannot answer it; no index".to_string()),
                )
            })
            .collect();
        let counts = summarize(&all_skip);
        assert_eq!(counts.seen, 54);
        assert_eq!(counts.graded, 0);
        assert!(
            evidence_shortfall(&all_skip).is_some(),
            "an all-SKIP run must fail: {counts:?}"
        );
    }

    /// A count is not a set: the five shapes the floor counts must be the five
    /// shapes, each with a match-set comparison that PASSes. Otherwise the
    /// documented growth path (a sixth gradable shape) lets one of them slip
    /// to `SKIP` without the count moving — the run stays green while it lost
    /// a fifth of its real evidence (`REVIEW-BLOCKERS.md` §5 blocker 3).
    #[test]
    fn one_must_grade_shape_turned_into_a_skip_fails_the_run() {
        let rows: Vec<Row> = MUST_GRADE
            .iter()
            .map(|id| reported(id, Graded::SetEquality, Verdict::Pass))
            .collect();
        assert_eq!(evidence_shortfall(&rows), None, "{rows:?}");

        // Q36 becomes a SKIP and a sixth shape grades in its place, so the
        // count alone is satisfied and only the named layer can fail.
        let mut rows: Vec<Row> = rows.into_iter().filter(|row| row.id != "Q36").collect();
        rows.push(reported(
            "Q36",
            Graded::Nothing,
            Verdict::Skip("free text is stemmed and scored by the real index".to_string()),
        ));
        rows.push(reported("Q99", Graded::SetEquality, Verdict::Pass));
        let counts = summarize(&rows);
        assert_eq!(counts.graded, MIN_GRADED, "the count alone is satisfied");
        let shortfall = evidence_shortfall(&rows).expect("a lost must_grade shape must fail");
        assert!(shortfall.contains("Q36"), "{shortfall}");

        // A named shape that is missing from the corpus is the same failure;
        // so is one that is present but VACUOUS (it compared nothing).
        let without = rows
            .iter()
            .map(|row| reported(&row.id, row.graded, Verdict::Pass))
            .filter(|row| row.id != "Q31")
            .collect::<Vec<Row>>();
        assert!(evidence_shortfall(&without)
            .expect("a missing named shape must fail")
            .contains("Q31"));
        let vacuous = rows
            .iter()
            .map(|row| {
                if row.id == "Q29" {
                    reported(
                        "Q29",
                        Graded::Vacuous,
                        Verdict::Vacuous("empty".to_string()),
                    )
                } else {
                    reported(&row.id, row.graded, Verdict::Pass)
                }
            })
            .collect::<Vec<Row>>();
        assert!(evidence_shortfall(&vacuous)
            .expect("a VACUOUS named shape must fail")
            .contains("Q29"));
    }

    /// A shape whose expected set is empty after the inventory restriction is
    /// `VACUOUS`, never `PASS`, and never counts as graded evidence: the
    /// difference between "we found nothing" and "we found the right
    /// nothing" is the whole point (`REVIEW-FIXES.md` §b(ii)).
    ///
    /// Grades through the fixture's inventory, which the file's
    /// `--features fixture` gate compiles in.
    #[test]
    fn an_empty_expected_set_is_vacuous_not_graded() {
        let inventory = fixture_inventory();
        // `group:B6` recorded three documents, none of them in the fixture —
        // the recorded shape Q16's situation.
        let shape = recorded_shape(
            "Q16",
            "group:B6",
            3,
            set(&[key("SomeOtherDataset", "1"), key("SomeOtherDataset", "2")]),
        );
        let row = grade(&shape, &inventory);
        assert!(
            matches!(row.verdict, Verdict::Vacuous(_)),
            "an empty expected set is VACUOUS, got {:?}",
            row.verdict
        );
        assert_eq!(row.graded, Graded::Vacuous);
        assert!(!row.is_graded_evidence());
        let counts = summarize(&[row]);
        assert_eq!(counts.graded, 0, "VACUOUS is not graded: {counts:?}");
        assert_eq!(counts.vacuous, 1, "{counts:?}");
        assert_eq!(counts.passed, 0, "VACUOUS is not a PASS: {counts:?}");
    }

    /// A quoted phrase, a `NEAR`/`ADJ` operator and a bare field value
    /// (`year:2014`, which GN4 builds as the phrase `year 2014`,
    /// `native.rs:721-724`) are skipped by name, not graded: the rendered terms
    /// mark the words as adjacent and the fixture double can only AND, while
    /// GN4's positionless index answers a phrase with 0 hits
    /// (`genenetwork4/docs/search-engines.md:98-102`; DECISIONS rows 35, 36).
    ///
    /// Needs the fixture's inventory, which the file's `--features fixture`
    /// gate compiles in.
    #[test]
    fn a_phrase_shape_is_skipped_as_unrepresentable() {
        let inventory = fixture_inventory();
        for (id, query) in [
            ("Q04", "\"alcohol dependence\""),
            ("Q11", "alcohol NEAR dependence"),
            ("Q12", "alcohol ADJ dependence"),
            ("Q55", "year:2014"),
            ("Q56", "mean:5"),
        ] {
            let shape = recorded_shape(id, query, 0, BTreeSet::new());
            let row = grade(&shape, &inventory);
            match &row.verdict {
                Verdict::Skip(reason) => assert!(
                    reason.starts_with("phrase-not-expressible"),
                    "{id} must say why it is skipped, got {reason:?}"
                ),
                other => panic!("{id} must be a SKIP, never a {other:?}"),
            }
            assert_eq!(row.graded, Graded::Nothing);
        }
    }
}
