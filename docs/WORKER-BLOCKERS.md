# WORKER-BLOCKERS — the five remaining blockers from `docs/REVIEW-FIXES.md` §d

Scope: exactly the five blockers in `docs/REVIEW-FIXES.md` §d, nothing else. No `cargo`
(`BASELINE.md` §1: the `rustc` on `PATH` belongs to a toolchain that does not provide one).
No network. Nothing written inside `/home/kabui/project5/genenetwork4` — that tree was read
only (`src/search/native.rs`, `docs/search-engines.md`, `test/data/parity-fixtures/Q*.json`).

Every "fails before" claim below was **executed**, not argued: the pinned toolchain
(`~/.rustup/toolchains/1.88.0-…/bin/rustc`) was used to build this tree against the
`serde`/`serde_json` rlibs already in GN4's `target/debug/deps`, a scratch copy had the
fixes reverted, and the same tests were run against it (§7). Results: **32 lib unit tests,
24 in `tests/grammar_render.rs`, 6 in `tests/hit_roundtrip.rs`, 4 in `tests/error_status.rs`,
7 in `tests/manifest.rs`, 8 in `tests/parity_corpus.rs` — all passing.**

> **Corrected after `docs/REVIEW-BLOCKERS.md`.** This report is the pass that review
> examined, and two of its verdicts did not survive it. **B3 is PARTIALLY, not FIXED** (§4):
> the label was taken out of the value term, which is what the finding named, but
> `year:2014` still renders as an AND of two words where GN4 builds a phrase — so the
> rendered query is not the query GN4 builds, which is what `docs/DECISIONS.md` row 35
> itself says is "a different query that would match". **B4 was narrowed, not fixed** (§5):
> the quote test was per token (`value.starts_with('"')`) while GN4's is per query
> (`native.rs:376-385`, `:399`), so `"alcohol mean:abc..def"` was still a 422 here and a 200
> at GN4. The follow-up pass fixed both and reports in `docs/WORKER-FINAL.md`; the verdict
> column below, §3's evidence sentence, §4 and §5 are corrected to what the code does now,
> and §4/§5 name which half of each finding was still open when this report was written.
> The test counts above are this pass's; the tree is now 32 / 26 / 6 / 4 / 7 / 9.

---

## 1 · Summary

| # | Finding | Verdict | Code | Fail-before test |
|---|---|---|---|---|
| B1 | `alcohol AND NOT dependence` is a spurious 422 | **FIXED** | `src/grammar.rs:733` | `tests/grammar_render.rs:455` `a_not_after_a_binary_operator_is_not_dangling` |
| B2 | the corpus runner is green with no evidence | **FIXED** | `tests/parity_corpus.rs:70` (`MIN_GRADED`), `:85` (`MUST_GRADE`), `:537` (`evidence_shortfall`), `:385` (`is_phrase_shape`) | `tests/parity_corpus.rs:945` `a_thinned_run_falls_below_the_evidence_floor`, `:993` `one_must_grade_shape_turned_into_a_skip_fails_the_run`, `:1048` `an_empty_expected_set_is_vacuous_not_graded`, `:1079` `a_phrase_shape_is_skipped_as_unrepresentable`; shell demo in §3 |
| B3 | the free-text fallback keeps `field:` | **PARTIALLY** — the label is out of the value term; the words are still not the PHRASE GN4 builds (§4) | `src/grammar.rs:549` (`free_text_terms`), `:838` (`text_words`), `:328` (`RenderedTerm::phrase`), `:497` (`is_registered_field`) | `tests/grammar_render.rs:122` `a_bare_numeric_value_renders_as_its_words`, `:179` `a_bare_field_value_is_a_phrase_not_an_and`, `:310` `an_empty_field_value_is_the_label_word` |
| B4 | `mean:"1..2"` is a 422 instead of a phrase | **FIXED** (per-query quote state, in the follow-up; §5) | `src/grammar.rs:806` (`quoted_tokens`), `:861` (`parse_range`), `:575` (`render`) | `tests/grammar_render.rs:210` `a_quoted_value_is_never_a_range`, `:248` `a_range_looking_token_inside_a_quote_is_not_a_422` |
| B5 | delivered report + dev-server doc contradict the code | **FIXED** | `docs/WORKER-CRATE.md:23,26,27,30,103`; `src/bin/gn-search.rs:21-28` | documentation only — see §6 for why no test can fail here |

The findings, quoted from `docs/REVIEW-FIXES.md` §d so a reader can see what each one was:

> 1. **`src/grammar.rs:679`** — `alcohol AND NOT dependence` (and `a AND (NOT b)`) is a 422:
>    `Some(previous) => is_operator(previous) && !exempt(previous)` treats *any* operator
>    followed by `NOT` as dangling. xapian accepts the form; F6 replaced a missing 422 with a
>    spurious one. Smallest fix: `&& !exempt(term)` added to that arm, plus a test case.
> 2. **`tests/parity_corpus.rs:486` + `:564-587`** — the runner can be green with no graded
>    evidence: the expected set is inventory-restricted (7 shapes vacuous, 3 all-five), and
>    neither `passed` nor `shapes.len()` is asserted, so a thinned corpus and an all-SKIP run
>    both exit 0.
> 3. **`src/grammar.rs:589-593`** — the free-text fallback keeps `field:`; `position:` and
>    `year:2014` are the two classes the F2/F3 fixes claim, and neither is rendered the way
>    GN4's parser renders it (`native.rs:723-724`). Nothing in the corpus can see it.
> 4. **`src/grammar.rs:549` + `:700-717`** — `mean:"1..2"` is a 422 in the crate and phrase
>    content at GN4 (`native.rs:380-385`, `:398-401`): the `..` scan is not quote-aware.
>    MINOR in blast radius, but it is a false rejection, the one direction a parity claim
>    cannot afford.
> 5. **`docs/WORKER-CRATE.md:23,26,27` and `src/bin/gn-search.rs:280` vs `:22-24`** — the
>    crate's delivered report still documents `serde_json::Map`, `page_bounds` and a fixture
>    that "honours `limit`" (three of the seven fixes contradict it), and the dev server
>    answers F1's refusal with 500 `internal_error` where it promises 503
>    `index_unavailable`. Documentation-level, but it is the file the next reader trusts.

---

## 2 · B1 — `alcohol AND NOT dependence` must not be a 422

**FIXED.** `src/grammar.rs:722-740` (`check_operators`); the changed line is `:733`:

```rust
Some(previous) => is_operator(previous) && !exempt(previous) && !exempt(term),
```

`NOT` is a prefix operator, so it closes the gap its binary operator opened. The boundary is
kept: `alcohol AND NOT`, `AND alcohol`, `alcohol AND OR dependence` and `a AND (NOT` are
still `SyntaxFault::DanglingOperator`, and Q51 (`alcohol AND`) still produces
`"Syntax: <expression> AND <expression>"` with class `QueryParserError` — the recorded
diagnostic, unchanged (`src/grammar.rs:406-419`).

Test: `tests/grammar_render.rs:455` `a_not_after_a_binary_operator_is_not_dangling`
(covers `alcohol AND NOT dependence`, `a AND (NOT b)`, `alcohol OR NOT dependence`,
`NOT alcohol`; asserts 4 rendered terms with `OPERATOR AND` then `OPERATOR NOT`, and that
`gn_search::search` answers 200). The three genuinely-dangling forms were added to
`tests/grammar_render.rs:414` `a_dangling_operator_is_a_syntax_error`.

**Assertion that would have caught the old bug** (reverted code, executed):
`tests/grammar_render.rs:451` `"alcohol AND NOT dependence" must render, got
Syntax { fault: DanglingOperator }`.

---

## 3 · B2 — the runner must not be green with no evidence

**FIXED.** `tests/parity_corpus.rs`, `docs/CORPUS-RUNNER.md`.

What the runner does now, per requirement:

* **(a) counts graded shapes and prints them** — `summarize` (`:478`) fills `Counts`
  (`:463-476`), `corpus_report` (`:699`) prints them in one auditable line.
* **(b) fails below a stated floor** — `const MIN_GRADED: usize = 5` (`:67`), enforced by
  `evidence_shortfall` (`:505`) and asserted in `corpus_report` (`:759-761`). The floor is
  named and justified in `docs/CORPUS-RUNNER.md:62-71` ("Raise it as the fixture grows; never
  lower it to make a run pass").
* **(c) `VACUOUS` is its own verdict** — `Verdict::Vacuous` (`:423`) / `Graded::Vacuous`
  (`:386`), set at `:651-665` only when the expected set is empty **and** our side is empty
  (a non-empty `extra` is still a `FAIL`). `Row::is_graded_evidence` (`:455`) excludes it,
  so `VACUOUS` is never counted as graded and never rendered as `PASS`.
* **(d) the three audit numbers + the exit rule** — printed at `:731-749`.

Live output on the committed corpus (executed):

```
54 shapes seen: 5 graded a set-equality/Jaccard comparison, 3 graded an error class,
7 VACUOUS (expected set empty: no evidence), 39 SKIP (the missing xapian backend's, see
docs/CORPUS-RUNNER.md); 8 PASS, 0 FAIL
exit rule: any FAIL fails the test, and so does fewer than MIN_GRADED = 5 graded
comparisons; PASS, VACUOUS and SKIP alone are not evidence
```

**The floor is 5, not the review's illustrative "40 of 54", and that is arithmetic, not a
relaxation:** only 12 shapes reach a set-equality comparison at all, and 7 of those are
`VACUOUS`, so exactly 5 (Q15, Q29, Q31, Q36, Q37) compare a non-empty expected set. That is
**five comparisons, two of them two-sided** — Q31 (`peak:5..15`) and Q36
(`additive:0.5..1.0`) expect 2 of the fixture's 5 documents and can fail in either direction,
while Q15, Q29 and Q37 expect the whole inventory and can only catch a renderer that drops
hits. A floor of 40 would fail every honest run forever. 5 is the count of real evidence, so
losing any single one of them now fails the run — which is what "a thinned corpus must not be
green" means. Named and justified in `docs/CORPUS-RUNNER.md:62-71`, and named *as a set* by
`MUST_GRADE` (`tests/parity_corpus.rs:85`), because a count is not a set: with a sixth
gradable shape in the corpus, one of these five could degrade to `SKIP` without the count
moving (`REVIEW-BLOCKERS.md` §5 blocker 3).

**Tests that would have caught the old bug:**

* `tests/parity_corpus.rs:945` `a_thinned_run_falls_below_the_evidence_floor` — at
  `MIN_GRADED` comparisons the run is evidence; one fewer is not; an all-SKIP run of 54
  shapes is not (`counts.seen == 54`, `counts.graded == 0`, shortfall `Some`).
* `tests/parity_corpus.rs:1048` `an_empty_expected_set_is_vacuous_not_graded` — a `group:B6`
  shape whose three recorded documents are outside the fixture grades as `VACUOUS`, not
  `PASS`, and `counts.graded == 0`.
* `tests/parity_corpus.rs:1079` `a_phrase_shape_is_skipped_as_unrepresentable` — Q04/Q11/Q12
  and the bare-value phrase class (`year:2014`, `mean:5`) print
  `SKIP(phrase-not-expressible…)`, never `PASS`.
* `tests/parity_corpus.rs:993` `one_must_grade_shape_turned_into_a_skip_fails_the_run` — the
  count is satisfied (five graded) and the run still fails because one of the five named
  shapes became a `SKIP` (`REVIEW-BLOCKERS.md` §5 blocker 3).

Executed shell demonstration (not the deliverable, the same behaviour end to end). One
graded shape deleted from a copy of the corpus:

```
$ GN4_PARITY_FIXTURES=/tmp/thinned ./bin_parity_corpus corpus_report   # Q31.json removed
53 shapes seen: 4 graded a set-equality/Jaccard comparison, … 7 PASS, 0 FAIL
thread 'corpus_report' panicked: 4 of 53 shapes graded a set-equality or Jaccard
comparison, below the evidence floor MIN_GRADED = 5 …
process exit code: 101
```

and the same corpus with a permissive floor (i.e. the old code's behaviour, which asserted
only `failed.is_empty()`):

```
exit code with a floor of 1 (the old no-floor behaviour): 0
```

---

## 4 · B3 — the free-text fallback must render the word(s), not `field:value`

**PARTIALLY** — and this is the corrected verdict. Two halves, and only the first was
closed by `option B (word-split)`: the label is no longer inside the value's term (which is
what the finding named), and the two words travel as separate terms — but at GN4 those two
words are a PHRASE, and an AND of two words is a different query. `docs/DECISIONS.md` row 35
says so in the same pass's own words ("an AND-of-words is a different query that would
match"), and the arithmetic is: `year:2014` → a `year 2014` phrase at GN4 → **0** hits on a
positionless index (Q04/Q11/Q12), while `year` AND `2014` matches whatever both words hit.
That is the false-positive direction, and nothing in this crate can see it. The `option B`
reasoning, as it must be preserved:

> Q53 `position:` returned **576 documents**, which can only be hits on the word `position`
> — dropping the label would leave an empty query (no value at all), which cannot produce
> 576; and GN4 measures `mean:5` as a `mean 5` phrase (`native.rs:722-724`), so both words
> are in the query xapian builds. GN4's index carries no positions, so the phrase variant
> matches nothing (`genenetwork4/docs/search-engines.md:98-102`; Q04/Q11/Q12 recorded 0
> hits). **The decision came from recorded hits, not from taste.**

Implemented at `src/grammar.rs:497` (`is_registered_field`), `:549` (`free_text_terms`),
`:575` (the fallback call), `:838` (`text_words`):

| input | renders as |
|---|---|
| `year:2014` | `FreeText("year")`, `FreeText("2014")`, both `phrase: true` |
| `mean:5` | `FreeText("mean")`, `FreeText("5")`, both `phrase: true` |
| `position:` | `FreeText("position")` (label alone, `phrase: false`: one word is not a phrase group) |
| `mean:"1..2"` | `FreeText("mean")`, `FreeText("1..2")`, both `phrase: true` |
| `foo:bar` (Q50, unregistered) | `FreeText("foo:bar")` — one term, unchanged |

The value's term never contains the label; the label is its own term. Quote marks are phrase
delimiters and are trimmed from each word. Decisions recorded as `docs/DECISIONS.md` row 34
(supersedes row 9's "free text" phrasing; rows 9 and 30 now point at it), row 35 (see
below) and row 36 (the `phrase` marker the follow-up pass added, which is what makes the
remaining half expressible to a real backend).

Tests (both rewritten from the ones that pinned the OLD `token as written` behaviour, which
is why they fail before and pass after):

* `tests/grammar_render.rs:122` `a_bare_numeric_value_renders_as_its_words`
* `tests/grammar_render.rs:179` `a_bare_field_value_is_a_phrase_not_an_and` (the half this
  report left open — fail-before: `"year:2014" is a phrase at GN4, not an AND of words:
  [RenderedTerm { …, phrase: false }, RenderedTerm { …, phrase: false }]`)
* `tests/grammar_render.rs:310` `an_empty_field_value_is_the_label_word`
* `tests/grammar_render.rs:152` `a_non_numeric_value_without_a_range_is_not_a_422`
  (same word-split, still a 200)
* `foo:bar` staying one term is pinned by the pre-existing
  `tests/grammar_render.rs:607` `free_text_operators_and_unknown_prefixes_pass_through`.

**Assertions that would have caught the old bug** (reverted code, executed):
`"year:2014" is two words, not a range: [RenderedTerm { prefix: "", term: "year:2014",
kind: FreeText }]` (1 ≠ 2) and `"position:" keeps the label as the word: left "position:",
right "position"`.

**Phrase shapes are skipped BY NAME, never passed.** `tests/parity_corpus.rs:385`
`is_phrase_shape` sends any `phrase`-marked term — a quoted phrase, and now also the
bare-value class (`year:2014`, `mean:5`) — or a `NEAR`/`ADJ` operator to
`SKIP(phrase-not-expressible…)` (`:290-298`) before any other blocker, so Q04/Q11/Q12 and
that class are never `PASS`. `docs/DECISIONS.md` rows 35 and 36 record the reason and the
consequence: a term list cannot be parity for a phrase, so the real backend must defer to
xapian's own parser for them. `docs/CORPUS-RUNNER.md` lists both rows in the skip table.
Keying the skip on the marker rather than on the raw string is what makes the class
visible: with the old `raw_query.contains('"')` test, `year:2014` fell through to the
generic free-text skip (`tests/parity_corpus.rs:1079`, fail-before: `Q55 must say why it
is skipped, got "free text is stemmed and scored by the real index…"`).

---

## 5 · B4 — a quoted value is never a range

**FIXED — but only fully in the follow-up pass.** What this pass did
(`value.starts_with('"')` → `Ok(None)`) made the quote test PER TOKEN, while GN4's is PER
QUERY (`native.rs:376-385` toggles `in_quotes` while walking the whole string; `:399` stops a
field value at a quote). So `mean:"1..2"` was fixed and `"alcohol mean:abc..def"` — a quoted
phrase containing a range-looking token — was still a 422 here and a 200 at GN4, and
`mean:1.."2"` was a 422 where GN4 parses the one-sided range `1..`. That is the same class
the finding named ("the `..` scan is not quote-aware"), so the verdict for this pass is
"narrowed".

Fixed for real in the follow-up at `src/grammar.rs:806` (`quoted_tokens`, the query-level
state machine `render` now walks) and `:861` (`parse_range`, where the value ends at the
first quote). The original test, still passing: `tests/grammar_render.rs:210`
`a_quoted_value_is_never_a_range` — `mean:"1..2"` is two free-text terms (`mean`, `1..2`),
is searchable (no 422), and the unquoted `mean:1..2` is still `ValueRange { slot: 0, begin:
Some(1.0), end: Some(2.0) }`. The recorded Q52 (`mean:abc..def` → `RangeBound`, class
`QueryParserError`) is untouched, pinned by `tests/grammar_render.rs:409`
`bad_numeric_bound_is_an_invalid_query`.

The per-token residue's test: `tests/grammar_render.rs:248`
`a_range_looking_token_inside_a_quote_is_not_a_422` — `"alcohol mean:abc..def"` renders two
phrase words and is searchable, `mean:1.."2"` renders the one-sided range plus the phrase
word `2` and is searchable, and the unquoted `mean:abc..def` is still `RangeBound`.

**Assertions that would have caught the old bug.** This pass's own revert (executed, §7) put
back `value.starts_with('"')` and the token's-own-first-character quote test; the new test
then fails at its first line with `the query renders: RangeBound { field: "mean", value:
"abc..def\"" }` — the residue this pass left. The assertion for `mean:"1..2"`,
`the query renders: RangeBound { field: "mean", value: "\"1..2\"" }`, is this report's
original fail-before run from before the per-token fix.

---

## 6 · B5 — the delivered report and the dev-server doc must match the code

**FIXED (documentation).** Kept `Internal` → **500** `internal_error`:
`docs/DECISIONS.md` row 32 already decided it ("`Internal` is 500 on GN4's existing table, so
no variant and no status is invented"), so the doc is what was wrong, not the code. No
variant, no status and no behaviour were added.

* `src/bin/gn-search.rs:21-28` — the module doc now separates the two classes instead of
  promising 503 for both: `Unreachable` (an index this build cannot open) → **503
  `index_unavailable`** (row 28); `Internal` (no backend for a configured path — the F1
  refusal — or `XAPIAN_DB_PATH` unset) → **500 `internal_error`** naming the fix (row 32).
  Either way it is never an empty array.
* `docs/WORKER-CRATE.md:23` — `extra: serde_json::Map<…>` → `extra:
  BTreeMap<String, serde_json::Value>`, GN4's own type, with the row-19 reason.
* `docs/WORKER-CRATE.md:26` — `page_bounds` (deleted by F5) → `offset_for_page`, plus the
  full-match-set contract and a pointer to row 33.
* `docs/WORKER-CRATE.md:27` — "honouring `XT{type}`, `limit`, `page`" → "honouring
  `XT{type}` and the page's offset — `limit` places the offset and is NOT a cap on the result
  (row 33)".
* `docs/WORKER-CRATE.md:30` — the status list now includes 500 `internal_error`.
* `docs/WORKER-CRATE.md:103` — §4.1, the "least sure" item that defended
  `serde_json::Map`, is marked RESOLVED with what the code now does (the same finding in a
  second place, which the review's line list did not reach).

**No test can fail for this one, and I am saying so rather than claiming coverage:** these
are prose corrections in two files and no executable surface reads them. The behaviour they
describe is already pinned elsewhere — `tests/error_status.rs` (the exhaustive 422/502/502/502/500
table) and the F5 contract tests in `src/index/fixture.rs`.

---

## 7 · Validation actually run

No `cargo` (`BASELINE.md` §1). The tree was copied to `/tmp/gns` and built with
`~/.rustup/toolchains/1.88.0-x86_64-unknown-linux-gnu/bin/rustc`, `--extern
serde=<GN4 deps>/libserde-914cf62a9f9cb3e1.rlib --extern
serde_json=<GN4 deps>/libserde_json-82fe5eed9538ec03.rlib -L dependency=<GN4 deps>`.

| build | result |
|---|---|
| `rustc --test src/lib.rs` (default features) | **32 passed**, 0 failed |
| `tests/grammar_render.rs` | **24 passed** (was 22 before this pass) |
| `tests/hit_roundtrip.rs` | **6 passed** |
| `tests/error_status.rs` | **4 passed** |
| `tests/manifest.rs` | **7 passed** |
| `tests/parity_corpus.rs` | **8 passed** (was 5), report as quoted in §3 |
| `rustc src/bin/gn-search.rs` (default, and `--cfg 'feature="serve"'`) | compiles; default prints the "no `serve` feature" message and exits 2 |
| `rustfmt --check` (pinned 1.88.0) on the four files touched | clean |
| `rustc --test src/lib.rs --cfg 'feature="xapian"'` | **36 passed, 1 FAILED** — pre-existing, see §8 |

Fail-before runs: a scratch copy (`/tmp/gns_rev`) with the B1, B3 and B4 source changes
reverted (and, for B2, the floor set to 1 to reproduce the old "no floor at all") was built
and run against the **new** tests; the failures quoted above are its output. `--features
xapian` counts as "not run" for the five blockers: none of them touches the stub.

---

## 8 · Residual risks, stated rather than hidden

1. **`MIN_GRADED = 5` is exactly today's evidence count.** It is a lower bound on the
   committed corpus (fixed at 54 files), so any further legitimate widening of the fixture
   must raise it by hand; conversely no run can quietly drop below it. That is the intended
   brittleness, and `docs/CORPUS-RUNNER.md` says to raise, never lower. **Corrected after
   `REVIEW-BLOCKERS.md`:** a count is not a set, so the five are now named in `MUST_GRADE`
   and each must be present, graded and `PASS` — the follow-up's fix for §5 blocker 3.
2. **Clause (b) (top-10 Jaccard) still never fires** on a five-document fixture, and the
   `our_top.len() >= 10` gate is still an invention relative to GN4 (which gates on the
   recorded count alone). The floor does not change that; it only stops the runner claiming
   more than it graded. Out of the five blockers' scope, so untouched.
3. **B3 cannot be parity for phrases — and that is why the verdict is PARTIALLY.**
   `year:2014` at GN4 is a two-word phrase, which this index answers with **0** documents
   (no positions); the crate emits the two words as separate terms. The follow-up marks them
   `phrase` (`docs/DECISIONS.md` row 36) so a real backend can hand them to xapian's parser
   as one phrase, and the runner skips the class by name (`phrase-not-expressible`), but the
   fixture double still cannot answer it: nothing in this crate grades the phrase class, and
   nothing can until a real backend exists.
4. **`index::fixture::tests::a_non_empty_index_path_is_refused` fails under
   `--cfg 'feature="xapian"'`** (`src/index/fixture.rs:376`): the second half of the test
   goes through `crate::search`, which selects the xapian stub in that build and answers
   `Unreachable`, not the `Internal` the test asserts. **Pre-existing** (nothing in this pass
   touches `fixture.rs`, `lib.rs` or `xapian.rs`) and out of the five blockers' scope, so it
   was not fixed — but it also falsifies `docs/WORKER-CRATE.md` §3's "32 unit tests with
   `--features xapian`, all passing", which is part of the same "the next reader trusts it"
   problem. It needs an owner decision (gate the second half on `#[cfg(not(feature =
   "xapian"))]`, or assert per backend).
5. **`README.md`'s expected test counts are stale** (it says 27 unit tests and 14 in
   `tests/grammar_render.rs`; the tree is now 32 and 24 — the 32 predates this pass, the 24
   does not). Left untouched to keep the diff to the five blockers; `docs/WORKER-CRATE.md`
   §4.3 also still carries the pre-F2 reading of `mean:5` (`point range 5..5`) and
   `mean:` (a 422), which this pass' B3/B4 work makes obsolete.
6. **No `git` in this tree** (`git status` → "not a git repository"), so "no staged files"
   is trivially true and there is no VCS diff to show; the change set is the file list in
   §9.
8. **The corpus runner reads GN4's tree by absolute path.** `DEFAULT_CORPUS` is
   `/home/kabui/project5/genenetwork4/test/data/parity-fixtures`; nothing was copied into
   this crate (the thinned demo copied out to `/tmp/thinned`), so the runner still needs that
   tree or `GN4_PARITY_FIXTURES`.
7. **Everything above is static-plus-rustc evidence**, not `cargo test`: no lockfile
   resolution, no doctests, no clippy, and no real xapian index anywhere.

---

## 9 · Files changed

| file | why |
|---|---|
| `src/grammar.rs` | B1 (`check_operators`), B3 (`is_registered_field`, `free_text_terms`, the fallback call), B4 (the quote guard in `parse_range`), plus the module/`TermKind`/`RenderedTerm` docs that described the old token-verbatim behaviour |
| `tests/grammar_render.rs` | the two rewritten fail-before tests, B1's positive test, B4's test, and the two extra genuinely-dangling forms |
| `tests/parity_corpus.rs` | B2: `MIN_GRADED`, `Graded`, `Verdict::Vacuous`, `Counts`/`summarize`/`evidence_shortfall`, the phrase blocker, the summary + exit rule, and three unit tests |
| `docs/CORPUS-RUNNER.md` | names `MIN_GRADED`, documents `VACUOUS`, the audit numbers, the exit rule and the thinned-corpus evidence; baseline corrected |
| `docs/DECISIONS.md` | rows 34 (word-split, from Q53's 576 recorded hits) and 35 (phrases unrepresentable at term level); rows 9, 30, 31 point at / state the new reading |
| `docs/WORKER-CRATE.md` | B5: rows 23, 26, 27, 30 and §4.1 corrected to what the code does |
| `src/bin/gn-search.rs` | B5: the module doc separates 503 `index_unavailable` (`Unreachable`) from 500 `internal_error` (`Internal`) |
| `docs/WORKER-BLOCKERS.md` | this report |
