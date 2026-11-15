I have everything I need. Read-only review complete — no writes were possible in this role (bash is read-only, and no edit tool exists), so both deliverables are inlined below for the runtime to persist.

---


# WORKER-FIXES — reconstructed report of the F1–F7 fix pass

**Reconstruction, not the worker's own words.** The fix-pass worker died before writing
this file; there is no report on disk to recover. Everything below was rebuilt by reading
the code on disk against `docs/REVIEW-CRATE.md` (the finding text) and the ground truth in
`/home/kabui/project5/genenetwork4`. No compiler, no `cargo`, no index: `BASELINE.md` §1
(the `rustc` on `PATH` is broken; only the 1.88.0 toolchain has a working one).
So every "FIXED" below is a *static* verdict, and the line numbers are the proof I offer.

Provenance check: `docs/DECISIONS.md` was extended with rows **30, 31, 32, 33** and rows
**9, 13, 19** were rewritten — the fix pass's only surviving self-description. That is
consistent with F1–F7 being worked in the order the review recommended.

---

## 1 · F1–F7, one row each

| # | Finding (short) | Verdict | Proof | What the code actually does (quoted line) |
|---|---|---|---|---|
| **F1** | Default build answers any real index path with the five fixture hits | **FIXED** | `src/index/fixture.rs:95-101`; `src/lib.rs:109-113`; test `fixture.rs:376` | `fn open(path: &str) -> Result<Self, SearchError> { if !path.is_empty() { return Err(SearchError::Internal(format!("no search backend is compiled into this build, so the index at {path} cannot` — the path is refused before any doc is materialised; only `""` opens. `open_index` still picks the backend by *feature* (`lib.rs:114-121`), so the refusal is the whole gate. |
| **F2** | Bare numeric field value rendered as a point range; `mean:abc` → spurious 422 | **FIXED** | `src/grammar.rs:544-560`, `:700-717`; test `grammar_render.rs:122`, `:136` | `let Some((begin, end)) = value.split_once("..") else { return Ok(None); };` (`:708-710`) — `Ok(None)` means "not a range", the `numeric_range` arm at `:549` does not `continue`, so the token falls out of the field dispatch as `TermKind::FreeText` (`:589-593`). No `..` ⇒ no `RangeBound` error. DECISIONS row 9 rewritten to this reading. |
| **F3** | Empty field value (`position:`, `author:`) collapses to match-nothing | **FIXED** | `src/grammar.rs:514-521`; test `grammar_render.rs:151` | `if let Some((field, value)) = token.split_once(':').filter(\|(_, value)\| !value.is_empty())` — an empty value never enters the field dispatch at all, so `position:` is free text. DECISIONS row 30 added. |
| **F4** | `Hs:`/`Mm:` absent entirely (Q40/Q41 hit sets, Q42 error class) | **FIXED** | table `src/grammar.rs:240-253`; dispatch `:584-587`; renderer `:613-660`; message `:401-403`, class `:416`; tests `grammar_render.rs:272,295,317` | `location_shorthands: &[LocationShorthand { field: "Hs", species_term: "XShuman" }, …]`, rendered as three terms: `Query::term(self.species_term…)`-equivalent Boolean `XShuman`/`XSmouse`, `format!("{chromosome_prefix}{chrom_number}")` ⇒ `XC2`, and a slot-2 `ValueRange`; an invalid value returns `RenderError::InvalidLocation` whose `message()` is `"InvalidOperationError: Query is invalid (invalid Hs/Mm location field)"` and whose class is `"InvalidOperationError"`. Liftover stays out; DECISIONS row 13 rewritten. |
| **F5** | Seam is page-sized, GN4's engine is full-set; `sort=lrs` would drift | **FIXED** (contract stated + implemented; the finding's `search_all` split was *not* taken) | contract `src/index/mod.rs:41-47`; impl `src/index/fixture.rs:114-124`; DECISIONS row 33; tests `fixture.rs:435,455,466` | `/// This is the FULL match set, not one page of it … a backend that returned only `limit` hits would make the route's sorted path rank the wrong documents`; the fixture builds `matching` over all docs and returns `Ok(matching.into_iter().skip(to_usize(offset)).collect())` — `limit` places the offset only. `limit == 0`/`u64::MAX` give offset 0. |
| **F6** | No syntax-error path; Q51-class 422 unreachable | **PARTIALLY** | `src/grammar.rs:669-688`, `:491-496`; DECISIONS row 31; tests `grammar_render.rs:345,375` | Q51 is reproduced (`alcohol AND` → `SyntaxFault::DanglingOperator`, message `"Syntax: <expression> AND <expression>"`, class `QueryParserError`), and an odd quote count is rejected. But the operator rule over-rejects: for `alcohol AND NOT dependence` the `NOT` term hits `Some(previous) => is_operator(previous) && !exempt(previous)` (`:679`) → `dangling = true` → 422, which xapian accepts. See REVIEW-FIXES blocker 1. |
| **F7** | `serde_json::Map` instead of GN4's `BTreeMap` | **FIXED** | `src/hit.rs:81` (+ doc `:73-80`); DECISIONS row 19; test `hit_roundtrip.rs:125` | `pub extra: BTreeMap<String, serde_json::Value>,` — feature-unification-proof key order, exactly GN4's `search/mod.rs:150-152`. |

### Where the code differs from what the finding asked (said plainly, not rounded up)

1. **F1's second half was not done.** The finding offered "or gate the module behind a
   `fixture` feature". There is no `fixture` feature in `Cargo.toml`; the path refusal is
   the only mechanism. That is the finding's primary remedy, so F1 stands — but a build
   that sets `XAPIAN_DB_PATH` and forgets `--features xapian` now gets `Internal` (500)
   from the dev server (`src/bin/gn-search.rs:280`), while that binary's own module doc
   (`bin:22-24`) and DECISIONS row 28 promise **503 `index_unavailable`** for an unusable
   index. Newly visible, not newly wrong — but still a doc/code mismatch.
2. **F5's signature is unchanged.** `fn search(&self, q, limit, page)` still takes a page
   size and a page, and nothing in the type system stops the next backend from
   `take(limit)`. The contract is a comment plus one backend's implementation; the
   finding's "add `search_all` and make `search` = `search_all` + `take(limit)`" would have
   made it structural. Behaviour is right; enforcement is documentary.
3. **F6 does more than Q51.** The finding asked for "dangling operator / unbalanced quote
   or paren"; the implementation added the quote fault (`:492-495`) with **no recorded
   shape** — `SyntaxFault::UnbalancedQuotes` is justified only as "the error CLASS".
   xapian's actual tolerance of an unterminated phrase is unverified here.
4. **F2/F3 leave the token as written.** The free-text fallback pushes `token`
   (`:589-593`) — `year:2014` becomes the free-text term `"year:2014"`, `position:` becomes
   `"position:"`. The finding's own evidence says GN4 builds "a `mean 5` phrase"
   (`native.rs:723-724`), i.e. two tokens with the colon consumed. No recording pins the
   difference, and nothing here can settle it.
5. **`docs/WORKER-CRATE.md` was not updated.** It still says
   `extra: serde_json::Map<String, Value>` (`WORKER-CRATE.md:23`), still lists
   `page_bounds` in `src/index/mod.rs` (`:26`), and still describes the fixture as
   "honouring `XT{type}`, `limit`, `page`" (`:27`). Three of the seven fixes contradict the
   crate's own delivered report.

---

## 2 · The corpus runner as built

**Files:** `tests/parity_corpus.rs` (661 lines, one `#[test] fn corpus_report`, plus a
`metric_tests` module) and `docs/CORPUS-RUNNER.md`.

**The exact command a maintainer runs** (nothing else is needed; no GN4 build, no index):

```sh
cd /home/kabui/gn-search
cargo test --test parity_corpus -- --nocapture
# a different corpus (GN4's own override variable):
GN4_PARITY_FIXTURES=/path/to/parity-fixtures cargo test --test parity_corpus -- --nocapture
```

`--nocapture` is load-bearing: the per-shape report goes to stdout and cargo swallows it
for a passing test. Default corpus `…/genenetwork4/test/data/parity-fixtures` (54
`Q*.json`, `reduced-v1`); the loader panics on a missing directory, a non-object fixture,
or a missing `id`/`raw_query`/`type`/`recorded_status` key (`:150-232`).

**What it grades**, three things in this order (per shape, `grade()` at `:379-519`):

1. **Error class, always, index-independent** (`:400-447`). A shape whose
   `recorded_status != 200` must come back as `SearchError::InvalidQuery` whose
   `xapian_error_type` equals the expected class — `RecordedStatus 500 ⇒
   "InvalidOperationError"`, everything else ⇒ `"QueryParserError"` (`:99-107`). The
   message is checked only for being non-empty (`:416`).
2. **No false rejection, always** (`:451-467`). A 200 shape must render (`GRAMMAR.render`)
   and search (`gn_search::search`) without error. This is what would have caught F2.
3. **The match set, only where the fixture can answer it** (`:479-518`): set equality of
   `(dataset, name)` `Option` pairs against `recorded ∩ fixture inventory`, then clause
   (b) — Jaccard ≥ 0.8 on the top ten — when it applies.

**What it skips** (`fixture_blocker`, `:250-330`) — the rule is "a rendered term the
fixture cannot resolve the way the index does ⇒ SKIP, never PASS":

| term / shape | verdict |
|---|---|
| `type` ≠ phenotype (the fixture holds five phenotypes only) | SKIP |
| no terms at all (Q48, Q49) | SKIP |
| `FreeText` (stemming/scoring) | SKIP |
| `Operator` (the fixture ANDs everything) | SKIP |
| `Probabilistic` (`XD`/`XRF`/`XWK`) | SKIP |
| `ValueRange` slot 3 or 6 (not on `SearchHit`) | SKIP |
| `Boolean` author / chr / peakchr / tissue / symbol | SKIP |
| `Boolean` dataset / group / species, and `ValueRange` slots 0/1/2/4/5 | **graded** |

**The metric.** Constants and formula are the corpus's own: `JACCARD_FLOOR: f64 = 0.8`
(`:60` ↔ GN4 `tests/search_parity.rs:64`), `(dataset, name)` with both sides `Option`
(`:57` ↔ GN4 `:459-461`), `|A∩B|/|A∪B|` with an empty union defined as 1.0
(`:602-608` ↔ GN4 `:587-593`), clause (b) only for recorded ≥ 10 (`:281-287`). **One
addition and one deviation:** the expected set is restricted to the fixture's inventory
(`:486`), and clause (b) is additionally gated on `our_top.len() >= 10` (`:508`).

**Baseline claimed:** 54 shapes, 15 PASS, 0 FAIL, 39 SKIP. I recomputed that split
statically from the corpus JSON and the blocker table and it is the same 15:
Q15, Q16, Q17, Q19, Q28, Q29, Q30, Q31, Q36, Q37, Q38, Q39 (set equality) and Q42, Q51,
Q52 (error class). Of the set-equality twelve, five are only satisfiable by two-or-fewer
documents (Q31 `peak:5..15` → 10199+10201; Q36 `additive:0.5..1.0` → 10200+10211), three
expect *all five* fixture docs (Q15, Q29, Q37), and four expect *nothing* (Q16, Q17, Q19,
Q28, Q30, Q38, Q39 — seven, in fact). Nothing was executed; this is arithmetic over
`identifiers` ∩ the five fixture names.

---

## 3 · The three places I am least sure

1. **That any of it compiles or that the runner prints 15 PASS.** No build was possible
   (`BASELINE.md` §1). The nearest-miss spots I had to reason about: the `Copy`-closure
   reuse in `location_terms` (`:618` closure moved into `ok_or_else` at `:627` and used
   again at `:628` — legal only because a closure over a `&str` is `Copy`); `cfg(test)` in
   an *integration* test (`metric_tests`, `:594`) — enabled by `--test`, so it should run;
   and `map_or(false, …)`/`write!(f, "PASS")` in fixture/runner, which are clippy
   `is_some_and`/`f.write_str` fodder if the gate is `-D warnings`.
2. **xapian's own acceptance of `alcohol AND NOT dependence` and of an unterminated
   phrase.** Only the binding can settle these; both decide whether F6's new 422s are
   parity or drift.
3. **Whether `TermKind::FreeText` carrying `mean:2014`/`position:` is the same query GN4
   builds.** GN4's measurement (`native.rs:723-724`) says `mean:5` is a *phrase of two
   words*; the crate hands a backend one string with the colon in it. Q53 (`position:`,
   576 recorded hits) and the `year:2014` class land exactly here, and the fixture skips
   them, so no local evidence exists either way.

---
