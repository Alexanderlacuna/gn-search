# CORPUS-RUNNER — the crate against GN4's 54 recorded shapes, without GN4

`tests/parity_corpus.rs` replays the committed parity corpus through this
crate and prints one graded line per shape. It exists because the real
acceptance test — GN4's own `tests/search_parity.rs` — cannot run here
(`BASELINE.md` §1–§4: no usable toolchain for that tree, and a cold build is
hours). The corpus plus this crate is enough to check the parts of the crate
that do not need a real index.

## The command

```sh
cd /home/kabui/gn-search
cargo test --test parity_corpus -- --nocapture
```

`--nocapture` is required: the per-shape report goes to stdout, and `cargo`
swallows stdout for passing tests. **This target carries evidence only in the
DEFAULT build.** Under `--features xapian` the fixture backend is replaced by
the xapian stub, so the runner opens no inventory, `SKIP`s every
recorded-200 shape by name, grades nothing but the three error classes and
does not apply the `MIN_GRADED` floor — it passes, and passing means "it
builds and the error classes hold", not "the results are right". That run is
in "Under `--features xapian`" below, with both summary lines quoted. A
different corpus is picked up from the same variable GN4's harness honours:

```sh
GN4_PARITY_FIXTURES=/path/to/parity-fixtures cargo test --test parity_corpus -- --nocapture
```

Default corpus: `/home/kabui/project5/genenetwork4/test/data/parity-fixtures`
(54 files, reduced-v1, 7.1 MB). A missing or malformed corpus is a loud
failure, never a smaller run: that would grade less than the report claims.

Without cargo (the situation `BASELINE.md` describes), the same file compiles
and runs under `rustc 1.88.0` (README "Running the tests").

## The report

```
Q31 | peak:5..15 | phenotype | 11931 | 2 hits | set-eq | PASS
Q19 | dataset:bxdpublish | phenotype | 13757 | 0 hits | vacuous | VACUOUS(expected set
     empty: none of the 13757 recorded hits is one of the fixture's 5 documents, so equality
     proves nothing)
Q02 | insulin | gene | 57632 | 0 hits | - | SKIP(the fixture index holds the five recorded
     alcohol phenotype documents only; this shape recorded 57632 gene documents in the
     production index (no xapian backend is compiled into this build))
Q42 | Hs:foo:1M | phenotype | error 500 | InvalidQuery(InvalidOperationError) | error-class | PASS
```

| column | meaning |
|---|---|
| `id` | the corpus's shape id (`Q01`…`Q54`) |
| `query` | the recorded `raw_query`, verbatim |
| `type` | the recorded document type |
| `recorded` | the recorded hit count, or `error <status>` for an error shape |
| `ours` | the crate's hit count, or the error variant + exception class |
| `graded` | `error-class`, `set-eq`, `set-eq+top10`, `vacuous`, or `-` when nothing was graded |
| verdict | `PASS`, `FAIL(reason)`, `VACUOUS(reason)`, `SKIP(reason)` |

The summary after the lines is the audit: how many shapes were seen, how many
**graded** a set-equality or Jaccard comparison, how many graded only an error
class, how many were `VACUOUS`, and how many were skipped.

```
54 shapes seen: 5 graded a set-equality/Jaccard comparison, 3 graded an error class,
7 VACUOUS (expected set empty: no evidence), 39 SKIP; 8 PASS, 0 FAIL
exit rule: any FAIL fails the test, and so does fewer than MIN_GRADED = 5 graded
comparisons; PASS, VACUOUS and SKIP alone are not evidence
```

**The exit rule, in full.** The test fails (non-zero exit) when any of:

1. any shape `FAIL`s — a divergence; or
2. one of the five shapes that must grade — `Q15`, `Q29`, `Q31`, `Q36`,
   `Q37` (`tests/parity_corpus.rs`, `const MUST_GRADE`) — is missing from the
   corpus, or does not grade a set-equality comparison, or does not `PASS`;
   or
3. fewer than `MIN_GRADED = 5` shapes graded a set-equality or Jaccard
   comparison.

Rule 2 exists because a count is not a set: the growth path is a sixth
comparable shape, and the moment it lands, one of the five degrading from
`set-eq` to `SKIP` leaves the count at five while the run lost a fifth of its
real evidence. A grammar regression that turns a graded shape's terms into
free text does exactly that — it skips rather than failing. Rule 3 is the
coarser backstop for a thinned corpus, a thinned fixture or an all-SKIP run.

`SKIP` never counts as `PASS`, `VACUOUS` never counts as `PASS`, and neither
counts toward the floor. The floor is a lower bound, not a target: on the
committed corpus it is five comparisons, **two of which are two-sided**
(Q31, Q36 — their expected sets are 2 of the fixture's 5 documents, so they
fail in either direction); the other three (Q15, Q29, Q37) expect the
fixture's whole inventory, so they can only catch a renderer that drops hits.
**Raise it as the fixture grows; never lower it to make a run pass.**

**`VACUOUS` is not a pass.** A shape is `VACUOUS` when the expected set is
empty *after* the inventory restriction — the recorded answer lies outside the
five documents the fixture holds — so any renderer that returns nothing would
"pass" it. Seven of the corpus's shapes are `VACUOUS` today, and five of those
seven (Q16, Q17, Q19, Q28, Q30) recorded **thousands** of hits in production.
Counting them as passes is what `REVIEW-FIXES.md` §b(ii) called a green run
with no evidence, so they carry their own verdict and are excluded from the
count.

Baseline at the time of writing: **54 shapes seen — 5 graded (set-equality), 3
error-class, 7 VACUOUS, 39 SKIP, 0 FAIL.**

## What it grades

Three things, in order, per shape.

**1 · The query class (index-independent, always graded).** A shape recorded
as an error must come back as `SearchError::InvalidQuery` carrying the
exception class GN4 maps to 422 `invalid_query`:

| shape | recorded | crate must produce |
|---|---|---|
| Q51 `alcohol AND` | 400 `b'QueryParserError'` | `InvalidQuery`, `QueryParserError`, `"Syntax: <expression> AND <expression>"` |
| Q52 `mean:abc..def` | 400 `b'QueryParserError'` | `InvalidQuery`, `QueryParserError`, `"Unknown range operation"` |
| Q42 `Hs:foo:1M` | 500 `InvalidOperationError` | `InvalidQuery`, `InvalidOperationError` (AMENDMENT E re-baseline of the recorded 500 to 422 — `search_parity.rs:82-88`) |

**2 · No false rejection (index-independent, always graded).** A shape
recorded as a 200 must never be rejected by the grammar. This is the mirror
of (1) and it matters: `REVIEW-CRATE.md` F2 was exactly a false 422
(`mean:abc`), which no recorded shape pins but which a 200-shape rule would
have caught had one existed. It also catches the reverse of Q42: an
`Hs:` value that is *valid* must not be an error.

**3 · The match set (only where the fixture can answer it).** The corpus
metric is the corpus's own (`tests/search_parity.rs`): full-set equality of
`(dataset, name)` pairs, plus `JACCARD_FLOOR = 0.8` on the top ten. Both are
applied, with one addition that is the whole point of this runner: the
expected set is restricted to the documents the fixture index actually holds
(`recorded ∩ fixture inventory`). Without that restriction every non-empty
shape would "fail" for holding five documents instead of 57 632, which would
grade the double, not the crate.

## What it skips, and why (all of it the missing backend's fault)

The fixture index (`src/index/fixture.rs`) holds **the five recorded alcohol
phenotypes** and nothing else, and its matching is a deliberate double:
substring instead of stemming, one hit field per value slot, no term
dictionary, operators dropped. A shape whose rendered terms need anything
beyond that is `SKIP`ped with that reason, and the reason always names the
missing backend. The rule, as coded (`fixture_blocker`):

| the rendered query contains | verdict | why the fixture cannot decide it |
|---|---|---|
| a phrase, or a `NEAR`/`ADJ` operator (Q04, Q11, Q12) | SKIP | `phrase-not-expressible`: the rendered terms may say the words are ADJACENT (`RenderedTerm::phrase`), and the fixture double can only AND, while this index carries no positions, so GN4 records 0 hits for both classes (`genenetwork4/docs/search-engines.md:98-102`; DECISIONS rows 35, 36) |
| a field token the grammar does not consume, where the label and value are one phrase (`year:2014` is the phrase `year 2014`, `mean:5` is `mean 5`) | SKIP | `phrase-not-expressible`: same class as the row above — GN4 builds a phrase (`native.rs:721-724`) and a phrase matches 0 documents here, so grading the AND-of-words would grade the double, not the crate (DECISIONS rows 34, 36) |
| `type=` anything but `phenotype` | SKIP | the five fixture documents are phenotypes (DECISIONS row 16); the recorded set is from the production index |
| no terms at all (Q48, Q49) | SKIP | GN4's route rejects an empty `q` with 400 *before* the engine (SYNTHESIS §2d); the recorded 200 is gn3's answer to a query this seam never sees |
| free text (`alcohol`, `alcohol*`, `"a b"`, `a&&b`) | SKIP | the index stems and scores free text (SCOUT-NATIVE §3); the fixture does a substring test over five documents |
| an operator (`AND`, `OR`, `NOT`, `XOR`, `NEAR`, `ADJ`) | SKIP | boolean structure is the backend parser's job (DECISIONS rows 6–7); the fixture ANDs every term and drops operators |
| a probabilistic field (`description:`, `rif:`/`RIF:`, `wiki:`/`WIKI:`) | SKIP | `XD`/`XRF`/`XWK` text lives in the index dictionaries and on no `SearchHit` field |
| `author:`, `symbol:`, `chr:`, `peakchr:`, `tissue:` | SKIP | the fixture has no field that means the same thing (whole author names vs author terms; no symbol field; the index's `XC`/`XPC` provenance is not visible in a `SearchHit`) |
| a value slot the hit shape does not carry (`peakmb:` = slot 3, sample count = slot 6) | SKIP | `slot_value` has no field for it |

Everything else — the exact-term fields the hit shape does carry
(`dataset:`, `group:`, `species:`) and the value slots it does carry
(`mean:` 0, `peak:` 1, `position:`/`pos:` 2, `additive:` 4, `year:` 5) — is
graded by set equality.

The shapes that therefore carry real weight today:

```
Q15 species:mouse            all 5 fixture docs, set equality    GRADED
Q29 mean:..15                all 5 fixture docs                 GRADED
Q31 peak:5..15               2 of the fixture's 5 (slot 1)       GRADED
Q36 additive:0.5..1.0        2 of the fixture's 5 (slot 4)       GRADED
Q37 year:2010..2020          all 5 fixture docs (slot 5; F2)     GRADED
Q16 group:B6                 recorded 3,      0 in the fixture   VACUOUS
Q17 group:b6                 recorded 3,      0 in the fixture   VACUOUS
Q19 dataset:bxdpublish       recorded 13757,  0 in the fixture   VACUOUS
Q28 mean:10..20              recorded 1206,   0 in the fixture   VACUOUS
Q30 mean:10..               recorded 8644,   0 in the fixture   VACUOUS
Q38 year:2030..2050          recorded 0                         VACUOUS
Q39 mean:99999..100000       recorded 0                         VACUOUS
Q42 Hs:foo:1M                error class                        graded (error class)
Q51 alcohol AND              error class                        graded (error class)
Q52 mean:abc..def            error class                        graded (error class)
```

So the match-set evidence is **five** comparisons, plus three error classes.
The five are the shapes where a grammar regression turns the report red (0 hits
≠ 5, or 3 hits ≠ 2) — five comparisons, two of them two-sided, as above — and
they are named in `MUST_GRADE`, so losing one fails the run even if another
comparable shape joins the corpus. The seven `VACUOUS` shapes are honest but
weak: they prove the crate does not invent a hit, not that it finds the right
ones — five of them answer thousands of documents in production and are graded
against the empty set. That gap is stated here rather than hidden in a PASS
column, and the `MIN_GRADED` floor is set on the five so that losing one of
them fails the run instead of quietly shrinking it.

## What it proves, and what it cannot

Proved, and this is the whole of it — the report of the run on the committed
corpus, verbatim: `54 shapes seen: 5 graded a set-equality/Jaccard comparison,
3 graded an error class, 7 VACUOUS (expected set empty: no evidence), 39 SKIP;
8 PASS, 0 FAIL`. That is: the grammar's verdict on the 54 recorded queries
agrees with GN4's — the three recorded error shapes come back with the same
exception class and no recorded 200 shape is rejected — `Hs:`/`Mm:` produce
the species/chromosome/position tree and its error class; a numeric field
value is a range only with `..` and only unquoted; empty values are text; a
text term carries no field label; and for the five shapes the fixture can
answer, the rendered terms select the same recorded documents the production
index selected. Also proved: the run cannot be green with no evidence — the
`MIN_GRADED` floor is on the count of those comparisons, and `VACUOUS` is its
own verdict.

Not proved (documented, not hidden):

1. **Clause (b) is never exercised.** The top-10 Jaccard needs ten hits from
   the real index; a five-document fixture cannot produce them, so the code
   path exists and is unit-tested with synthetic ten-hit sets, but no real
   shape reaches it. It also compares the UNRESTRICTED recorded top ten
   against our top ten, while clause (a) compares inventory-restricted sets:
   on a partially widened fixture that becomes a spurious `FAIL`, so whoever
   widens the fixture must restrict `recorded_top` too (the code says so at
   the clause-(b) call site).
2. **No stemming, wildcard, phrase, `NEAR`/`ADJ` or operator semantics.**
   `alcohol*` (Q05), `"alcohol dependence"` (Q04) and `alcohol OR dependence`
   (Q08) are all `SKIP`: their results depend on the index's term dictionary
   and positional data.
3. **No ranking.** Hit order is the fixture's build order; relevance is the
   real index's.
4. **No field-level parity.** Only `(dataset, name)` is compared, exactly as
   the corpus grades; `lrs`, `geno_chr`, `pubmed_link` and `extra` are not.
5. **Nothing about the xapian binding.** The backend does not exist here
   (`src/index/xapian.rs` is a stub), so no xapian code path is exercised.
6. **No quoted, phrase or operator class is graded, so the runner cannot see a
   regression there.** `VERIFY-FINAL.md` §4 measured this rather than
   arguing it: with blocker B-A reverted in a scratch copy — a spurious 422
   restored on `"alcohol mean:abc..def"`, a quoted range no longer a range —
   `corpus_report` still reported `8 PASS, 0 FAIL` and exited 0, and the same
   held for B-B reverted and for B-C's named layer deleted. The 39 `SKIP`
   classes absorb those regressions because **the corpus holds no
   quoted-range shape to see them with**, so "the grammar rejects exactly the
   classes GN4 rejects and nothing more" was falsifiable and false and is not
   claimed here: what is proved is that the grammar's verdict on these 54
   recorded queries agrees with GN4's. The guard for the quoted/range-inside-
   quote class is the unit test
   `tests/grammar_render.rs::a_range_looking_token_inside_a_quote_is_not_a_422`,
   not this runner.
7. **The corpus is data, not truth about the code.** If the recordings were
   mistranscribed, this runner inherits it; it is the same corpus GN4 grades
   against, which is the point.

One of those gaps is the crate's main representational limit rather than a
fixture limitation, so it has its own section next: the phrase class.

## What this runner cannot grade and why

This is the crate's **main representational limit**, stated as its own section
because every other gap below is "the fixture is a double" while this one is
"the seam cannot say it":

> **The phrase class.** A query whose answer depends on word ADJACENCY is
> not graded by anything in this crate, and cannot be graded by this runner at
> all.

### The class

Three syntactic forms, one semantics:

| form | example | GN4 builds |
|---|---|---|
| a quoted region | `"alcohol dependence"` (Q04) | one xapian phrase |
| a positional operator | `alcohol NEAR dependence` (Q11), `alcohol ADJ dependence` (Q12) | a phrase, via xapian's `NEAR`/`ADJ` |
| a field token the grammar does not consume as a range | `year:2014`, `mean:5` | the phrase `year 2014`, `mean 5` (`native.rs:721-724`) |

The renderer emits the WORDS and marks them `phrase`
(`RenderedTerm::phrase`, `src/grammar.rs`; `docs/DECISIONS.md` rows 34 and
36), so the adjacency is expressible to a backend — but this crate never
evaluates it, and nothing here checks that a backend would.

### The evidence: 0 hits, so there is nothing to compare

GN4's index stores stemmed terms with **no positions**, so a phrase or a
`NEAR`/`ADJ` query parses and then matches **zero** documents
(`genenetwork4/docs/search-engines.md:98-102`). The corpus records exactly
that: **Q04, Q11 and Q12 each recorded 0 hits**.

An expected set of zero is not gradeable evidence by this runner's own rule
— it is `VACUOUS`, and the zero is structural, not a fact about ranking. If
the runner graded it, any backend that ANDed the words would pass (the zero
would agree), while an AND matches a *larger* set than the phrase: passing the
shapes would bless exactly the false-positive query the marker exists to
prevent. So the class is **`SKIP`ped by name**, `phrase-not-expressible`
(`is_phrase_shape`, `tests/parity_corpus.rs:396`; the reason string says the
words are adjacent and the fixture can only AND). No new code was needed for
this: the naming already existed.

How much that is on the committed corpus: **3 of the 54 shapes** (Q04, Q11,
Q12), all of them recorded-0. The field-token branch of the same test
(`year:2014` → the phrase `year 2014`) is reachable in the code but no
committed shape reaches it; it is exercised only by the constructed shapes
Q55/Q56 in `tests/parity_corpus.rs::a_phrase_shape_is_skipped_as_unrepresentable`.

### What guards it today, and what that is worth

One unit test per form, and no parity:

| guard | what it proves | what it does not |
|---|---|---|
| `tests/grammar_render.rs::a_bare_field_value_is_a_phrase_not_an_and` | `year:2014` renders two words BOTH marked `phrase`; `position:` is one word; `mean:1..2` is a range, not a phrase | that a backend turning those words into a phrase returns GN4's documents |
| `tests/grammar_render.rs::a_quoted_value_is_never_a_range` | a quoted value is phrase content, never range syntax or a 422 | any match set for a quoted phrase |
| `tests/parity_corpus.rs::a_phrase_shape_is_skipped_as_unrepresentable` | the phrase class is skipped BY NAME, never graded or passed | that phrases are right — it only proves the runner declines to claim |

So the honest sentence is: **the phrase class has a shape and no oracle.**
The 39 `SKIP`s absorb it, which is why `docs/VERIFY-FINAL.md` §4 could revert
the quoting fixes and still get `8 PASS, 0 FAIL`.

### The requirement on the real backend

**The phrase class is answered by handing the phrase-marked terms to xapian's
own parser as ONE phrase — never as an AND of words** (`docs/DECISIONS.md` row
35). Concretely: consecutive `phrase` terms are one phrase object passed to
`parse_query_checked`; nowhere in the renderer or the seam does the crate
flatten them into `AND`. That requirement is met by construction in
`src/grammar.rs` (the marker) and can only be *verified* by a populated index
that stores positions — the positionless layout that produced Q04/Q11/Q12's
zeros will keep producing zeros, so a wider fixture does not close this gap.

## Under `--features xapian`

**Read this before quoting a green feature run.** `--features xapian` is a
compile-and-error-class gate, not an evidence gate: in that build the fixture —
whose document set every recorded match set is restricted to — is not compiled
in, `src/lib.rs` selects the xapian stub instead, and `corpus_report` grades
**0 match-set comparisons** and does **not** apply the `MIN_GRADED` floor. Green
there means exactly two things: **it builds**, and **the three recorded error
classes still hold**. It says nothing about whether any query selects the right
documents. The evidence gate is the **DEFAULT build.**

Both runs, verbatim (`cargo test --offline --test parity_corpus -- --nocapture`):

```
# default — the evidence gate: 5 match-set comparisons, floor applied
54 shapes seen: 5 graded a set-equality/Jaccard comparison, 3 graded an error class, 7 VACUOUS (expected set empty: no evidence), 39 SKIP (the missing xapian backend's: no xapian index is compiled into this build, see docs/CORPUS-RUNNER.md); 8 PASS, 0 FAIL
exit rule: any FAIL fails the test, and so does fewer than MIN_GRADED = 5 graded comparisons; PASS, VACUOUS and SKIP alone are not evidence

# --features xapian — 0 match-set comparisons, the floor is NOT applied
54 shapes seen: 0 graded a set-equality/Jaccard comparison, 3 graded an error class, 0 VACUOUS (expected set empty: no evidence), 51 SKIP (the missing fixture backend's: no fixture inventory is compiled into this build, see docs/CORPUS-RUNNER.md); 3 PASS, 0 FAIL
exit rule: any FAIL fails the test, and so does a graded match-set comparison in a build with no fixture backend to grade one against; the MIN_GRADED = 5 evidence floor is not applied to this run

no fixture backend under this feature: 51 shapes SKIP(no-fixture-backend-under-this-feature), 0 graded — a compile-and-error-class gate, not evidence about any match set
```

Two things in those lines are the report telling you which build you are in,
not decoration: the `SKIP` parenthetical (`SKIP_ATTRIBUTION` in the test —
the missing **xapian** backend by default, the missing **fixture** inventory
under the feature, never the other build's absence, `VERIFY-REMAINING.md`
F2), and the exit rule (the floor of 5 in the default build; no floor, and an
assertion that *nothing* graded a match set, under the feature, `F3`). A run
that names the wrong absence is a bug in the runner, not a wording nit.

The mechanism: `FIXTURE_BACKEND` is `false` under the feature
(`tests/parity_corpus.rs:101`), so `corpus_report` opens no inventory, prints
`SKIP(no-fixture-backend-under-this-feature)` on every recorded-200 shape,
still grades the three error classes (the grammar decides those before any
index is opened), does not apply the `MIN_GRADED` floor, and asserts that zero
match-set comparisons were graded. The two tests that need the inventory are
gated on `#[cfg(not(feature = "xapian"))]`, which is why the target runs 7
tests there and 9 by default.

## How to widen it

The blockers are all "the fixture cannot answer this", so they disappear by
construction when a real backend exists: implement `Index` over xapian and
the same file grades clause (a) and clause (b) at full width (that is what
GN4's own harness does). The runner needs no change for that — only a way to
point `FIXTURE_PATH` at an index, which is deliberately not wired in while
the only backend is a test double that refuses non-empty paths
(`src/index/fixture.rs`, F1).

## Evidence that the runner bites

With the `Hs:`/`Mm:` error path and the syntax check disabled in a scratch
copy of the crate, the report turns to `13 PASS, 2 FAIL` and the test fails
with:

```
2 shape(s) diverged: ["Q42: FAIL(recorded 500, crate answers 200 with 0 hits)",
                      "Q51: FAIL(recorded 400, crate answers 200 with 5 hits)"]
```

That is the red-control for the two fixes the corpus can see (F4, F6). F1,
F2, F3, F5 and F7 are pinned by their own tests instead, because no recorded
shape exercises them (`mean:5`, `position:`, `limit=0`, an unknown index
path, `extra` key order).

**A thinned corpus fails without failing anything.** With one graded shape
removed from a copy of the corpus (`Q31.json`, the `peak:5..15` comparison):

```sh
mkdir /tmp/thinned && cp test/data/parity-fixtures/Q*.json /tmp/thinned/
rm /tmp/thinned/Q31.json
GN4_PARITY_FIXTURES=/tmp/thinned cargo test --test parity_corpus -- --nocapture
```

```
53 shapes seen: 4 graded a set-equality/Jaccard comparison, 3 graded an error class,
7 VACUOUS, 39 SKIP; 7 PASS, 0 FAIL
thread 'corpus_report' panicked at tests/parity_corpus.rs:
4 of 53 shapes graded a set-equality or Jaccard comparison, below the evidence floor
MIN_GRADED = 5: 7 VACUOUS, 39 SKIP, 3 error-class.
```

The report says `0 FAIL` and the run still exits non-zero: that is the floor
doing its job. `tests/parity_corpus.rs::metric_tests`
(`a_thinned_run_falls_below_the_evidence_floor`,
`an_empty_expected_set_is_vacuous_not_graded`,
`a_phrase_shape_is_skipped_as_unrepresentable`) pins the same behaviour
without the shell: the grading functions are tested on constructed rows and
shapes, not only through the corpus — except under `--features xapian`, where
the last two are gated out because they grade through the fixture's inventory
(see "Under `--features xapian`" above).
