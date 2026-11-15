//! The renderer: query string in, index terms out.
//!
//! SCOUT section implemented: SCOUT-NATIVE §3 (the grammar), reached through
//! the public [`gn_search::grammar`] API. The grammar's own tables are
//! tested per prefix and per slot in `src/grammar.rs`; this file tests what
//! a query actually becomes, which is what an index backend consumes.

use gn_search::grammar::{RenderError, RenderedTerm, SyntaxFault, TermKind, GRAMMAR};
use gn_search::SearchError;

fn terms(q: &str) -> Vec<RenderedTerm> {
    GRAMMAR.render(q).expect("the query renders").terms
}

fn one(q: &str) -> RenderedTerm {
    let mut rendered = terms(q);
    assert_eq!(rendered.len(), 1, "{q:?} must render exactly one term");
    rendered.remove(0)
}

/// Every boolean prefix, one case each: the index term is the prefix plus
/// the LOWERCASED value, because the index stores only lowercase boolean
/// terms (SCOUT-NATIVE §3).
#[test]
fn every_boolean_prefix_renders_a_lowercase_exact_term() {
    for (field, prefix, value) in [
        ("author", "A", "Williams"),
        ("species", "XS", "Mouse"),
        ("group", "XG", "BXD"),
        ("tissue", "XI", "Liver"),
        ("dataset", "XDS", "BXDPublish"),
        ("symbol", "XY", "Bdnf"),
        ("chr", "XC", "16"),
        ("peakchr", "XPC", "1"),
    ] {
        let term = one(&format!("{field}:{value}"));
        assert_eq!(term.prefix, prefix, "prefix for {field}");
        assert_eq!(
            term.term,
            format!("{prefix}{}", value.to_lowercase()),
            "term for {field}"
        );
        assert_eq!(term.kind, TermKind::Boolean, "kind for {field}");
    }
}

/// Two spellings of the same boolean value render to the same term — this
/// is the behaviour that makes `dataset:BXDPublish` match an index full of
/// lowercase terms (SCOUT-NATIVE §3).
#[test]
fn boolean_value_case_does_not_change_the_term() {
    assert_eq!(
        one("dataset:BXDPublish").term,
        one("dataset:bxdpublish").term
    );
}

/// Every free-text prefix, aliases included (gn3 registers `RIF` and `WIKI`
/// as well as `rif` and `wiki`; SCOUT-NATIVE §3).
#[test]
fn every_probabilistic_prefix_renders_a_field_term() {
    for (field, prefix, value) in [
        ("description", "XD", "Alcohol"),
        ("rif", "XRF", "Liver"),
        ("RIF", "XRF", "Liver"),
        ("wiki", "XWK", "Strain"),
        ("WIKI", "XWK", "Strain"),
    ] {
        let term = one(&format!("{field}:{value}"));
        assert_eq!(term.prefix, prefix, "prefix for {field}");
        assert_eq!(
            term.term,
            format!("{prefix}{}", value.to_lowercase()),
            "term for {field}"
        );
        assert_eq!(term.kind, TermKind::Probabilistic, "kind for {field}");
    }
}

/// Every numeric range field, one case each: marker, raw value, slot, and
/// bounds — including the open sides the grammar allows (SCOUT-NATIVE §3).
#[test]
fn every_numeric_slot_renders_a_value_range() {
    for (field, marker, slot, value, begin, end) in [
        ("mean", "mean:", 0u32, "1..5", Some(1.0), Some(5.0)),
        ("peak", "peak:", 1u32, "5..", Some(5.0), None),
        ("peakmb", "peakmb:", 3u32, "..20", None, Some(20.0)),
        (
            "additive",
            "additive:",
            4u32,
            "-2..2",
            Some(-2.0),
            Some(2.0),
        ),
        (
            "year",
            "year:",
            5u32,
            "2010..2020",
            Some(2010.0),
            Some(2020.0),
        ),
    ] {
        let term = one(&format!("{field}:{value}"));
        assert_eq!(term.prefix, marker, "marker for {field}");
        assert_eq!(term.term, value, "raw value for {field}");
        assert_eq!(
            term.kind,
            TermKind::ValueRange { slot, begin, end },
            "slot {slot} for {field}"
        );
    }
}

/// A numeric field value WITHOUT `..` is free text, not a point range — and
/// the colon is a WORD BREAK, so `year:2014` is the two words `year` and
/// `2014`, which is what xapian builds (`native.rs:722-725`: "`mean:5` is a
/// `mean 5` phrase"; `native.rs:389-405` inspects only values containing
/// `..`). `REVIEW-FIXES.md` §d blocker 3; DECISIONS row 34.
#[test]
fn a_bare_numeric_value_renders_as_its_words() {
    for (q, words) in [
        ("year:2014", ["year", "2014"]),
        ("mean:5", ["mean", "5"]),
        ("peak:1.5", ["peak", "1.5"]),
        ("additive:-2", ["additive", "-2"]),
    ] {
        let rendered = terms(q);
        assert_eq!(
            rendered.len(),
            2,
            "{q:?} is two words, not a range: {rendered:?}"
        );
        for (term, word) in rendered.iter().zip(words) {
            assert_eq!(term.kind, TermKind::FreeText, "{q:?} is text, not a range");
            assert_eq!(term.term, word, "{q:?} word");
            assert_eq!(term.prefix, "", "a text term carries no field label");
        }
        assert!(
            GRAMMAR.render(q).is_ok(),
            "{q:?} must render, not be rejected"
        );
    }
}

/// A non-numeric value WITHOUT `..` is not a range at all, so it is not an
/// error either: `mean:abc` renders as its two words and a 200 is reachable
/// (`native.rs:389-405` inspects only values containing `..`; the recorded
/// 422, Q52, is `mean:abc..def`).
#[test]
fn a_non_numeric_value_without_a_range_is_not_a_422() {
    for q in ["mean:abc", "year:twenty", "peak:abc"] {
        let rendered = terms(q);
        assert_eq!(
            rendered.len(),
            2,
            "{q:?} is the label and the value: {rendered:?}"
        );
        assert!(
            rendered.iter().all(|term| term.kind == TermKind::FreeText),
            "{q:?} is text: {rendered:?}"
        );
        assert!(
            GRAMMAR.render(q).is_ok(),
            "{q:?} must render, not be rejected"
        );
    }
}

/// A field token whose value the punctuation grammar does not consume is a
/// PHRASE at xapian, not an AND of its words: GN4 measures `mean:5` as a
/// `mean 5` phrase (`native.rs:721-724`) and its positionless index answers a
/// phrase with **0** hits (Q04/Q11/Q12). The rendered terms therefore carry
/// [`RenderedTerm::phrase`], so a real backend can hand the words to xapian's
/// parser as adjacency instead of ANDing them — which is the only shape that
/// can ever be parity (`REVIEW-BLOCKERS.md` §5 blocker 2; DECISIONS row 36).
#[test]
fn a_bare_field_value_is_a_phrase_not_an_and() {
    for (q, words) in [("year:2014", ["year", "2014"]), ("mean:5", ["mean", "5"])] {
        let rendered = terms(q);
        assert_eq!(rendered.len(), 2, "{q:?} is two phrase words: {rendered:?}");
        for (term, word) in rendered.iter().zip(words) {
            assert_eq!(term.term, word, "{q:?} word");
            assert_eq!(term.kind, TermKind::FreeText, "{q:?} is text");
            assert!(
                term.phrase,
                "{q:?} is a phrase at GN4, not an AND of words: {rendered:?}"
            );
        }
    }

    // One word is not a phrase group: `position:` is the word `position`.
    assert!(!one("position:").phrase);
    // An unregistered name is not a field, so nothing is split (Q50), and a
    // standalone word is not a phrase either.
    assert_eq!(one("foo:bar").term, "foo:bar");
    assert!(!one("foo:bar").phrase);
    assert!(!one("alcohol").phrase);
    // A range is a predicate, not a phrase.
    assert!(!one("mean:1..2").phrase);
}

/// A quoted value is phrase content, so a `..` inside quotes is not range
/// syntax: `mean:"1..2"` is the words `mean` and `1..2`, never a slot-0
/// range and never the Q52 422 — GN4's scan skips quoted content and stops
/// the value at a quote (`native.rs:380-385`, `:398-401`).
/// `REVIEW-FIXES.md` §d blocker 4.
#[test]
fn a_quoted_value_is_never_a_range() {
    let rendered = terms("mean:\"1..2\"");
    assert_eq!(rendered.len(), 2, "two words: {rendered:?}");
    assert_eq!(rendered[0].term, "mean");
    assert_eq!(
        rendered[1].term, "1..2",
        "the phrase content without its quotes"
    );
    for term in &rendered {
        assert_eq!(
            term.kind,
            TermKind::FreeText,
            "phrase content is text, not a VALUE range"
        );
    }
    assert!(
        GRAMMAR.render("mean:\"1..2\"").is_ok(),
        "a quoted range must not be a 422"
    );
    // The unquoted form is still the range it always was.
    assert_eq!(
        one("mean:1..2").kind,
        TermKind::ValueRange {
            slot: 0,
            begin: Some(1.0),
            end: Some(2.0)
        }
    );
}

/// Quote state is a property of the QUERY, not of the token: GN4's range guard
/// walks the whole string with an `in_quotes` state machine
/// (`native.rs:376-385`) and stops a field value at the next quote (`:399`).
/// So a range-looking token inside a quoted phrase is phrase content, and
/// `mean:1.."2"` is the one-sided range `1..` plus the phrase word `2`.
/// Neither is the Q52 422 — the per-token test made both of them one
/// (`REVIEW-BLOCKERS.md` §5 blocker 1).
#[test]
fn a_range_looking_token_inside_a_quote_is_not_a_422() {
    // The token starts INSIDE a quoted region: `mean:abc..def` is phrase
    // content there, not a bound, because the phrase opened before it.
    let rendered = terms("\"alcohol mean:abc..def\"");
    assert_eq!(rendered.len(), 2, "two phrase words: {rendered:?}");
    assert_eq!(rendered[0].term, "alcohol");
    assert_eq!(rendered[1].term, "mean:abc..def");
    for term in &rendered {
        assert_eq!(
            term.kind,
            TermKind::FreeText,
            "phrase content is text, not a predicate: {rendered:?}"
        );
        assert!(term.phrase, "the words of a quoted region are a phrase");
    }
    assert!(
        GRAMMAR.render("\"alcohol mean:abc..def\"").is_ok(),
        "a quoted phrase is a 200 at GN4, never the Q52 422"
    );

    // The token starts outside the quotes and the value ENDS at the quote: the
    // range keeps its one parsed side and the quoted tail is phrase content
    // the range did not consume.
    let rendered = terms("mean:1..\"2\"");
    assert_eq!(
        rendered.len(),
        2,
        "a one-sided range and a phrase word: {rendered:?}"
    );
    assert_eq!(
        rendered[0].kind,
        TermKind::ValueRange {
            slot: 0,
            begin: Some(1.0),
            end: None
        },
        "the value stops at the quote, so the upper side is open"
    );
    assert_eq!(rendered[1].term, "2", "the quoted tail is phrase content");
    assert!(rendered[1].phrase);
    assert!(
        GRAMMAR.render("mean:1..\"2\"").is_ok(),
        "a value that ends at a quote is not a bad bound"
    );

    // An operator inside a quoted region is phrase content as well: GN4's
    // parser reads quoted content as a phrase, never as a boolean expression.
    let rendered = terms("\"alcohol AND dependence\"");
    assert_eq!(rendered.len(), 3, "three phrase words: {rendered:?}");
    assert!(
        rendered
            .iter()
            .all(|term| term.kind == TermKind::FreeText && term.phrase),
        "a quoted `AND` is a word, not an operator: {rendered:?}"
    );

    // The UNQUOTED shape is still the recorded Q52 422.
    assert_eq!(
        GRAMMAR
            .render("mean:abc..def")
            .expect_err("abc is not a number"),
        RenderError::RangeBound {
            field: "mean".to_string(),
            value: "abc..def".to_string(),
        }
    );
}

/// An empty field value is free text, not match-nothing: `position:` never
/// reaches a processor, and the recorded Q53 answer is a 200 with 576 hits —
/// hits on the WORD `position`, which is why the label survives as a term
/// and its colon does not (`native.rs:264-270`; DECISIONS rows 30 and 34).
#[test]
fn an_empty_field_value_is_the_label_word() {
    for (q, word) in [
        ("position:", "position"),
        ("pos:", "pos"),
        ("mean:", "mean"),
        ("author:", "author"),
        ("dataset:", "dataset"),
        ("Hs:", "Hs"),
    ] {
        let term = one(q);
        assert_eq!(term.kind, TermKind::FreeText, "{q:?} is text");
        assert_eq!(term.term, word, "{q:?} keeps the label as the word");
        assert_eq!(term.prefix, "", "{q:?} carries no field label");
    }
    // And it renders rather than being rejected.
    assert!(GRAMMAR.render("position:").is_ok());
}

/// The both-open range matches nothing, which every backend reads the same
/// way (SCOUT-NATIVE §3).
#[test]
fn the_both_open_range_has_no_bounds() {
    let term = one("mean:..");
    assert_eq!(
        term.kind,
        TermKind::ValueRange {
            slot: 0,
            begin: None,
            end: None
        }
    );
}

/// Slot 2: a point location expands by 50 kb on each side, in megabases —
/// the unit the slot holds (SCOUT-NATIVE §3).
#[test]
fn position_point_expands_by_fifty_kilobases() {
    let term = one("position:1M");
    assert_eq!(term.prefix, "position:");
    assert_eq!(
        term.kind,
        TermKind::ValueRange {
            slot: 2,
            begin: Some(0.95),
            end: Some(1.05)
        }
    );
}

/// The `pos` alias is the same rule, and the window never goes below 0
/// (SCOUT-NATIVE §3).
#[test]
fn position_never_expands_below_zero() {
    let term = one("pos:10k");
    assert_eq!(term.prefix, "pos:");
    assert_eq!(
        term.kind,
        TermKind::ValueRange {
            slot: 2,
            begin: Some(0.0),
            end: Some(0.06)
        }
    );
}

/// A position range keeps open sides and converts both to megabases.
#[test]
fn position_range_converts_to_megabases() {
    let term = one("position:100000..200000");
    assert_eq!(
        term.kind,
        TermKind::ValueRange {
            slot: 2,
            begin: Some(0.1),
            end: Some(0.2)
        }
    );
}

/// An unparseable position declines to "match nothing" instead of an error:
/// gn3's escaped `ValueError` is not a contract, and GN4's blessed
/// behaviour for the class is a 200 with no hits (SCOUT-NATIVE §3, §6).
#[test]
fn unparseable_position_matches_nothing() {
    let term = one("position:abc");
    assert_eq!(
        term.kind,
        TermKind::ValueRange {
            slot: 2,
            begin: None,
            end: None
        }
    );
}

/// A numeric bound that is not a number on a `..` range is the recorded Q52
/// diagnostic and reaches a caller as the 422-producing variant, not as a
/// panic or an empty result (SCOUT-NATIVE §6).
#[test]
fn bad_numeric_bound_is_an_invalid_query() {
    let error = GRAMMAR
        .render("mean:abc..def")
        .expect_err("abc is not a number");
    assert_eq!(
        error,
        RenderError::RangeBound {
            field: "mean".to_string(),
            value: "abc..def".to_string(),
        }
    );
    assert_eq!(error.message(), "Unknown range operation");
    assert_eq!(error.xapian_error_type(), "QueryParserError");

    match gn_search::search("", "mean:abc..def", "phenotype", 20, 1) {
        Err(SearchError::InvalidQuery {
            message,
            xapian_error_type,
        }) => {
            assert_eq!(message, "Unknown range operation");
            assert_eq!(xapian_error_type.as_deref(), Some("QueryParserError"));
        }
        other => panic!("expected InvalidQuery, got {other:?}"),
    }
}

/// `Hs:` / `Mm:` are species ∧ chromosome ∧ slot-2 range, with no liftover:
/// `Hs:chr2:1M..1.2M` is `XShuman` + `XC2` + 1.0–1.2 Mb (Q40, 4296 recorded
/// hits; GN4's `LocationProcessor`, `native.rs:310-350`).
#[test]
fn a_human_location_renders_species_chromosome_and_position() {
    let rendered = terms("Hs:chr2:1M..1.2M");
    assert_eq!(rendered.len(), 3, "species, chromosome and position");
    assert_eq!(rendered[0].prefix, "XS");
    assert_eq!(rendered[0].term, "XShuman");
    assert_eq!(rendered[0].kind, TermKind::Boolean);
    assert_eq!(rendered[1].prefix, "XC");
    assert_eq!(rendered[1].term, "XC2");
    assert_eq!(rendered[1].kind, TermKind::Boolean);
    assert_eq!(rendered[2].prefix, "Hs:");
    assert_eq!(
        rendered[2].kind,
        TermKind::ValueRange {
            slot: 2,
            begin: Some(1.0),
            end: Some(1.2),
        }
    );
}

/// The mouse shorthand is the same shape on slot 2, and a point location
/// expands by ±50 kb like `position:` (Q41 `Mm:chr1:5M`, 491 recorded hits).
#[test]
fn a_mouse_location_point_expands_like_a_position() {
    let rendered = terms("Mm:chr1:5M");
    assert_eq!(rendered.len(), 3);
    assert_eq!(rendered[0].term, "XSmouse");
    assert_eq!(rendered[1].term, "XC1");
    assert_eq!(
        rendered[2].kind,
        TermKind::ValueRange {
            slot: 2,
            begin: Some(4.95),
            end: Some(5.05),
        }
    );
    // The value is lowercased before it is split, as gn3 does.
    let rendered = terms("Hs:chrX:1M");
    assert_eq!(rendered[1].term, "XCx");
}

/// An invalid location is the recorded Q42 class: a 422-shaped
/// `InvalidQuery` carrying `InvalidOperationError`, never a 200 and never an
/// mset crash (`native.rs:187-215`, AMENDMENT E).
#[test]
fn an_invalid_location_is_an_invalid_query() {
    for q in ["Hs:foo:1M", "Mm:chr1", "Hs:chr1:abc", "Hs:1:2:3"] {
        let error = GRAMMAR.render(q).expect_err("{q:?} is not a location");
        assert!(
            matches!(error, RenderError::InvalidLocation { .. }),
            "{q:?} must be an InvalidLocation, got {error:?}"
        );
        assert_eq!(
            error.message(),
            "InvalidOperationError: Query is invalid (invalid Hs/Mm location field)"
        );
        assert_eq!(error.xapian_error_type(), "InvalidOperationError");
        match gn_search::search("", q, "phenotype", 20, 1) {
            Err(SearchError::InvalidQuery {
                message,
                xapian_error_type,
            }) => {
                assert_eq!(xapian_error_type.as_deref(), Some("InvalidOperationError"));
                assert!(message.starts_with("InvalidOperationError: Query is invalid"));
            }
            other => panic!("{q:?} must be InvalidQuery, got {other:?}"),
        }
    }
}

/// A dangling operator is a syntax fault — the recorded Q51 shape, which GN4
/// reports as 422 `invalid_query` with xapian's diagnostic (F6).
#[test]
fn a_dangling_operator_is_a_syntax_error() {
    for q in [
        "alcohol AND",
        "AND alcohol",
        "alcohol AND OR dependence",
        // A `NOT` at the end has a term on neither side it could negate.
        "alcohol AND NOT",
        "a AND (NOT",
    ] {
        let error = GRAMMAR.render(q).expect_err("{q:?} is not a query");
        assert_eq!(
            error,
            RenderError::Syntax {
                fault: SyntaxFault::DanglingOperator
            }
        );
        assert_eq!(error.message(), "Syntax: <expression> AND <expression>");
        assert_eq!(error.xapian_error_type(), "QueryParserError");
    }
    match gn_search::search("", "alcohol AND", "phenotype", 20, 1) {
        Err(SearchError::InvalidQuery { message, .. }) => {
            assert_eq!(message, "Syntax: <expression> AND <expression>");
        }
        other => panic!("expected InvalidQuery, got {other:?}"),
    }
    // Operators in their legal places are untouched — including a leading
    // `NOT`, whose xapian behaviour no recorded shape pins.
    assert!(GRAMMAR.render("alcohol AND dependence").is_ok());
    assert!(GRAMMAR.render("alcohol NOT dependence").is_ok());
    assert!(GRAMMAR.render("NOT alcohol").is_ok());
    assert!(GRAMMAR
        .render("(alcohol OR insulin) AND species:mouse")
        .is_ok());
}

/// `NOT` is a PREFIX operator: it needs no term on its left, so it is legal
/// after a binary operator too. `alcohol AND NOT dependence` and
/// `a AND (NOT b)` are the canonical xapian exclusions and must render, not
/// 422 (`REVIEW-FIXES.md` §d blocker 1 — a 422 where production answers is
/// the direction a parity claim cannot afford).
#[test]
fn a_not_after_a_binary_operator_is_not_dangling() {
    for q in [
        "alcohol AND NOT dependence",
        "a AND (NOT b)",
        "alcohol OR NOT dependence",
        "NOT alcohol",
    ] {
        GRAMMAR
            .render(q)
            .unwrap_or_else(|error| panic!("{q:?} must render, got {error:?}"));
        assert!(GRAMMAR.render(q).is_ok(), "{q:?} must render, not be a 422");
    }
    let rendered = terms("alcohol AND NOT dependence");
    assert_eq!(
        rendered.len(),
        4,
        "two words and two operators: {rendered:?}"
    );
    assert_eq!(rendered[1].term, "AND");
    assert_eq!(rendered[2].term, "NOT");
    assert_eq!(rendered[2].kind, TermKind::Operator);
}

/// An unbalanced quote is the renderer's own rejection — and NOT the engine's.
///
/// This is the one renderer rule the moved engine does not share: xapian
/// parses `alcohol "dependence` as a phrase whose close it supplies, so
/// `gn_search::search` accepts it (measured after the move: it reaches the
/// index and fails as `Internal`, not `InvalidQuery`). The engine is
/// authority, so this test pins the DOUBLE's rule and says so, rather than
/// claiming a production class it does not have. No recorded shape carries an
/// unterminated phrase (F6), so nothing graded moves.
#[test]
fn an_unbalanced_quote_is_the_renderers_own_syntax_error() {
    let error = GRAMMAR
        .render("alcohol \"dependence")
        .expect_err("the phrase never closes");
    assert_eq!(
        error,
        RenderError::Syntax {
            fault: SyntaxFault::UnbalancedQuotes
        }
    );
    assert_eq!(error.xapian_error_type(), "QueryParserError");
    // A balanced phrase is not a syntax fault (the recorded Q04 shape).
    assert!(GRAMMAR.render("\"alcohol dependence\"").is_ok());
}

/// Free text, operators and unregistered prefixes render as themselves;
/// structure (grouping, precedence) is the backend's (SCOUT-NATIVE §3).
#[test]
fn free_text_operators_and_unknown_prefixes_pass_through() {
    let rendered = terms("alcohol AND author:Williams");
    assert_eq!(rendered.len(), 3);
    assert_eq!(rendered[0].kind, TermKind::FreeText);
    assert_eq!(rendered[0].term, "alcohol");
    assert_eq!(rendered[0].prefix, "");
    assert_eq!(rendered[1].kind, TermKind::Operator);
    assert_eq!(rendered[1].term, "AND");
    assert_eq!(rendered[2].kind, TermKind::Boolean);
    assert_eq!(rendered[2].term, "Awilliams");

    // Lowercase operators are plain terms, not operators.
    let rendered = terms("alcohol and");
    assert_eq!(rendered[1].kind, TermKind::FreeText);
    assert_eq!(rendered[1].term, "and");

    // An unregistered prefix is text (recorded shape Q50: `foo:bar`).
    let rendered = one("foo:bar");
    assert_eq!(rendered.kind, TermKind::FreeText);
    assert_eq!(rendered.term, "foo:bar");
}

/// Parentheses around a field term do not stop it being one.
#[test]
fn parentheses_are_trimmed_before_classifying() {
    let rendered = one("(dataset:bxd)");
    assert_eq!(rendered.kind, TermKind::Boolean);
    assert_eq!(rendered.term, "XDSbxd");
}

/// The document type travels with the query, as the `XT{type}` term an
/// index filters on (SCOUT-NATIVE §3, §4).
#[test]
fn type_filter_travels_with_the_query() {
    let parsed = GRAMMAR.render("alcohol").expect("renders");
    assert_eq!(parsed.raw, "alcohol");
    assert_eq!(parsed.type_filter, None);
    assert_eq!(parsed.type_term(), None);

    let typed = parsed.clone().with_type("phenotype");
    assert_eq!(typed.type_filter.as_deref(), Some("phenotype"));
    assert_eq!(typed.type_term().as_deref(), Some("XTphenotype"));

    assert_eq!(parsed.with_type("").type_term(), None);
}

/// An empty query renders no terms — the route, not the engine, rejects it
/// with 400 (SCOUT-NATIVE §6).
#[test]
fn empty_query_renders_no_terms() {
    let parsed = GRAMMAR.render("   ").expect("renders");
    assert!(parsed.terms.is_empty());
    assert_eq!(parsed.raw, "   ");
}
