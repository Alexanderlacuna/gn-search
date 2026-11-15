# REVIEW-BLOCKERS — cross-family review of the blocker pass

Reviewer: `reviewer-alt` (non-GLM family, independent of the author of the pass under
review). Scope: `src/grammar.rs`, `tests/grammar_render.rs`, `tests/parity_corpus.rs`,
`docs/CORPUS-RUNNER.md`, `docs/WORKER-BLOCKERS.md`, judged against
`/home/kabui/project5/genenetwork4` (`src/search/native.rs`, `tests/search_parity.rs`) read
only.

No `cargo`, no writes anywhere, nothing executed inside the GN4 tree. Every number below was
recomputed from the corpus JSON and `src/index/fixture.rs` by hand/scripted read, not taken
from the worker's report. Where my count agrees with `docs/WORKER-BLOCKERS.md` I say so,
because that is itself evidence.

**Verdict: PASS-WITH-FIXES.** Three blockers in §5. The pass is real work — the false-green
mechanism is genuinely closed and the B1/B4 false 422s are genuinely gone for the named
inputs — but B3 and B4 are marked FIXED when each still has a reachable instance of the same
bug class, and the evidence floor protects a *count* rather than the five shapes that carry
the count.

---

## 1 · B1 … B5

| # | Verdict | Proving line | Why |
|---|---|---|---|
| B1 | **FIXED** (one input; class residual) | `src/grammar.rs:733` | `!exempt(term)` added to the operator-adjacency arm; `alcohol AND NOT dependence`, `a AND (NOT b)`, `alcohol OR NOT dependence` render; `alcohol AND NOT`, `AND alcohol`, `a AND OR b`, `a AND (NOT` still 422. |
| B2 | **FIXED** | `tests/parity_corpus.rs:505-517`, `:750-762`, `:455-457`, `:478-495` | The run cannot be green on zero graded comparisons. See §2 — verified by reading the whole exit path, not the report's prose. |
| B3 | **PARTIALLY** | `src/grammar.rs:497,521-536,643` | The label is out of the value term (the reported bug). The result is an **AND of two words where GN4 builds a phrase** — the crate's own `docs/DECISIONS.md` row 35 says that is "a different query that would match". See §1.3. |
| B4 | **PARTIALLY** | `src/grammar.rs:762` | `mean:"1..2"` is fixed. The class is not: the quote test is per-*value* (`value.starts_with('"')`) while GN4's is per-*query* (`in_quotes` state machine). `"alcohol mean:abc..def"` is still a 422 in the crate and a 200 at GN4. See §1.4 — this is the one I would call a blocker. |
| B5 | **FIXED** (doc-only) | `src/bin/gn-search.rs:23-29`, `:283`, `:286`; `docs/WORKER-CRATE.md:23,26,27,30` | The four named contradictions are gone and the code matches the doc (`Unreachable`→503, `Internal`→500). Two stale spots remain in the same file (§1.5) but they are not the reported finding. |

### 1.1 · B1 — GN4 quoted against the crate

The crate now has a hand-written operator-adjacency table (`src/grammar.rs:722-743`):

```rust
Some(previous) => is_operator(previous) && !exempt(previous) && !exempt(term),
```

GN4 has no such table. Its syntax verdict is xapian's, produced by handing the raw string to
the parser (`src/search/native.rs:450-457`):

```rust
    // 1. Parse first (search.py:291 parses before :296 opens the
    //    index), through the checked-parse API: a malformed query is a
    //    typed error, not a panic (the fork's parse_query_checked).
    let parsed = {
        let mut parser = parser().lock().expect("native parser mutex poisoned");
        parser
            .0
            .parse_query_checked::<&str>(q, None, None)
```

and GN4's only *evidence* that a trailing `AND` is rejected is the xapian error it receives
(`src/search/native.rs:582-586`):

```rust
        let err = qp
            .parse_query_checked::<&str>("alcohol AND", None, None)
            .expect_err("alcohol AND must be rejected");
        assert_eq!(err.error_type, "QueryParserError");
        assert_eq!(err.message, "Syntax: <expression> AND <expression>");
```

So B1 is fixed *for the named input*, in the right direction, and the fix is minimal. But the
structure that produced the bug is untouched: a four-branch re-implementation of xapian's
grammar with exactly one recorded shape (Q51) as evidence. `NOT` was the form that happened
to be reported; `XOR`/`NEAR`/`ADJ` arity is unmodelled, and anything xapian accepts that this
table does not is a spurious 422 of the same class. That is an acceptable residual *if it is
written down*; `docs/WORKER-BLOCKERS.md` §2 does not write it down. The new test
(`tests/grammar_render.rs:455`) is a good one — it pins both the accept and the still-422
list.

### 1.2 · B2 — see §2 (the whole exit path).

### 1.3 · B3 — GN4 quoted against the crate, and the crate against itself

The crate's rendering rule (`src/grammar.rs:521-536`, applied at `:643`) produces, for
`year:2014`, `FreeText("year")` + `FreeText("2014")` — two independent free-text terms, which
every downstream backend will AND.

GN4 builds a **phrase** for the same token, and the crate's own decision record quotes it
(`docs/DECISIONS.md` row 34, citing `native.rs:722-724`):

> GN4 measures `mean:5` as a `mean 5` phrase (`native.rs:722-724`)

and GN4's assertion is at `src/search/native.rs:721-724`:

```rust
        // A non-range use of the prefix is plain text (gn3 measures the
        // same: `mean:5` is a `mean 5` phrase, never a range).
        let mean5 = description(&mut qp, "mean:5");
        assert!(!mean5.contains("VALUE"), "mean:5 is text, got {mean5:?}");
```

The crate then contradicts itself. `docs/DECISIONS.md` row 35, written in the same pass:

> A term list cannot say "adjacent", and an AND-of-words is a different query that would
> match; the real backend must hand phrases to xapian's own parser, so a term-level renderer
> cannot be parity for them.

`year:2014` is exactly that AND-of-words. And the corpus says what a phrase answers on GN4's
index: Q04 (`"alcohol dependence"`), Q11 (`alcohol NEAR dependence`), Q12
(`alcohol ADJ dependence`) each recorded **0 hits** — because the index stores no positions
(`genenetwork4/docs/search-engines.md:98-102`). So the arithmetic is: at GN4 `year:2014` → 0
hits; in this crate plus any real backend → whatever "year" AND "2014" matches, which is not
0. **False-positive results, the direction a search API is judged on.**

Nothing in this crate can see it: every free-text shape is `SKIP`ped
(`tests/parity_corpus.rs:294-299`), there is no `year:2014`-class shape in the 54, and
`tests/grammar_render.rs:122` pins the divergent behaviour deliberately. Marking B3 FIXED is
therefore an overclaim: what was fixed is "the value term no longer contains the label". What
`docs/REVIEW-FIXES.md` §d(3) actually alleged was "neither is rendered the way GN4's parser
renders it" — still true for the value-present class.

Smallest honest fix (two options, both cheap):
* *bookkeeping*: verdict → PARTIALLY in `WORKER-BLOCKERS.md` §1/§4, with `year:2014` named as
  a known non-parity class in the same paragraph as the phrase skip; or
* *code*: add `TermKind::Phrase(Vec<word>)` (or a `phrase: bool` on `RenderedTerm`) so the
  seam can say "adjacent", and make the real backend hand the words to xapian's parser. That
  is the only shape of the data that can ever be parity, and it is the same object row 35
  already asks for.

The B3 *empty-value* half (`position:` → the word `position`) is right and is genuinely
decided by Q53's 576 recorded hits — but note the runner `SKIP`s Q53 as free text, so the
single piece of evidence the decision rests on is never re-checked by the runner (see §4.3).

### 1.4 · B4 — GN4 quoted against the crate (this one is still live)

Crate (`src/grammar.rs:755-767`):

```rust
fn parse_range(
    field: &str,
    value: &str,
) -> Result<Option<(Option<f64>, Option<f64>)>, RenderError> {
    // A quoted value is phrase content and never reaches a range processor,
    // so it can be neither a range nor the Q52 "Unknown range operation"
    // error (`native.rs:380-385`, `:398-401`).
    if value.starts_with('"') {
        return Ok(None);
    }
```

GN4 (`src/search/native.rs:376-385`, `:399`):

```rust
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
...
                        .split(|c: char| c.is_whitespace() || matches!(c, '(' | ')' | '"'))
```

GN4 decides *per query*: a marker inside a quoted region is never a range, and a value stops
at a quote. The crate decides *per token start*: "does this value begin with `\"`".

Reachable counterexample, traced through both:

* input `"alcohol mean:abc..def"` (one quoted phrase containing a range-looking token)
* crate `render()`: quotes balanced (2) → `split_whitespace` → tokens `"alcohol` and
  `mean:abc..def"` → second token: `split_once(':')` → `("mean", "abc..def\"")`, non-empty →
  `numeric_range("mean")` → `parse_range` → does **not** start with `"` → `split_once("..")` →
  bound `abc` fails → `RenderError::RangeBound` → `InvalidQuery` → **422**.
* GN4: `guard_numeric_ranges` skips everything while `in_quotes` → no error; xapian parses the
  phrase → **200**.

Same class, second form: `mean:1.."2"` → GN4 stops the value at the quote and gets the
one-sided range `1..` (200); the crate parses `"2"` as a float and 422s.

This is precisely the failure mode the blocker was about ("the `..` scan is not quote-aware"),
and it still is not. `tests/grammar_render.rs:177` only pins the case where the quote is at
the value's first byte, so it cannot see it.

Smallest fix: make quote state a property of the query, not the token. In `render()`, walk
`q` once and tag each token with "starts inside quotes"; skip field classification (and thus
`parse_range`) for tokens inside a quoted region. `guard_numeric_ranges` already is that
state machine — the renderer should mirror it rather than re-derive it from the token's first
character. A one-line mitigation that is strictly better than nothing: `if value.contains('"')
{ return Ok(None) }` — but do the state machine.

### 1.5 · B5 — fixed, with stale residue

Verified as claimed: `src/bin/gn-search.rs:23-29` separates 503 `index_unavailable`
(`Unreachable`) from 500 `internal_error` (`Internal`) and `status_of` at `:283`/`:286`
implements exactly that; `docs/WORKER-CRATE.md:23/26/27/30` no longer say
`serde_json::Map`, `page_bounds`, or "honours `limit`". Good.

Residue in the same file the next reader trusts (`docs/WORKER-CRATE.md:19-35`), not the
reported finding but in its category:

* the line-count column is now wrong almost everywhere: `src/grammar.rs` "753" → **1043**,
  `tests/grammar_render.rs` "269" → **562**, `src/index/fixture.rs` "433" → **504**,
  `src/lib.rs` "114" → **123**, `src/bin/gn-search.rs` "384" → **396**, `docs/DECISIONS.md`
  "67 / 29 rows" → **77 / 35 rows**. `src/grammar.rs` "20 unit tests" no longer matches the
  32 the pass itself reports.
* §4.3 still carries the pre-F2 reading of `mean:5` / `mean:` that this pass's own B3/B4 work
  makes obsolete — the worker admits this in `WORKER-BLOCKERS.md` §8.5.

Not a blocker. But "the file the next reader trusts" is only trustworthy again after those
two edits, and they are a five-minute diff.

---

## 2 · THE FALSE-GREEN TEST

**No — the runner cannot exit 0 while grading nothing, nor while grading three shapes.** The
whole exit decision is here (`tests/parity_corpus.rs:750-762`):

```rust
    let failed: Vec<&Row> = rows.iter().filter(|row| row.verdict.is_fail()).collect();
    assert!(failed.is_empty(), ...);
    if let Some(shortfall) = evidence_shortfall(&counts) {
        panic!("{shortfall}");
    }
```

and `evidence_shortfall` (`:505-507`) returns `None` only when `counts.graded >= MIN_GRADED`
(`:67`, `= 5`); `counts.graded` is incremented only at `:491`, only under
`row.is_graded_evidence()` (`:455-457` = `graded.is_match_set() && matches!(verdict, Pass)`),
and `Graded::Vacuous` (`:386`, set at `:651-665` only when expected *and* actual are both
empty — a non-empty `extra` is a `FAIL`) and `Graded::Nothing` (every `SKIP`) both fail
`is_match_set` (`:404-406`), so `VACUOUS`, `SKIP` and the three error-class lines cannot pay
into the floor; a panicking `#[test]` makes the integration-test binary exit non-zero, so
"graded nothing" ⇒ 3 < 5 ⇒ panic, and "three shapes" ⇒ 3 < 5 ⇒ panic. What the floor
*actually* buys, and why it is worth keeping, is one layer deeper than the review asked: a
grammar regression that turns any of the five gradable shapes into free text does not FAIL,
it silently becomes a `SKIP` (the fixture blocker is checked before grading,
`:604-612`/`:238-333`), so without the floor the run would go *greener* while losing the
evidence — the count catching that is a real property, not decoration. It becomes "yes" by
exactly three edits, in increasing order of how likely a future contributor is to make them:
(i) `MIN_GRADED = 5` → `3`; (ii) leaving `MIN_GRADED` alone but counting *rows* rather than
distinct shapes — `load_shapes` (`:129-150`) never dedups by `id` and `corpus_report` never
asserts `counts.seen == 54` or id uniqueness, so two copies of one fixture file (`Q31b.json`
satisfies both the `Q` prefix and `.json`) turn three shapes into five graded rows; or
(iii) running the target with `--skip corpus_report` / `metric_tests`, since the floor lives
inside one `#[test]` and there is no separate gate (a real hazard given `CORPUS-RUNNER.md`
tells the operator to use `--nocapture`, i.e. to invoke this target ad hoc).

---

## 3 · THE HONEST FLOOR

**`MIN_GRADED = 5` is arithmetically correct and is not a number picked to pass. But the
prose justification attached to it overstates it by three shapes.**

I recomputed the corpus from the fixtures, independently of the worker's report. Of the 54
shapes, exactly **12** reach a set-equality comparison at all (`fixture_blocker` returns
`None`): Q15, Q16, Q17, Q19, Q28, Q29, Q30, Q31, Q36, Q37, Q38, Q39. The other 42 split as
3 error-class (Q42, Q51, Q52) + 39 `SKIP` (free text 19, operator 8, non-phenotype 9,
phrase 3, empty-terms 2, slot-3/6 1, probabilistic 5, `tissue`/`chr`/`peakchr`/`author` 4) —
which is exactly the `5 / 3 / 7 / 39` printed in `CORPUS-RUNNER.md:60-61`, so the audit line
is truthful.

The seven `VACUOUS` ones are `VACUOUS` for a checkable reason: I intersected each recorded
`identifiers` set with the fixture's five `(dataset, name)` pairs
(`C57BL-6JxC57BL-6NJF2Publish` × {10199, 10200, 10201, 10210, 10211}) from
`src/index/fixture.rs`. Q16 = 0, Q17 = 0, Q19 = 0, Q28 = 0, Q30 = 0, Q38 = 0, Q39 = 0. The
five non-vacuous ones:

| shape | expected = recorded ∩ inventory | fixture value that decides it | discriminating? |
|---|---|---|---|
| Q15 `species:mouse` | **5** (all five) | `species = "mouse"` on all | **one-sided** |
| Q29 `mean:..15` | **5** (all five) | means 3.02, 6.61, 9.88, 3.42, 7.78 all ≤ 15 | **one-sided** |
| Q31 `peak:5..15` | **2** (10199, 10201) | lrs 6.63, 5.39 in range; 4.23/4.61/3.72 out | **two-sided** |
| Q36 `additive:0.5..1.0` | **2** (10200, 10211) | 0.606, 0.933 in; 0.377, 0.466, −1.10 out | **two-sided** |
| Q37 `year:2010..2020` | **5** (all five) | `year = 2013` on all | **one-sided** |

So: 12 can be *compared*, 5 can be *graded*, **2 (Q31, Q36) can fail in both directions**.
For Q15/Q29/Q37 the expected set equals the entire fixture inventory, so the comparison can
only catch a renderer that *under*-matches; a term that matches everything — the failure a bad
range or a sloppy boolean tends to produce — passes all three. `docs/CORPUS-RUNNER.md:73-75`
and `docs/WORKER-BLOCKERS.md:108` both say the floor is "the number of shapes on the committed
corpus whose comparison can distinguish a correct renderer from a wrong one". True only for
wrong renderers that drop hits. The honest sentence is "five comparisons, two of which are
two-sided".

Is 5 defensible as a floor? Yes — it is the maximum the current fixture can supply, so it is
brittle in the *safe* direction (any single loss fails the run). It is **not** defensible as a
*set*: see blocker 3 in §5. And note the ceiling is a fixture fact, not a corpus fact: Q28
(`mean:10..20`, recorded 1206) and Q30 (`mean:10..`, recorded 8644) are gradable the moment a
sixth fixture document has a mean above 10, which is the intended growth path.

---

## 4 · WHAT STILL LIES

Three places where a `PASS` from this runner still does not mean parity. All three are
invisible because the shapes that would show them are `SKIP`.

**4.1 · The rendering is not the query GN4 builds — named: Q09, Q07, Q08, Q13, Q53.**
Operators render as opaque `TermKind::Operator` entries (`src/grammar.rs:507-513`) and the
fixture *drops* them (`src/index/fixture.rs:171-173`, `TermKind::Operator => true`) while
ANDing every other term. So for Q09 `alcohol NOT dependence` (recorded 1152) a backend built
straight off `ParsedQuery::terms` computes `alcohol AND dependence` (Q07, recorded 45) — the
exact inverse of what was asked. Nothing grades it: all of Q07/Q08/Q09/Q10/Q13/Q43/Q44/Q46
are `SKIP`, so 13 of the corpus's 54 shapes contribute nothing to the boolean-structure
question, and the 5 graded ones contain no operator at all. Same seam, same paragraph:
`Q53 position:` and the `year:2014` class from §1.3 — an AND-of-words where GN4 builds a
phrase that its positionless index answers with 0 hits (recorded Q04/Q11/Q12 = 0).

**4.2 · Three of the five graded PASSes are whole-inventory — named: Q15, Q29, Q37.**
Expected = the entire fixture, so the only way they fail is by returning fewer than all five
documents. A renderer that emits any predicate true of all five passes. The runner cannot
detect over-matching anywhere in the corpus, and the 7 `VACUOUS` shapes are the mirror image
of the same limitation (expected empty ⇒ cannot detect under-matching). Related: the numeric
slot→field mapping has no independent oracle in this tree — `src/index/fixture.rs:334-343`
(`slot_value`: 0 mean, 1 lrs, 2 geno_mb, 4 additive, 5 year) is a second transcription of the
same SCOUT-INDEXER §2 list the renderer uses (`src/grammar.rs:230-262`). If that list is
wrong, Q31 and Q36 both `PASS` anyway, because the double and the crate make the same mistake.

**4.3 · The error-class PASSes grade the class, not the diagnostic — named: Q51, Q52, Q42.**
`tests/parity_corpus.rs:559-566` fails the row only when the message is *empty*:

```rust
                let verdict = if xapian_error_type.as_deref() == Some(expected) {
                    // The message text is not part of the graded class, but
                    // printing it is what makes a wrong message visible.
                    if message.trim().is_empty() {
```

Any non-empty text passes. `docs/CORPUS-RUNNER.md:84-88` says the crate "must produce"
`QueryParserError`, `"Syntax: <expression> AND <expression>"` for Q51 — the runner does not
check that, so the doc describes a stricter grader than the code. (The message *is* pinned,
by `tests/grammar_render.rs:414-441`; the runner's claim is the thing that is wrong.)
Separately, Q42's PASS means "the crate agrees with AMENDMENT E's re-baselined 422", not with
the recorded 500 — defensible, but it is a `PASS` against a decision, not against the corpus.

---

## 5 · VERDICT AND REMAINING BLOCKERS

**PASS-WITH-FIXES** — the pass closes the false-green mechanism for real and the two
false-rejection fixes are correct for their named inputs; the deliverable's *verdict column* is
what I do not accept, on B3 and B4.

1. **B4 is not fixed, only narrowed** — `src/grammar.rs:762`. `value.starts_with('"')` makes
   the quote test per-token while GN4's is per-query (`native.rs:376-385`, `:399`). Input
   `"alcohol mean:abc..def"` (a quoted phrase containing a range-looking token) is a `422` in
   the crate and a `200` at GN4; `mean:1.."2"` likewise. Smallest fix: carry "this token
   starts inside a quoted region" through `render()` and skip field classification for it
   (mirror `guard_numeric_ranges`' `in_quotes`), and add the quoted-phrase case to
   `tests/grammar_render.rs:177`.
2. **B3 is marked FIXED while the crate still answers a different question** —
   `src/grammar.rs:521-536`. `year:2014` / `mean:5` render as AND-of-words; GN4 builds a
   phrase (`native.rs:721-724`) and its positionless index answers phrases with 0 hits
   (recorded Q04/Q11/Q12). `docs/DECISIONS.md` row 35 says exactly why that is not parity
   while row 34 ships it. Smallest fix: mark B3 PARTIALLY in `WORKER-BLOCKERS.md` §1/§4 and
   name the class next to the existing phrase skip; the real fix is a phrase-able
   `RenderedTerm` (or the backend handing phrases to xapian's parser, which row 35 already
   requires).
3. **The evidence floor protects a count, not the five shapes** — `tests/parity_corpus.rs:67`,
   `:478-495`. `MIN_GRADED = 5` is exactly today's evidence, and `CORPUS-RUNNER.md` instructs
   "raise it as the fixture grows". The moment a sixth gradable shape appears (the documented
   growth path), one of Q15/Q29/Q31/Q36/Q37 can silently degrade from `set-eq` to `SKIP`
   without failing anything: the count stays 5. Smallest fix: assert the graded *ids*, e.g.
   `let must_grade = ["Q15","Q29","Q31","Q36","Q37"]` and require every one to appear with
   `graded.is_match_set()` **and** `Verdict::Pass`; keep `MIN_GRADED` as the coarser backstop.

Non-blocking, worth a line in the report: (a) B1's operator table is a re-implementation of
xapian's grammar with one recorded shape as evidence — say so; (b) `CORPUS-RUNNER.md:73-75`
and `WORKER-BLOCKERS.md:108` should say "five comparisons, two of them two-sided" rather than
"can distinguish a correct renderer from a wrong one"; (c) the clause-(b) path, if it ever
fires (`:673` requires `our_top.len() >= 10`), compares the **unrestricted** recorded top-10
against our top-10 while clause (a) compares inventory-restricted sets — on a partially
widened fixture that is a guaranteed spurious `FAIL`, worth a comment now that widening is the
documented next step; (d) `docs/WORKER-CRATE.md`'s line-count table and §4.3 are stale (§1.5);
(e) the runner is default-feature only — under `--features xapian` `fixture_inventory()`
panics on `XapianIndex::open("")` (`src/index/xapian.rs:75-81`); that is the loud direction,
but the message will read as a fixture failure rather than "run this target without
`xapian`".

---

## Appendix · Method

* Corpus: 54 `Q*.json` in `/home/kabui/project5/genenetwork4/test/data/parity-fixtures`
  (read only); `type`, `recorded_status`, `hit_count`, `raw_query`, `identifiers` read
  per file. `identifiers` intersected with the fixture's five `(dataset, name)` pairs taken
  from `src/index/fixture.rs:378-455`.
* Gradability: each `raw_query` traced through `Grammar::render` (`src/grammar.rs:545-650`)
  and then `fixture_blocker` (`tests/parity_corpus.rs:238-333`) by hand — term kinds,
  slots, phrase check, type filter.
* Fixture-side match sets recomputed from the five documents' `mean` / `lrs` / `additive` /
  `year` / `species` values and compared with the corpus-side intersection: the two agree on
  all five graded shapes (5 / 5 / 2 / 2 / 5), which is why Q31 and Q36 are genuine two-sided
  PASSes rather than one-sided ones.
* GN4 ground truth read only: `src/search/native.rs` (lines cited inline) and
  `tests/search_parity.rs` (metric at `:64-76`, `:570-593`, classes at `:70-92`).
* No `cargo`, no build, no writes. The claim counts in `WORKER-BLOCKERS.md` §7 (32/24/6/4/7/8
  tests passing) were **not** re-executed; they are taken as attested.
