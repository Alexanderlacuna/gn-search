//! The GN4 query grammar as DATA, plus a pure renderer.
//!
//! SCOUT section implemented: SCOUT-NATIVE §3 (the query builder GN4
//! configures, mirrored from gn3's `gn3/gn3/api/search.py:213-273`), with
//! the slot numbers of SCOUT-INDEXER §2 (0 mean, 1 peak/LRS, 2 position-Mb,
//! 3 peak-Mb, 4 additive, 5 year, 6 nsamples).
//!
//! There is deliberately no xapian dependency here. The grammar is a
//! table, and [`Grammar::render`] turns a query string into the terms an
//! index backend consumes: (`prefix`, `term`, `kind`) triples. Building
//! the parser objects (stemmer, field processors, range processors) is the
//! backend's job, because it needs the binding — GN4 does it in
//! `src/search/native.rs::build_query_parser` against the `xapian_rs` fork
//! (SCOUT-NATIVE §8). Keeping this file free of that binding is what makes
//! the grammar testable on a machine with no xapian at all.
//!
//! What the renderer does and does not do:
//!
//! * it classifies each whitespace-separated token: a registered field
//!   prefix (with a non-empty value) renders as that field's term; a
//!   boolean operator renders as an operator; everything else is free text;
//! * it applies the value transformations the index depends on: boolean
//!   values are lowercased (the index stores only lowercase boolean terms,
//!   SCOUT-NATIVE §3), and numeric/position values are parsed into slot
//!   bounds;
//! * a numeric field value is a range ONLY when it contains `..` and is not
//!   quoted: xapian consults a range processor only for unquoted range
//!   syntax, so `mean:5` is a `mean 5` phrase and `mean:"1..2"` is phrase
//!   content, never a range (`native.rs:389-405`, `native.rs:722-725`). The
//!   value ENDS at the first quote, as the parser's does (`native.rs:398-401`),
//!   so `mean:1.."2"` is the one-sided range `1..` plus the phrase word `2`;
//! * quote state is a property of the QUERY, not of the token, because that
//!   is how GN4 decides it (`native.rs:376-385`): inside a quoted region
//!   nothing is a field prefix or an operator, so `"alcohol mean:abc..def"`
//!   is a phrase and not the Q52 422;
//! * a registered field whose value the grammar does not consume is TEXT,
//!   and that colon is a word break: `mean:5` is the phrase `mean 5` and
//!   `position:` is the word `position` (Q53's 576 recorded hits;
//!   `native.rs:722-724`), so the label and the value become separate
//!   free-text terms (DECISIONS row 34) that carry [`RenderedTerm::phrase`]
//!   (DECISIONS row 36);
//! * it rejects the two classes GN4 maps to 422 `invalid_query` without a
//!   backend: a numeric bound that is not a number (Q52), an invalid
//!   `Hs:`/`Mm:` location (Q42), and a syntactically malformed query — a
//!   dangling operator (Q51) or an unbalanced quote ([`RenderError`]);
//! * it does NOT model grouping or precedence, and it expresses a phrase only
//!   as `phrase`-marked words: which words are adjacent is the backend's own
//!   parser's job. [`ParsedQuery::raw`] keeps the query verbatim for the
//!   structure the backend's own parser rebuilds.

/// How free text is stemmed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StemStrategy {
    /// Stem every term and tag the stem with [`Language::stem_prefix`] —
    /// xapian's `STEM_ALL_Z`, so free text matches stemmed and
    /// case-insensitively (SCOUT-NATIVE §3).
    AllZ,
}

/// One language the parser answers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Language {
    /// ISO code the parser is asked for (`en`).
    pub code: &'static str,
    /// The stemming strategy for free-text terms.
    pub stem_strategy: StemStrategy,
    /// The prefix a stemmed term carries under [`StemStrategy::AllZ`].
    pub stem_prefix: &'static str,
}

/// An exact-term field prefix (gn3's "boolean prefix").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BooleanPrefix {
    /// The field name as written in a query (`dataset`).
    pub field: &'static str,
    /// The index term prefix (`XDS`).
    pub prefix: &'static str,
    /// Whether the value is lowercased before the term is formed.
    ///
    /// True for every boolean prefix GN4 configures: the index stores
    /// lowercase boolean terms only, so `dataset:BXDPublish` and
    /// `dataset:bxdpublish` are the same query (SCOUT-NATIVE §3 — gn3
    /// itself lowercases only `group`, and GN4 does it for all of them;
    /// the difference is parity-neutral because every graded shape already
    /// uses lowercase values).
    pub lowercase_value: bool,
}

/// A free-text (BM25-scored) field prefix (gn3's "probabilistic prefix").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProbabilisticPrefix {
    /// The lowercase field name (`rif`).
    pub field: &'static str,
    /// The index term prefix (`XRF`).
    pub prefix: &'static str,
    /// Case variants that name the same field (`RIF`, `WIKI` —
    /// SCOUT-NATIVE §3: gn3 registers both forms).
    pub aliases: &'static [&'static str],
}

/// A numeric range field and the value slot it reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NumericRange {
    /// The field name as written in a query, without the colon (`mean`).
    pub field: &'static str,
    /// The marker as the parser's range processor registers it, colon
    /// included (`mean:`).
    pub marker: &'static str,
    /// The value slot the range compares against (SCOUT-INDEXER §2).
    pub slot: u32,
}

/// A species-location shorthand (`Hs:chr2:1M..1.2M`): one field name that
/// carries species, chromosome and position at once (GN4's
/// `LocationProcessor`, `native.rs:310-350`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocationShorthand {
    /// The field name as a query writes it (`Hs`).
    pub field: &'static str,
    /// The species boolean term the index stores for it (`XShuman`) —
    /// GN4's `species_term` (`native.rs:151-170`).
    pub species_term: &'static str,
}

/// The `position` / `pos` rule: base-pair coordinates on one value slot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PositionRule {
    /// The field names that select this rule (`position`, `pos`).
    pub fields: &'static [&'static str],
    /// The value slot holding positions, in megabases.
    pub slot: u32,
    /// A point location expands by this many bases on each side.
    pub half_window_bases: f64,
    /// Bases per megabase: the divisor between a query's coordinates and
    /// the value the slot holds.
    pub bases_per_megabase: f64,
}

/// The whole grammar, as data.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grammar {
    /// The languages the parser answers to; `en` is the one GN4 uses.
    pub languages: &'static [Language],
    /// Exact-term prefixes.
    pub boolean_prefixes: &'static [BooleanPrefix],
    /// Free-text prefixes.
    pub probabilistic_prefixes: &'static [ProbabilisticPrefix],
    /// Numeric range fields and their slots.
    pub numeric_ranges: &'static [NumericRange],
    /// Species-location shorthands (`Hs`, `Mm`).
    pub location_shorthands: &'static [LocationShorthand],
    /// The `position`/`pos` rule.
    pub position: PositionRule,
    /// Boolean operators. Uppercase only: lowercase forms are plain terms
    /// (SCOUT-NATIVE §3).
    pub operators: &'static [&'static str],
}

/// The grammar GN4 speaks: gn3's parser configuration, verbatim
/// (SCOUT-NATIVE §3).
pub const GRAMMAR: Grammar = Grammar {
    languages: &[Language {
        code: "en",
        stem_strategy: StemStrategy::AllZ,
        stem_prefix: "Z",
    }],
    boolean_prefixes: &[
        BooleanPrefix {
            field: "author",
            prefix: "A",
            lowercase_value: true,
        },
        BooleanPrefix {
            field: "species",
            prefix: "XS",
            lowercase_value: true,
        },
        BooleanPrefix {
            field: "group",
            prefix: "XG",
            lowercase_value: true,
        },
        BooleanPrefix {
            field: "tissue",
            prefix: "XI",
            lowercase_value: true,
        },
        BooleanPrefix {
            field: "dataset",
            prefix: "XDS",
            lowercase_value: true,
        },
        BooleanPrefix {
            field: "symbol",
            prefix: "XY",
            lowercase_value: true,
        },
        BooleanPrefix {
            field: "chr",
            prefix: "XC",
            lowercase_value: true,
        },
        BooleanPrefix {
            field: "peakchr",
            prefix: "XPC",
            lowercase_value: true,
        },
    ],
    probabilistic_prefixes: &[
        ProbabilisticPrefix {
            field: "description",
            prefix: "XD",
            aliases: &[],
        },
        ProbabilisticPrefix {
            field: "rif",
            prefix: "XRF",
            aliases: &["RIF"],
        },
        ProbabilisticPrefix {
            field: "wiki",
            prefix: "XWK",
            aliases: &["WIKI"],
        },
    ],
    numeric_ranges: &[
        NumericRange {
            field: "mean",
            marker: "mean:",
            slot: 0,
        },
        NumericRange {
            field: "peak",
            marker: "peak:",
            slot: 1,
        },
        NumericRange {
            field: "peakmb",
            marker: "peakmb:",
            slot: 3,
        },
        NumericRange {
            field: "additive",
            marker: "additive:",
            slot: 4,
        },
        NumericRange {
            field: "year",
            marker: "year:",
            slot: 5,
        },
    ],
    // `Hs` / `Mm` (SCOUT-NATIVE §3; `native.rs:151-170`): species AND
    // chromosome AND a slot-2 position range, with NO liftover. OPEN-4 is
    // about the liftover OR-branch gn3 has and GN4 does not; it is not a
    // reason to omit the shorthands themselves.
    location_shorthands: &[
        LocationShorthand {
            field: "Hs",
            species_term: "XShuman",
        },
        LocationShorthand {
            field: "Mm",
            species_term: "XSmouse",
        },
    ],
    position: PositionRule {
        fields: &["position", "pos"],
        slot: 2,
        half_window_bases: 50_000.0,
        bases_per_megabase: 1_000_000.0,
    },
    operators: &["AND", "OR", "NOT", "XOR", "NEAR", "ADJ"],
};

/// What a rendered term means to an index backend.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TermKind {
    /// An exact term: match `term` verbatim against the index's terms.
    Boolean,
    /// A free-text field term: `term` is the prefix plus the lowercased
    /// value, which the backend stems (the index stores `Z`-prefixed
    /// stems, SCOUT-NATIVE §3) and scores.
    Probabilistic,
    /// Free text: one word to be stemmed and scored like any other
    /// free-text term (`stem_prefix` + stem). A field token the grammar does
    /// not consume becomes one term per word — `mean:5` is the two words
    /// `mean` and `5` of one phrase (DECISIONS rows 34, 36).
    FreeText,
    /// A value-slot range. Bounds are in the slot's own units (megabases
    /// for [`PositionRule::slot`]) and are `None` where open; both `None`
    /// means the term matches nothing.
    ValueRange {
        slot: u32,
        begin: Option<f64>,
        end: Option<f64>,
    },
    /// A boolean operator between terms. Structure is the backend's.
    Operator,
}

/// One term the index backend consumes.
///
/// The (`prefix`, `term`, `kind`) triple: `prefix` names the index field
/// that produced it (`XDS`, `XD`, `mean:`, `position:`), and is empty for
/// free text and operators. `term` is the string the index works with —
/// for [`TermKind::Boolean`] and [`TermKind::Probabilistic`] it is
/// `prefix` + the value as the index stores it (lowercased for booleans),
/// for [`TermKind::ValueRange`] it is the value as the caller wrote it, and
/// for free text and operators the token verbatim — one word per term for a
/// token the renderer split at a field colon (`mean:5` is two terms).
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedTerm {
    pub prefix: String,
    pub term: String,
    pub kind: TermKind,
    /// Whether this term is one word of a PHRASE rather than a standalone
    /// term: consecutive terms whose `phrase` is set form ONE phrase, which
    /// the backend must hand to its own parser as adjacent words instead of
    /// ANDing them (DECISIONS rows 35, 36).
    ///
    /// Two classes carry it: the label and value of a field token the
    /// grammar does not consume (`mean:5` is xapian's phrase `mean 5`, not
    /// `mean` AND `5` — `native.rs:721-724`), and the words of a quoted
    /// region. A backend that ANDs them answers a different, larger query:
    /// this index carries no positions, so GN4 answers a phrase with 0 hits
    /// (`genenetwork4/docs/search-engines.md:98-102`).
    pub phrase: bool,
}

/// A query, rendered.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedQuery {
    /// The query exactly as the caller wrote it — for a backend that
    /// needs the structure (grouping, phrases, `NEAR` arity) the renderer
    /// does not model.
    pub raw: String,
    /// The document type to filter on (`phenotype`, `gene`), when one is
    /// attached; see [`ParsedQuery::with_type`].
    pub type_filter: Option<String>,
    /// The rendered terms, in query order.
    pub terms: Vec<RenderedTerm>,
}

impl ParsedQuery {
    /// Attach the document type GN4's route validated (`phenotype` or
    /// `gene`; SCOUT-NATIVE §1).
    ///
    /// The type is not grammar — it is an index-schema fact, the `XT{type}`
    /// term the indexer writes (SCOUT-INDEXER §2) — but it rides here
    /// because the assigned index seam is `search(&q, limit, page)`: the
    /// filter has to travel with the query. An empty type attaches no
    /// filter.
    pub fn with_type(mut self, type_: &str) -> Self {
        self.type_filter = if type_.is_empty() {
            None
        } else {
            Some(type_.to_string())
        };
        self
    }

    /// The `XT{type}` term an index filters on, when a type is attached
    /// (SCOUT-NATIVE §3, §4).
    pub fn type_term(&self) -> Option<String> {
        self.type_filter.as_ref().map(|type_| format!("XT{type_}"))
    }
}

/// A query the grammar rejects before any backend sees it.
///
/// Three classes, all of which GN4 reports as 422 `invalid_query` and all of
/// which are pinned by recorded shapes:
///
/// * a numeric-range side that is not a number — gn3's
///   `QueryParserError("Unknown range operation")` (the recorded Q52
///   diagnostic);
/// * an `Hs:`/`Mm:` value that is not `chr<chromosome>:<position-spec>` —
///   gn3's `ValueError`, which GN4 reports as
///   `InvalidOperationError` (the recorded Q42 500, re-baselined to 422 by
///   AMENDMENT E, `native.rs:187-215`);
/// * a syntactically malformed query — xapian's
///   `QueryParserError("Syntax: <expression> AND <expression>")` (the
///   recorded Q51 diagnostic).
///
/// An unparseable `position` value is NOT this: gn3's position processor
/// declines and GN4 treats it as an empty match, so it renders as a range
/// with no bounds instead (DECISIONS row 11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderError {
    /// A numeric range side that is not a number (`mean:abc..def`, Q52).
    RangeBound {
        /// The field whose value did not parse (`mean`).
        field: String,
        /// The value as the caller wrote it (`abc..def`).
        value: String,
    },
    /// An `Hs:`/`Mm:` value that is not a location (`Hs:foo:1M`, Q42).
    InvalidLocation {
        /// The value as the caller wrote it (`foo:1M`).
        value: String,
    },
    /// A query that cannot be parsed as a query at all.
    Syntax {
        /// Which syntax fault was found.
        fault: SyntaxFault,
    },
}

/// The syntax faults [`RenderError::Syntax`] distinguishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyntaxFault {
    /// An operator with no term on one side (`alcohol AND`, the recorded
    /// Q51 shape; `AND alcohol`; `alcohol AND OR dependence`).
    DanglingOperator,
    /// An odd number of `"` — a phrase that never closes.
    ///
    /// No recorded shape pins this one: only the error CLASS (422
    /// `invalid_query`, `QueryParserError`) is the graded part; the
    /// diagnostic text is xapian's own form and unrecorded.
    UnbalancedQuotes,
}

impl RenderError {
    /// The diagnostic GN4 puts in the 422 body (SCOUT-NATIVE §6).
    pub fn message(&self) -> &'static str {
        match self {
            Self::RangeBound { .. } => "Unknown range operation",
            Self::InvalidLocation { .. } => {
                "InvalidOperationError: Query is invalid (invalid Hs/Mm location field)"
            }
            Self::Syntax { fault } => match fault {
                // The recorded Q51 diagnostic, verbatim.
                SyntaxFault::DanglingOperator => "Syntax: <expression> AND <expression>",
                SyntaxFault::UnbalancedQuotes => "Syntax: <expression>",
            },
        }
    }

    /// The exception class GN4 reports alongside it (SCOUT-NATIVE §6).
    pub fn xapian_error_type(&self) -> &'static str {
        match self {
            Self::RangeBound { .. } | Self::Syntax { .. } => "QueryParserError",
            Self::InvalidLocation { .. } => "InvalidOperationError",
        }
    }
}

impl Grammar {
    /// `languages["en"]` — the parser's English configuration
    /// (SCOUT-NATIVE §3).
    pub fn language(&self, code: &str) -> Option<&'static Language> {
        self.languages.iter().find(|language| language.code == code)
    }

    /// The boolean prefix registered for `field` (`dataset` -> `XDS`).
    pub fn boolean_by_field(&self, field: &str) -> Option<&'static BooleanPrefix> {
        self.boolean_prefixes
            .iter()
            .find(|prefix| prefix.field == field)
    }

    /// The boolean prefix registered for an index term prefix
    /// (`XDS` -> `dataset`) — the direction an index backend needs to map
    /// a rendered term back onto a stored field.
    pub fn boolean_by_prefix(&self, prefix: &str) -> Option<&'static BooleanPrefix> {
        self.boolean_prefixes
            .iter()
            .find(|entry| entry.prefix == prefix)
    }

    /// The free-text prefix registered for `field`, or for one of its
    /// aliases (`rif` and `RIF` both answer).
    pub fn probabilistic_by_field(&self, field: &str) -> Option<&'static ProbabilisticPrefix> {
        self.probabilistic_prefixes
            .iter()
            .find(|prefix| prefix.field == field || prefix.aliases.contains(&field))
    }

    /// The free-text prefix registered for an index term prefix
    /// (`XD` -> `description`).
    pub fn probabilistic_by_prefix(&self, prefix: &str) -> Option<&'static ProbabilisticPrefix> {
        self.probabilistic_prefixes
            .iter()
            .find(|entry| entry.prefix == prefix)
    }

    /// The numeric range field called `field` (`mean` -> slot 0).
    pub fn numeric_range(&self, field: &str) -> Option<&'static NumericRange> {
        self.numeric_ranges
            .iter()
            .find(|range| range.field == field)
    }

    /// The species-location shorthand called `field` (`Hs` -> `XShuman`).
    pub fn location_shorthand(&self, field: &str) -> Option<&'static LocationShorthand> {
        self.location_shorthands
            .iter()
            .find(|shorthand| shorthand.field == field)
    }

    /// Whether `field` selects the position rule (`position`, `pos`).
    pub fn is_position_field(&self, field: &str) -> bool {
        self.position.fields.contains(&field)
    }

    /// Whether `token` is a boolean operator (uppercase only).
    pub fn is_operator(&self, token: &str) -> bool {
        self.operators.contains(&token)
    }

    /// Whether `field` is one of the grammar's registered field names.
    ///
    /// This is the test for "that colon was a field marker, not text": an
    /// unregistered name (`foo:bar`, the recorded Q50 shape) is not a field,
    /// so there is nothing to split around its colon.
    fn is_registered_field(&self, field: &str) -> bool {
        self.boolean_by_field(field).is_some()
            || self.probabilistic_by_field(field).is_some()
            || self.numeric_range(field).is_some()
            || self.is_position_field(field)
            || self.location_shorthand(field).is_some()
    }

    /// The free-text terms for a token the field dispatch did not consume.
    ///
    /// An unregistered field name (`foo:bar`, Q50) is not a field, so the
    /// token stays one term as written. A REGISTERED field whose value the
    /// grammar does not consume is text, and xapian reads its colon as a
    /// word break: `mean:5` is a `mean 5` phrase (`native.rs:722-724`), and
    /// the recorded Q53 `position:` matched 576 documents on the word
    /// `position` — so the label is a word, not a prefix to discard. The
    /// label and the value become separate terms, and an empty value emits
    /// the label alone.
    ///
    /// The label and the value are the two words of ONE phrase, so both
    /// carry [`RenderedTerm::phrase`] (DECISIONS row 36): xapian builds
    /// adjacency there, and a backend that ANDs the words instead answers a
    /// different query.
    ///
    /// Quote marks are phrase delimiters, not term content, so they are
    /// trimmed from each word: `mean:"1..2"` is the two words `mean` and
    /// `1..2` of one phrase. A token that STARTS inside a quoted region is
    /// phrase content whole — no field split, no colon split, no field
    /// error — because GN4 decides that per query (`native.rs:376-385`).
    fn free_text_terms(&self, token: &str, in_quoted_region: bool) -> Vec<RenderedTerm> {
        if in_quoted_region {
            return text_words(token, true);
        }
        let quoted = token.contains('"');
        let Some((field, value)) = token.split_once(':') else {
            return text_words(token, quoted);
        };
        if !self.is_registered_field(field) {
            return text_words(token, quoted);
        }
        if value.is_empty() {
            return text_words(field, quoted);
        }
        let mut words = text_words(field, true);
        words.extend(text_words(value, true));
        words
    }

    /// Render `q` into the terms an index backend consumes.
    ///
    /// Total on every input EXCEPT the three [`RenderError`] classes:
    /// anything that is not a registered field term with a value, or an
    /// operator, renders as free text. Token-level only — a value ends at
    /// whitespace or at a quote, and grouping is left to the backend (see
    /// the module doc).
    pub fn render(&self, q: &str) -> Result<ParsedQuery, RenderError> {
        if q.matches('"').count() % 2 == 1 {
            return Err(RenderError::Syntax {
                fault: SyntaxFault::UnbalancedQuotes,
            });
        }
        let mut terms = Vec::new();
        for (token, in_quoted_region) in quoted_tokens(q) {
            // Structure characters are not modelled; a token's own
            // parentheses are trimmed so `(dataset:bxd)` still reads as a
            // dataset term.
            let token = token.trim_start_matches('(').trim_end_matches(')');
            if token.is_empty() {
                continue;
            }
            // Nothing inside a quoted region is a field prefix or an
            // operator: GN4's quote state is a property of the QUERY, not of
            // the token (`native.rs:376-385`), so `"alcohol mean:abc..def"`
            // is phrase content and not the Q52 422.
            if !in_quoted_region {
                if self.is_operator(token) {
                    terms.push(RenderedTerm {
                        prefix: String::new(),
                        term: token.to_string(),
                        kind: TermKind::Operator,
                        phrase: false,
                    });
                    continue;
                }
                // A field term needs a value. An empty one (`position:`, which
                // Q53 records as a 200 with 576 hits) never reaches a processor
                // and is free text like any other bare word
                // (`native.rs:264-270`).
                if let Some((field, value)) =
                    token.split_once(':').filter(|(_, value)| !value.is_empty())
                {
                    if let Some(prefix) = self.boolean_by_field(field) {
                        let value = if prefix.lowercase_value {
                            value.to_lowercase()
                        } else {
                            value.to_string()
                        };
                        terms.push(RenderedTerm {
                            prefix: prefix.prefix.to_string(),
                            term: format!("{}{value}", prefix.prefix),
                            kind: TermKind::Boolean,
                            phrase: false,
                        });
                        continue;
                    }
                    if let Some(prefix) = self.probabilistic_by_field(field) {
                        terms.push(RenderedTerm {
                            prefix: prefix.prefix.to_string(),
                            term: format!("{}{}", prefix.prefix, value.to_lowercase()),
                            kind: TermKind::Probabilistic,
                            phrase: false,
                        });
                        continue;
                    }
                    if let Some(range) = self.numeric_range(field) {
                        // Only a value containing `..` is a range at all: xapian
                        // consults a range processor for range syntax only, so
                        // `mean:5` is a `mean 5` phrase and never a range
                        // (`native.rs:389-405`, `native.rs:722-725`). Anything
                        // else falls through to free text below.
                        if let Some((begin, end)) = parse_range(field, value)? {
                            terms.push(RenderedTerm {
                                prefix: range.marker.to_string(),
                                term: value.to_string(),
                                kind: TermKind::ValueRange {
                                    slot: range.slot,
                                    begin,
                                    end,
                                },
                                phrase: false,
                            });
                            // Everything from a quote on is phrase content the
                            // value did not consume: `mean:1.."2"` is the range
                            // `1..` AND the phrase word `2`, so the tail travels
                            // rather than being dropped.
                            if let Some((_, rest)) = value.split_once('"') {
                                terms.extend(text_words(rest, true));
                            }
                            continue;
                        }
                    }
                    if self.is_position_field(field) {
                        // An unparseable position declines to a range with no
                        // bounds, which every backend reads as "match nothing":
                        // gn3's escaped `ValueError` is not a contract, and GN4's
                        // blessed behaviour for the class is a 200 with no hits
                        // (SCOUT-NATIVE §3, §6 — the parity-metric amendment).
                        let (begin, end) =
                            parse_position(value, &self.position).unwrap_or_default();
                        terms.push(RenderedTerm {
                            prefix: format!("{field}:"),
                            term: value.to_string(),
                            kind: TermKind::ValueRange {
                                slot: self.position.slot,
                                begin,
                                end,
                            },
                            phrase: false,
                        });
                        continue;
                    }
                    if let Some(shorthand) = self.location_shorthand(field) {
                        terms.extend(self.location_terms(shorthand, value)?);
                        continue;
                    }
                }
            }
            terms.extend(self.free_text_terms(token, in_quoted_region));
        }
        self.check_operators(&terms)?;
        Ok(ParsedQuery {
            raw: q.to_string(),
            type_filter: None,
            terms,
        })
    }

    /// Render an `Hs:`/`Mm:` value into the three terms the index ANDs
    /// together: the species term, the chromosome term and the slot-2
    /// position range (GN4's `LocationProcessor`, `native.rs:310-350`).
    ///
    /// The value is lowercased before it is split, exactly as gn3 does, and
    /// it has to be exactly `chr<chromosome>:<position-spec>`; a chromosome
    /// not starting with `chr`, or an unparseable position spec, is gn3's
    /// `ValueError` class — the recorded Q42 shape, which GN4 reports as
    /// `InvalidOperationError` instead of reproducing the mset crash
    /// (`native.rs:187-215`, AMENDMENT E).
    fn location_terms(
        &self,
        shorthand: &LocationShorthand,
        value: &str,
    ) -> Result<Vec<RenderedTerm>, RenderError> {
        let invalid = || RenderError::InvalidLocation {
            value: value.to_string(),
        };
        let value = value.to_lowercase();
        let mut parts = value.split(':');
        let (chromosome, position_spec) = match (parts.next(), parts.next(), parts.next()) {
            (Some(chromosome), Some(position_spec), None) => (chromosome, position_spec),
            _ => return Err(invalid()),
        };
        let chrom_number = chromosome.strip_prefix("chr").ok_or_else(invalid)?;
        let (begin, end) = parse_position(position_spec, &self.position).ok_or_else(invalid)?;
        // The two boolean prefixes come from the grammar's own table, so a
        // renumbered prefix cannot drift: `XS` + the species value is the
        // stored term `XShuman`, `XC` + the chromosome number is `XC2`
        // (`native.rs:351-377`).
        let species_prefix = self
            .boolean_by_field("species")
            .map_or("XS", |prefix| prefix.prefix);
        let chromosome_prefix = self
            .boolean_by_field("chr")
            .map_or("XC", |prefix| prefix.prefix);
        Ok(vec![
            RenderedTerm {
                prefix: species_prefix.to_string(),
                term: shorthand.species_term.to_string(),
                kind: TermKind::Boolean,
                phrase: false,
            },
            RenderedTerm {
                prefix: chromosome_prefix.to_string(),
                term: format!("{chromosome_prefix}{chrom_number}"),
                kind: TermKind::Boolean,
                phrase: false,
            },
            RenderedTerm {
                prefix: format!("{}:", shorthand.field),
                term: position_spec.to_string(),
                kind: TermKind::ValueRange {
                    slot: self.position.slot,
                    begin,
                    end,
                },
                phrase: false,
            },
        ])
    }

    /// Reject an operator no parser can place: one with no term on one side
    /// (`alcohol AND` — the recorded Q51 shape, a 422 at GN4), or two
    /// operators in a row (`alcohol AND OR dependence`).
    ///
    /// `NOT` is a prefix operator, so it needs no term on its left: it is
    /// exempt in leading position, after itself, and after a binary
    /// operator. `alcohol AND NOT dependence` and `a AND (NOT b)` are the
    /// canonical xapian exclusions; rejecting them would be a 422 where
    /// production answers, the one direction a parity claim cannot afford
    /// (`REVIEW-FIXES.md` §d blocker 1).
    fn check_operators(&self, terms: &[RenderedTerm]) -> Result<(), RenderError> {
        let is_operator = |term: &RenderedTerm| term.kind == TermKind::Operator;
        let exempt = |term: &RenderedTerm| term.term == "NOT";
        for (index, term) in terms.iter().enumerate() {
            if !is_operator(term) {
                continue;
            }
            let before = index.checked_sub(1).map(|previous| &terms[previous]);
            let dangling = match before {
                None => !exempt(term),
                // A `NOT` closes the gap its binary operator opened.
                Some(previous) => is_operator(previous) && !exempt(previous) && !exempt(term),
            } || terms.get(index + 1).is_none();
            if dangling {
                return Err(RenderError::Syntax {
                    fault: SyntaxFault::DanglingOperator,
                });
            }
        }
        Ok(())
    }
}

/// `q`'s whitespace-separated tokens, each tagged with whether its first
/// character lies INSIDE a quoted region.
///
/// GN4 decides what is a field term per QUERY, not per token: its range guard
/// walks the whole string with an `in_quotes` state machine
/// (`native.rs:376-385`), so a `<prefix>:` inside a quoted phrase is phrase
/// content and never reaches a range processor, and its value stops at the
/// next quote (`native.rs:398-401`). The renderer mirrors that machine
/// instead of testing the token's own first character, which is what made
/// `"alcohol mean:abc..def"` a 422 here and a 200 at GN4
/// (`REVIEW-BLOCKERS.md` §5 blocker 1).
fn quoted_tokens(q: &str) -> Vec<(&str, bool)> {
    let mut spans: Vec<(usize, usize, bool)> = Vec::new();
    let mut in_quotes = false;
    let mut start: Option<(usize, bool)> = None;
    for (index, ch) in q.char_indices() {
        if ch.is_whitespace() {
            if let Some((begin, quoted)) = start.take() {
                spans.push((begin, index, quoted));
            }
            continue;
        }
        if start.is_none() {
            start = Some((index, in_quotes));
        }
        if ch == '"' {
            in_quotes = !in_quotes;
        }
    }
    if let Some((begin, quoted)) = start {
        spans.push((begin, q.len(), quoted));
    }
    spans
        .into_iter()
        .map(|(begin, end, quoted)| (&q[begin..end], quoted))
        .collect()
}

/// The free-text terms for one token, each tagged with whether it is one word
/// of a PHRASE rather than a standalone term ([`RenderedTerm::phrase`]).
///
/// Quote marks are phrase delimiters, not term content, so they are trimmed
/// from the word; a word that is nothing but quotes emits no term at all.
fn text_words(token: &str, phrase: bool) -> Vec<RenderedTerm> {
    let word = token.trim_matches('"');
    if word.is_empty() {
        return Vec::new();
    }
    vec![RenderedTerm {
        prefix: String::new(),
        term: word.to_string(),
        kind: TermKind::FreeText,
        phrase,
    }]
}

/// Parse a numeric range value: `<num>..<num>`, with either side optionally
/// open (`..5`, `5..`, `..`).
///
/// `None` means the value is not a range at all, so the token is free text:
/// xapian consults a range processor for UNQUOTED range syntax only, which
/// makes `mean:5` a `mean 5` phrase, `mean:"1..2"` phrase content and
/// `mean:abc` text rather than an error (`native.rs:389-405` inspects only
/// unquoted values containing `..`, stopping the value at a quote;
/// `native.rs:722-725` asserts the `mean:5` case). Both sides open (`..`) is
/// the both-empty range, which matches nothing.
//
// `clippy::type_complexity`: the nesting IS the return contract — `None` is
// "not a range", `Some((None, None))` is "a range with both sides open", and a
// `type` alias would rename that without making it any easier to read. The
// honest fix is a named bounds struct, which would change `parse_range` and
// `parse_position` together; that is a refactor of the rendering seam, not a
// lint fix, so it is recorded in `docs/KNOWN-DEBT.md` instead.
#[allow(clippy::type_complexity)]
fn parse_range(
    field: &str,
    value: &str,
) -> Result<Option<(Option<f64>, Option<f64>)>, RenderError> {
    // The value ENDS at the first quote, as it does for the parser
    // (`native.rs:398-401` stops a field value at whitespace, a parenthesis
    // or a `"`): a quoted part is phrase content and never a bound, so
    // `mean:"1..2"` has no unquoted value at all — not a range, and not the
    // Q52 "Unknown range operation" error — and `mean:1.."2"` is the
    // one-sided range `1..` (`REVIEW-BLOCKERS.md` §5 blocker 1).
    let value = value.split('"').next().unwrap_or_default();
    let bound = |side: &str| -> Option<Option<f64>> {
        if side.is_empty() {
            Some(None)
        } else {
            side.parse::<f64>().ok().map(Some)
        }
    };
    let Some((begin, end)) = value.split_once("..") else {
        return Ok(None);
    };
    let unparseable = || RenderError::RangeBound {
        field: field.to_string(),
        value: value.to_string(),
    };
    let begin = bound(begin).ok_or_else(unparseable)?;
    let end = bound(end).ok_or_else(unparseable)?;
    Ok(Some((begin, end)))
}

/// gn3's `parse_position` (SCOUT-NATIVE §3; gn3 `search.py:145-165`): a
/// `..` spec is a range with either side optionally open, and a point
/// expands by [`PositionRule::half_window_bases`] on each side, clamped at
/// 0.
///
/// Bounds are returned in MEGABASES — the unit slot 2 holds — so a backend
/// compares them against the slot value without knowing about bases.
fn parse_position(spec: &str, rule: &PositionRule) -> Option<(Option<f64>, Option<f64>)> {
    let to_megabases = |bases: f64| bases / rule.bases_per_megabase;
    match spec.split_once("..") {
        Some((begin, end)) => {
            let begin = if begin.is_empty() {
                None
            } else {
                Some(to_megabases(apply_si_suffix(begin)?))
            };
            let end = if end.is_empty() {
                None
            } else {
                Some(to_megabases(apply_si_suffix(end)?))
            };
            Some((begin, end))
        }
        None => {
            let point = apply_si_suffix(spec)?;
            Some((
                Some(to_megabases((point - rule.half_window_bases).max(0.0))),
                Some(to_megabases(point + rule.half_window_bases)),
            ))
        }
    }
}

/// gn3's `apply_si_suffix` (SCOUT-NATIVE §3; gn3 `search.py:136-142`): an
/// optional trailing `k`/`m`/`g` (any case) multiplies by 10^3/6/9; a
/// value without a suffix must be an integer, so `1.5` is not a
/// coordinate (gn3 raises `ValueError` there, which is why the caller
/// treats `None` as a decline).
fn apply_si_suffix(value: &str) -> Option<f64> {
    let last = value.chars().last()?;
    let multiplier = match last.to_ascii_lowercase() {
        'k' => 1e3,
        'm' => 1e6,
        'g' => 1e9,
        // No suffix: `int(location)` in gn3, so a decimal point declines.
        _ => return value.parse::<i64>().ok().map(|number| number as f64),
    };
    // The last char is an ASCII suffix letter here, so this slice is on a
    // character boundary.
    let digits = &value[..value.len() - last.len_utf8()];
    digits.parse::<f64>().ok().map(|number| number * multiplier)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One case per boolean prefix: the field name, the index term prefix,
    /// and the lowercase rule all have to be right together, because the
    /// index holds only lowercase boolean terms (SCOUT-NATIVE §3).
    #[test]
    fn boolean_prefix_author() {
        let prefix = GRAMMAR.boolean_by_field("author").expect("registered");
        assert_eq!(prefix.prefix, "A");
        assert!(prefix.lowercase_value);
        assert_eq!(GRAMMAR.boolean_by_prefix("A"), Some(prefix));
    }

    #[test]
    fn boolean_prefix_species() {
        let prefix = GRAMMAR.boolean_by_field("species").expect("registered");
        assert_eq!(prefix.prefix, "XS");
        assert!(prefix.lowercase_value);
    }

    #[test]
    fn boolean_prefix_group() {
        let prefix = GRAMMAR.boolean_by_field("group").expect("registered");
        assert_eq!(prefix.prefix, "XG");
        assert!(prefix.lowercase_value);
    }

    #[test]
    fn boolean_prefix_tissue() {
        let prefix = GRAMMAR.boolean_by_field("tissue").expect("registered");
        assert_eq!(prefix.prefix, "XI");
        assert!(prefix.lowercase_value);
    }

    #[test]
    fn boolean_prefix_dataset() {
        let prefix = GRAMMAR.boolean_by_field("dataset").expect("registered");
        assert_eq!(prefix.prefix, "XDS");
        assert!(prefix.lowercase_value);
    }

    #[test]
    fn boolean_prefix_symbol() {
        let prefix = GRAMMAR.boolean_by_field("symbol").expect("registered");
        assert_eq!(prefix.prefix, "XY");
        assert!(prefix.lowercase_value);
    }

    #[test]
    fn boolean_prefix_chr() {
        let prefix = GRAMMAR.boolean_by_field("chr").expect("registered");
        assert_eq!(prefix.prefix, "XC");
        assert!(prefix.lowercase_value);
    }

    #[test]
    fn boolean_prefix_peakchr() {
        let prefix = GRAMMAR.boolean_by_field("peakchr").expect("registered");
        assert_eq!(prefix.prefix, "XPC");
        assert!(prefix.lowercase_value);
    }

    /// One case per free-text prefix, aliases included: gn3 registers both
    /// the lowercase field and its uppercase alias (SCOUT-NATIVE §3).
    #[test]
    fn probabilistic_prefix_description() {
        let prefix = GRAMMAR
            .probabilistic_by_field("description")
            .expect("registered");
        assert_eq!(prefix.prefix, "XD");
        assert!(prefix.aliases.is_empty());
    }

    #[test]
    fn probabilistic_prefix_rif() {
        let prefix = GRAMMAR.probabilistic_by_field("rif").expect("registered");
        assert_eq!(prefix.prefix, "XRF");
        assert_eq!(
            GRAMMAR.probabilistic_by_field("RIF").map(|p| p.prefix),
            Some("XRF"),
        );
    }

    #[test]
    fn probabilistic_prefix_wiki() {
        let prefix = GRAMMAR.probabilistic_by_field("wiki").expect("registered");
        assert_eq!(prefix.prefix, "XWK");
        assert_eq!(
            GRAMMAR.probabilistic_by_field("WIKI").map(|p| p.prefix),
            Some("XWK"),
        );
    }

    /// One case per numeric range slot: the slot numbers are the indexer's
    /// (SCOUT-INDEXER §2), verified against gn3's literal slot list.
    #[test]
    fn numeric_range_mean_slot() {
        assert_eq!(GRAMMAR.numeric_range("mean").map(|r| r.slot), Some(0));
        assert_eq!(
            GRAMMAR.numeric_range("mean").map(|r| r.marker),
            Some("mean:")
        );
    }

    #[test]
    fn numeric_range_peak_slot() {
        assert_eq!(GRAMMAR.numeric_range("peak").map(|r| r.slot), Some(1));
        assert_eq!(
            GRAMMAR.numeric_range("peak").map(|r| r.marker),
            Some("peak:")
        );
    }

    #[test]
    fn numeric_range_peakmb_slot() {
        assert_eq!(GRAMMAR.numeric_range("peakmb").map(|r| r.slot), Some(3));
        assert_eq!(
            GRAMMAR.numeric_range("peakmb").map(|r| r.marker),
            Some("peakmb:"),
        );
    }

    #[test]
    fn numeric_range_additive_slot() {
        assert_eq!(GRAMMAR.numeric_range("additive").map(|r| r.slot), Some(4));
        assert_eq!(
            GRAMMAR.numeric_range("additive").map(|r| r.marker),
            Some("additive:"),
        );
    }

    #[test]
    fn numeric_range_year_slot() {
        assert_eq!(GRAMMAR.numeric_range("year").map(|r| r.slot), Some(5));
        assert_eq!(
            GRAMMAR.numeric_range("year").map(|r| r.marker),
            Some("year:")
        );
    }

    /// Slot 2 is the position slot, reached through `position` and `pos`
    /// only, and a point expands by 50 kb (SCOUT-NATIVE §3).
    #[test]
    fn position_rule_slot_and_window() {
        assert_eq!(GRAMMAR.position.slot, 2);
        assert!(GRAMMAR.is_position_field("position"));
        assert!(GRAMMAR.is_position_field("pos"));
        assert!(!GRAMMAR.is_position_field("peak"));
        assert_eq!(GRAMMAR.position.half_window_bases, 50_000.0);
        assert_eq!(GRAMMAR.position.bases_per_megabase, 1_000_000.0);
    }

    /// One case per species-location shorthand: the field name is the query
    /// spelling and the species term is what the index stores (GN4
    /// registers `XShuman`/`XSmouse` with no liftover — `native.rs:151-170`).
    #[test]
    fn location_shorthand_hs_is_human() {
        let shorthand = GRAMMAR.location_shorthand("Hs").expect("registered");
        assert_eq!(shorthand.species_term, "XShuman");
    }

    #[test]
    fn location_shorthand_mm_is_mouse() {
        let shorthand = GRAMMAR.location_shorthand("Mm").expect("registered");
        assert_eq!(shorthand.species_term, "XSmouse");
        assert_eq!(
            GRAMMAR.location_shorthand("hs"),
            None,
            "field names are exact"
        );
        assert_eq!(GRAMMAR.location_shorthand("chr"), None);
    }

    /// Free text is stemmed English with the `Z` tag (`STEM_ALL_Z`) —
    /// `languages["en"]` (SCOUT-NATIVE §3).
    #[test]
    fn language_en_stems_all_z() {
        let language = GRAMMAR.language("en").expect("English is registered");
        assert_eq!(language.stem_strategy, StemStrategy::AllZ);
        assert_eq!(language.stem_prefix, "Z");
        assert_eq!(GRAMMAR.language("de"), None);
    }

    /// Operators are uppercase only; the lowercase forms are plain terms
    /// (SCOUT-NATIVE §3).
    #[test]
    fn operators_are_uppercase_only() {
        assert!(GRAMMAR.is_operator("AND"));
        assert!(GRAMMAR.is_operator("ADJ"));
        assert!(!GRAMMAR.is_operator("and"));
    }

    /// SI suffixes multiply; an unsuffixed value must be an integer, so a
    /// decimal declines (gn3's `int(location)`, SCOUT-NATIVE §3).
    #[test]
    fn si_suffix_multiplies_and_declines_decimals() {
        assert_eq!(apply_si_suffix("1M"), Some(1_000_000.0));
        assert_eq!(apply_si_suffix("2k"), Some(2_000.0));
        assert_eq!(apply_si_suffix("1g"), Some(1_000_000_000.0));
        assert_eq!(apply_si_suffix("500"), Some(500.0));
        assert_eq!(apply_si_suffix("1.5"), None);
        assert_eq!(apply_si_suffix(""), None);
        assert_eq!(apply_si_suffix("1.5M"), Some(1_500_000.0));
    }
}
