# WORKER-FINAL — the three blockers from `docs/REVIEW-BLOCKERS.md` §5

Scope: exactly the three blockers in `docs/REVIEW-BLOCKERS.md` §5, plus the three
one-line non-blocking notes it lists. Nothing else was changed.

Build: **`cargo` works in this tree now.** The pinned toolchain
`~/.rustup/toolchains/1.88.0-x86_64-unknown-linux-gnu/bin/cargo` resolves `serde`/
`serde_json` offline from `~/.cargo/registry`, so every claim below is `cargo test`, not
`rustc`-by-hand. Nothing was built inside `/home/kabui/project5/genenetwork4` (read only:
`src/search/native.rs`, `test/data/parity-fixtures/Q*.json`).

**Results: 84 tests, 0 failed — 32 lib unit, 4 `error_status`, 26 `grammar_render`,
6 `hit_roundtrip`, 7 `manifest`, 9 `parity_corpus`.** Per-blocker fail-before runs were
executed against scratch copies with the fix reverted (§5).

---

## 1 · Verdicts

| # | `REVIEW-BLOCKERS.md` §5 | Verdict | Proving code | Test |
|---|---|---|---|---|
| B-A (its blocker 1) | B4 per-token quote test | **FIXED** | `src/grammar.rs:806` (`quoted_tokens`), `:575` (`render`), `:861` (`parse_range`) | `tests/grammar_render.rs:248` `a_range_looking_token_inside_a_quote_is_not_a_422` |
| B-B (its blocker 2) | B3 AND-of-words, verdict FIXED | **FIXED (the honesty fix)** — marker in the data, class documented, runner skips by name, verdict corrected to PARTIALLY | `src/grammar.rs:328` (`RenderedTerm::phrase`), `:549` (`free_text_terms`), `:838` (`text_words`); `tests/parity_corpus.rs:385` (`is_phrase_shape`); `docs/DECISIONS.md` rows 35/36 | `tests/grammar_render.rs:179` `a_bare_field_value_is_a_phrase_not_an_and`; `tests/parity_corpus.rs:1079` `a_phrase_shape_is_skipped_as_unrepresentable` |
| B-C (its blocker 3) | floor protects a count, not the five shapes | **FIXED** | `tests/parity_corpus.rs:85` (`MUST_GRADE`), `:537` (`evidence_shortfall`) | `tests/parity_corpus.rs:993` `one_must_grade_shape_turned_into_a_skip_fails_the_run` |

The **B3 verdict is now PARTIALLY, with the reason stated** (`docs/WORKER-BLOCKERS.md` §1
table and §4, plus the correction banner at its top): the label is out of the value term,
but `year:2014` is still not the query GN4 builds — GN4 builds a phrase
(`native.rs:721-724`) that its positionless index answers with 0 hits (Q04/Q11/Q12), while
an AND of the words matches something else. Nothing in this crate grades the phrase class
today, and nothing can until a real backend exists; what changed is that the gap is now
**expressible** (`RenderedTerm::phrase`) instead of implicit, and **skipped by name**
instead of invisible.

---

## 2 · B-A — quote state is a property of the QUERY

`src/grammar.rs`

* `:806` `quoted_tokens(q) -> Vec<(&str, bool)>` — one walk of `q` with an `in_quotes` state
  machine (the same machine `guard_numeric_ranges` runs, `native.rs:376-385`) that tags each
  whitespace token with whether its **first character lies inside a quoted region**.
  `render` (`:575`) iterates it instead of `q.split_whitespace()`, and inside a quoted region
  runs neither the operator classification nor the field dispatch — so no prefix lookup, no
  range parse, no `InvalidLocation`.
* `:861` `parse_range` — **the value ends at the first quote** (`native.rs:399`: a field value
  stops at whitespace, a parenthesis or `"`), replacing `value.starts_with('"')`. So
  `mean:"1..2"` still has no unquoted value (not a range, not the Q52 error), and
  `mean:1.."2"` is the one-sided range `1..` — with the quoted tail emitted as phrase content
  rather than dropped, because dropping a user's constraint matches *more* than GN4 does.
* `:549` `free_text_terms(token, in_quoted_region)` — a token inside a quoted region is
  phrase content whole: no colon split, one term. `:838` `text_words` is the single word
  builder (quote marks trimmed, empty word → no term).

Test `tests/grammar_render.rs:248` `a_range_looking_token_inside_a_quote_is_not_a_422`:

* `"alcohol mean:abc..def"` → renders **Ok**, two phrase words (`alcohol`,
  `mean:abc..def`), no `ValueRange`, and `gn_search::search` returns `Ok` (a 200 at GN4);
* `mean:1.."2"` → renders **Ok**, `ValueRange { slot: 0, begin: Some(1.0), end: None }` plus
  the phrase word `2`, and `search` returns `Ok`;
* the unquoted `mean:abc..def` is still `RenderError::RangeBound { field: "mean", value:
  "abc..def" }` (the recorded Q52 422), asserted in the same test and again in the
  pre-existing `tests/grammar_render.rs:409` `bad_numeric_bound_is_an_invalid_query`.

**Fails on the old code** (verified, §5): `the query renders: RangeBound { field: "mean",
value: "abc..def\"" }` — i.e. the quoted phrase was a 422 here and a 200 at GN4.

Behaviour changes worth naming: `AND`/`NEAR`/`ADJ` inside quotes are no longer operator
terms (they are phrase words — GN4's parser treats quoted content as phrase content only;
pinned by the same test, `"alcohol AND dependence"` → three `FreeText` `phrase` terms), and
a `mean:1.."2"`-shaped token now yields two terms instead of one.

## 3 · B-B — the rendered form says "phrase", and the runner skips the class by name

`src/grammar.rs`

* `:328` `RenderedTerm` gains `pub phrase: bool`: consecutive terms with `phrase` set are ONE
  phrase, which the backend must hand to xapian's own parser instead of ANDing. Set for
  (a) the label and value of a field token the grammar does not consume — `mean:5` →
  `mean`+`5`, `year:2014` → `year`+`2014`, i.e. exactly the `mean 5` phrase of
  `native.rs:721-724` — and (b) the words of a quoted region. A single word
  (`position:` → `position`) and an unregistered name (`foo:bar`, Q50) stay `phrase: false`:
  there is no adjacency to express.
* `:549`/`:838` are where it is decided; nothing else in the crate reads it, and no backend
  behaviour changed (the fixture still ANDs — it cannot do better).

Tests:

* `tests/grammar_render.rs:179` `a_bare_field_value_is_a_phrase_not_an_and` — `year:2014`
  and `mean:5` render two `FreeText` terms with `phrase == true`; `position:`, `foo:bar`,
  `alcohol` and `mean:1..2` are `phrase == false`.
* `tests/parity_corpus.rs:1079` `a_phrase_shape_is_skipped_as_unrepresentable` — Q04, Q11,
  Q12 **and** the bare-value class (`year:2014` as `Q55`, `mean:5` as `Q56`) each print
  `SKIP(phrase-not-expressible…)` and `Graded::Nothing`. The skip predicate
  (`tests/parity_corpus.rs:385`) now keys on the marker plus `NEAR`/`ADJ`, replacing the
  raw-string `raw_query.contains('"')` sniff — which is what made the `year:2014` class fall
  through to the generic free-text skip and become indistinguishable from a real AND.

**Fail-before** (verified, §5): with the marker reverted, `"year:2014" is a phrase at GN4,
not an AND of words: [RenderedTerm { …, phrase: false }, RenderedTerm { …, phrase: false }]`;
with the skip predicate reverted to the raw-string test,
`Q55 must say why it is skipped, got "free text is stemmed and scored by the real index…"`.

Documentation: `docs/DECISIONS.md` row 36 records the marker (and row 35 now points at it),
row 37 records the per-query quote state of B-A, row 34 points at row 36.
`docs/WORKER-BLOCKERS.md` corrects B3 FIXED → **PARTIALLY** in the §1 table and §4, and its
§8.3 residual says what is still not graded.

## 4 · B-C — the graded ids are asserted, not just the count

`tests/parity_corpus.rs`

* `:85` `const MUST_GRADE: [&str; 5] = ["Q15", "Q29", "Q31", "Q36", "Q37"]` — **verified
  against the live fixtures**, not copied: `cargo test --test parity_corpus -- --nocapture`
  prints `set-eq | PASS` for exactly those five (Q31 → 2 hits, Q36 → 2 hits, Q15/Q29/Q37 →
  5 hits) and `vacuous` for the other seven comparable shapes. The list is what the report
  says it is.
* `:537` `evidence_shortfall(rows: &[Row])` now has two layers: every `MUST_GRADE` id must be
  present, must have `graded.is_match_set()` and must be `Verdict::Pass`; and
  `counts.graded >= MIN_GRADED` (`:70`, unchanged) stays as the coarser backstop.
  `corpus_report` calls it with the rows and panics with the joined shortfalls.
* `:993` `one_must_grade_shape_turned_into_a_skip_fails_the_run` — five passing rows is
  `None`; then Q36 becomes a `SKIP` **and a sixth shape grades in its place**, so
  `counts.graded == MIN_GRADED` and only the named layer can fail: the run fails and the
  message names Q36. The same test also covers a named shape missing from the corpus (Q31)
  and one that turned `VACUOUS` (Q29).
* `tests/parity_corpus.rs:945` `a_thinned_run_falls_below_the_evidence_floor` was updated to
  build its rows from `MUST_GRADE` (it now exercises both layers) and still asserts the
  count layer alone: `MIN_GRADED` rows is `None`, one fewer is `Some`, 54 all-SKIP rows is
  `Some`.

**Fail-before** (verified, §5): with the named layer deleted (count-only, the old code),
`one_must_grade_shape_turned_into_a_skip_fails_the_run` panics with `a lost must_grade shape
must fail` **while `corpus_report` still passes** — the reviewer's exact scenario.

## 5 · Fail-before runs (executed, not argued)

Each fix was reverted in its own scratch copy (`/tmp/gns_rev_*`, copied from the finished
tree) and the new tests were run against it:

| copy | revert | command | result |
|---|---|---|---|
| `/tmp/gns_rev_a` | B-A: back to the per-token test (`value.starts_with('"')`) and `in_quoted_region` forced `false` | `cargo test --offline --test grammar_render` | **25 passed, 1 failed** — `a_range_looking_token_inside_a_quote_is_not_a_422`: `the query renders: RangeBound { field: "mean", value: "abc..def\"" }` |
| `/tmp/gns_rev_b` | B-B: `text_words` ignores `phrase` (`phrase: false`) | `cargo test --offline` | **82 passed, 2 failed** — `a_bare_field_value_is_a_phrase_not_an_and`: `"year:2014" is a phrase at GN4, not an AND of words: [RenderedTerm { prefix: "", term: "year", kind: FreeText, phrase: false }, …]` (the quote test fails too, because it also asserts the marker) |
| `/tmp/gns_rev_c` | B-C: named layer removed from `evidence_shortfall` (count layer kept) | `cargo test --offline --test parity_corpus` | **8 passed, 1 failed** — `one_must_grade_shape_turned_into_a_skip_fails_the_run`: `a lost must_grade shape must fail`; `corpus_report` still PASSed |
| `/tmp/gns_rev_d` | B-B runner half: `is_phrase_shape` back to `raw_query.contains('"') \|\| NEAR/ADJ` | `cargo test --offline --test parity_corpus` | **8 passed, 1 failed** — `Q55 must say why it is skipped, got "free text is stemmed and scored by the real index…"` |

## 6 · Non-blocking notes (all three, one line each)

* **(b) wording** — `docs/CORPUS-RUNNER.md` ("So the match-set evidence is **five**
  comparisons … **two of them two-sided**", and the exit-rule paragraph) and
  `docs/WORKER-BLOCKERS.md` §3 now say *five comparisons, two of them two-sided* (Q31, Q36),
  instead of claiming the five can distinguish a correct renderer from any wrong one.
* **(c) clause-(b) comment** — `tests/parity_corpus.rs:729-739` (inside `grade`, above
  `if recorded_top.len() >= 10 && our_top.len() >= 10`) states that this path compares the
  **unrestricted** recorded top ten with our top ten while clause (a) compares
  inventory-restricted sets, that it is dead at five documents, and that whoever widens the
  fixture must restrict `recorded_top` too or get a spurious `FAIL`. `CORPUS-RUNNER.md`
  §"Not proved" item 1 repeats it for the operator.
* **(e) `--features xapian` message** — `tests/parity_corpus.rs:573` `fixture_inventory`'s
  `expect` now reads *"…; run this target without the `xapian` feature (that build's stub
  refuses an empty path as an unset XAPIAN_DB_PATH)"*. Verified by running
  `cargo test --offline --features xapian --test parity_corpus`: 6 passed, 3 failed, each
  failing with that message. GN4's own operator-facing text (`XAPIAN_DB_PATH is not
  configured…`) was left verbatim, as `docs/DECISIONS.md` row 18 requires.

## 7 · Validation actually run

All commands with `PATH=~/.rustup/toolchains/1.88.0-x86_64-unknown-linux-gnu/bin:$PATH`,
cwd `/home/kabui/gn-search`, each wrapped in `timeout 900`.

| command | result |
|---|---|
| `cargo test --offline` | **84 passed, 0 failed**: lib 32, bin 0, `error_status` 4, `grammar_render` **26** (was 24), `hit_roundtrip` 6, `manifest` 7, `parity_corpus` **9** (was 8), doc-tests 0 |
| `cargo test --offline --test parity_corpus -- --nocapture` | 9 passed; report: `54 shapes seen: 5 graded a set-equality/Jaccard comparison, 3 graded an error class, 7 VACUOUS, 39 SKIP; 8 PASS, 0 FAIL`; Q04/Q11/Q12 print `SKIP(phrase-not-expressible…)` |
| `cargo test --offline --features xapian --test parity_corpus` | 6 passed, 3 failed — the fixture target is default-feature only, now with the message from §6(e) |
| `cargo test --offline --features xapian` | 36 passed, **1 failed** — `index::fixture::tests::a_non_empty_index_path_is_refused`, **pre-existing** (`docs/WORKER-BLOCKERS.md` §8.4), untouched: nothing in this pass touches `fixture.rs`, `lib.rs` or `xapian.rs` |
| `cargo clippy --offline --all-targets` | no new lints: the warnings are all on pre-existing patterns — `src/error.rs:86-87`, three `.map_or` hints and one `match` hint in `src/grammar.rs`, the `very complex type` hint on `parse_range`'s UNCHANGED return type, `tests/manifest.rs:26/31`, and `uninlined_format_args` on the untouched exit-rule `println!` (`tests/parity_corpus.rs:810`). Nothing flags `quoted_tokens`, `text_words`, the `phrase` field or the new tests. |
| `rustfmt --check --edition 2021 src/*.rs src/index/*.rs src/bin/*.rs tests/*.rs` | clean |

## 8 · Residual risks

1. **The phrase class is still not parity, by construction.** `year:2014` is a phrase at GN4
   and 0 hits there; a backend built off these terms *can* express it (`phrase: true`) but
   the fixture double still ANDs, so the corpus grades nothing in that class. It is skipped by
   name, documented (DECISIONS 35/36), and the verdict says PARTIALLY.
2. **`phrase` grouping is a convention on consecutive terms**, not a nested structure: a
   backend must group runs of `phrase: true` terms. A future `TermKind::Phrase(Vec<..>)` would
   be stricter, but it makes `TermKind` non-`Copy` and rewrites every match in the crate and
   its tests — out of scope for three blockers.
3. **`mean:1.."2"` renders as `VALUE_GE 1` plus the phrase word `2`.** The parser-side split
   for that shape is not recorded in the corpus; the *guard* behaviour it mirrors
   (`native.rs:398-401`) is. Asserted as the crate's behaviour, not as measured GN4 output.
4. **`MIN_GRADED` is still 5** — now a backstop under `MUST_GRADE`, so a sixth gradable shape
   appearing and then degrading is caught by the count, and any of the five degrading is
   caught by name.
5. **`--features xapian` remains red** for `a_non_empty_index_path_is_refused` (pre-existing,
   §8.4 of the blockers report; needs an owner decision, not a drive-by fix).
6. **No `git` in this tree** (`git status` → "not a git repository"), so "nothing staged" is
   trivially true and there is no VCS diff; the change set is the file list below.

## 9 · Files changed

| file | why |
|---|---|
| `src/grammar.rs` | B-A (`quoted_tokens`, `render`, `parse_range`, `free_text_terms`), B-B (`RenderedTerm::phrase`, `text_words`), and the module/`TermKind`/`RenderedTerm` docs that described the old behaviour |
| `tests/grammar_render.rs` | `a_range_looking_token_inside_a_quote_is_not_a_422` (B-A), `a_bare_field_value_is_a_phrase_not_an_and` (B-B) |
| `tests/parity_corpus.rs` | B-C (`MUST_GRADE`, two-layer `evidence_shortfall`, `one_must_grade_shape_turned_into_a_skip_fails_the_run`, the floor test rebuilt on `MUST_GRADE`), B-B runner half (`is_phrase_shape` on the marker, `phrase-not-expressible` skip + `Q55`/`Q56` cases), non-blocking (c) and (e) |
| `docs/DECISIONS.md` | row 36 (the phrase marker), row 37 (per-query quote state); rows 34/35 updated to point at them; row 35's skip name corrected |
| `docs/CORPUS-RUNNER.md` | two-layer exit rule, "five comparisons, two of them two-sided", the `phrase-not-expressible` skip rows, clause-(b) residual, default-feature-only note |
| `docs/WORKER-BLOCKERS.md` | correction banner; B3 FIXED → **PARTIALLY** (§1, §4, §8.3); B4 §5 rewritten to what the code does now; §3 wording; skip names; line refs |
| `docs/WORKER-FINAL.md` | this report |

```acceptance-report
{
  "criteriaSatisfied": [
    {
      "id": "criterion-1",
      "status": "satisfied",
      "evidence": "Only the three §5 blockers plus the three named one-line notes changed: 2 source/test files for B-A/B-B, 1 for B-C, and 3 docs corrected as the blockers require (DECISIONS rows 36/37, CORPUS-RUNNER, WORKER-BLOCKERS verdict column). No dependency, feature, API variant or route change."
    },
    {
      "id": "criterion-2",
      "status": "satisfied",
      "evidence": "Every claimed test was executed with the pinned cargo (84 passed, 0 failed) and each fix's fail-before was executed in a scratch copy with the fix reverted (/tmp/gns_rev_a..d), with the panic text quoted in §5 of docs/WORKER-FINAL.md."
    }
  ],
  "changedFiles": [
    "src/grammar.rs",
    "tests/grammar_render.rs",
    "tests/parity_corpus.rs",
    "docs/DECISIONS.md",
    "docs/CORPUS-RUNNER.md",
    "docs/WORKER-BLOCKERS.md",
    "docs/WORKER-FINAL.md"
  ],
  "testsAddedOrUpdated": [
    "tests/grammar_render.rs:248 a_range_looking_token_inside_a_quote_is_not_a_422 (new, B-A)",
    "tests/grammar_render.rs:179 a_bare_field_value_is_a_phrase_not_an_and (new, B-B)",
    "tests/parity_corpus.rs:993 one_must_grade_shape_turned_into_a_skip_fails_the_run (new, B-C)",
    "tests/parity_corpus.rs:945 a_thinned_run_falls_below_the_evidence_floor (updated: rows built from MUST_GRADE, exercises both layers)",
    "tests/parity_corpus.rs:1079 a_phrase_shape_is_skipped_as_unrepresentable (updated: phrase-not-expressible name, Q55 year:2014 and Q56 mean:5 cases)"
  ],
  "commandsRun": [
    {
      "command": "cargo test --offline",
      "result": "passed",
      "summary": "84 passed, 0 failed (lib 32, error_status 4, grammar_render 26, hit_roundtrip 6, manifest 7, parity_corpus 9)"
    },
    {
      "command": "cargo test --offline --test parity_corpus -- --nocapture",
      "result": "passed",
      "summary": "9 passed; 54 shapes: 5 graded, 3 error-class, 7 VACUOUS, 39 SKIP, 8 PASS, 0 FAIL; Q04/Q11/Q12 SKIP(phrase-not-expressible)"
    },
    {
      "command": "cargo test --offline --features xapian --test parity_corpus",
      "result": "failed",
      "summary": "6 passed, 3 failed by design: the fixture target is default-feature only and now says so in the panic message (verifies non-blocking (e))"
    },
    {
      "command": "cargo test --offline --features xapian",
      "result": "failed",
      "summary": "36 passed, 1 failed: index::fixture::tests::a_non_empty_index_path_is_refused — pre-existing (WORKER-BLOCKERS §8.4), untouched"
    },
    {
      "command": "cargo clippy --offline --all-targets",
      "result": "passed",
      "summary": "no new lints; all warnings are on pre-existing patterns (error.rs, manifest.rs, map_or/match hints in grammar.rs, parse_range's unchanged return type, the exit-rule println!)"
    },
    {
      "command": "rustfmt --check --edition 2021 src/*.rs src/index/*.rs src/bin/*.rs tests/*.rs",
      "result": "passed",
      "summary": "clean"
    },
    {
      "command": "cargo test --offline --test grammar_render (in /tmp/gns_rev_a, B-A reverted)",
      "result": "failed",
      "summary": "25 passed, 1 failed: a_range_looking_token_inside_a_quote_is_not_a_422 -> RangeBound { field: \"mean\", value: \"abc..def\\\"\" }"
    },
    {
      "command": "cargo test --offline (in /tmp/gns_rev_b, phrase marker reverted)",
      "result": "failed",
      "summary": "82 passed, 2 failed: a_bare_field_value_is_a_phrase_not_an_and -> \"year:2014\" is a phrase at GN4, not an AND of words"
    },
    {
      "command": "cargo test --offline --test parity_corpus (in /tmp/gns_rev_c, named layer removed)",
      "result": "failed",
      "summary": "8 passed, 1 failed: one_must_grade_shape_turned_into_a_skip_fails_the_run -> a lost must_grade shape must fail; corpus_report still green"
    },
    {
      "command": "cargo test --offline --test parity_corpus (in /tmp/gns_rev_d, raw-string phrase sniff restored)",
      "result": "failed",
      "summary": "8 passed, 1 failed: Q55 must say why it is skipped, got \"free text is stemmed and scored by the real index…\""
    }
  ],
  "validationOutput": [
    "cargo test --offline: 32 lib + 0 bin + 4 error_status + 26 grammar_render + 6 hit_roundtrip + 7 manifest + 9 parity_corpus = 84 passed, 0 failed",
    "parity report: '54 shapes seen: 5 graded a set-equality/Jaccard comparison, 3 graded an error class, 7 VACUOUS (expected set empty: no evidence), 39 SKIP; 8 PASS, 0 FAIL'",
    "MUST_GRADE verified against the live report: Q15/Q29/Q31/Q36/Q37 are exactly the rows printing 'set-eq | PASS' (5/5/2/2/5 hits)",
    "fail-before, B-A: the query renders: RangeBound { field: \"mean\", value: \"abc..def\\\"\" }",
    "fail-before, B-B: \"year:2014\" is a phrase at GN4, not an AND of words: [RenderedTerm { prefix: \"\", term: \"year\", kind: FreeText, phrase: false }, RenderedTerm { prefix: \"\", term: \"2014\", kind: FreeText, phrase: false }]",
    "fail-before, B-C: a lost must_grade shape must fail (count layer satisfied at 5 graded, corpus_report still passed)",
    "fail-before, phrase skip by name: Q55 must say why it is skipped, got \"free text is stemmed and scored by the real index; the fixture's stand-in is a case-insensitive substring test over five documents …\""
  ],
  "residualRisks": [
    "The phrase class (year:2014 / mean:5 / quoted phrases) is still not parity: it is now expressible (RenderedTerm::phrase) and skipped by name (phrase-not-expressible), but nothing grades it until a real backend exists; the B3 verdict now says PARTIALLY for exactly this reason.",
    "phrase grouping is a convention over consecutive terms rather than a nested PhraseGroup; a TermKind::Phrase(Vec<..>) variant would be stricter but makes TermKind non-Copy and rewrites every match site.",
    "mean:1..\"2\" renders as VALUE_GE 1 plus the phrase word 2: it mirrors GN4's guard (native.rs:398-401) and is asserted as the crate's behaviour, not as measured GN4 output (no recorded shape).",
    "cargo test --features xapian still fails on index::fixture::tests::a_non_empty_index_path_is_refused — pre-existing, needs an owner decision, untouched here."
  ],
  "noStagedFiles": true,
  "diffSummary": "src/grammar.rs: quote state is per query (quoted_tokens + render skips field/operator dispatch inside quotes; parse_range ends the value at the first quote and keeps the quoted tail as phrase content) and RenderedTerm gains phrase to mark the words of a phrase (mean:5 / year:2014 / quoted regions). tests/grammar_render.rs: two new tests. tests/parity_corpus.rs: MUST_GRADE names the five shapes and evidence_shortfall enforces ids + count (new test proves a count-only run goes green while a named shape is lost); the phrase skip is keyed on the marker and renamed phrase-not-expressible; clause-(b) comment and the --features xapian panic message. docs: DECISIONS rows 36/37 (+34/35 pointers), CORPUS-RUNNER two-layer exit rule and two-sided wording, WORKER-BLOCKERS B3 FIXED -> PARTIALLY with the reason.",
  "reviewFindings": [
    "no blockers",
    "note: docs/WORKER-BLOCKERS.md §7/§9 still describe the previous pass's rustc-based run and 24/8 test counts; this pass's counts (26/9, 84 total, cargo) are stated in its correction banner and in docs/WORKER-FINAL.md rather than rewriting that report's own history."
  ],
  "manualNotes": "cargo works in this tree with the pinned toolchain and offline registry, so the next lane does not need the rlib/rustc workaround. docs/WORKER-CRATE.md's stale line-count table (REVIEW-BLOCKERS §1.5/§5(d)) was deliberately left alone: it is outside the three blockers and the three named one-line notes."
}
```
