I have everything I need. Here is the artifact (no write-capable tool in this role, so the runtime persists it).

---

# VERIFY-FINDINGS — independent verification of the F1–F7 pass

Reviewer: `reviewer-alt`, non-GLM family, independent of the author.
Everything below was **run** on this tree. Where a sentence restates a claim of
`docs/WORKER-FINDINGS.md` it is marked *re-derived* (I measured it myself) or
*unverified*; nothing is taken on trust.

Toolchain `~/.rustup/toolchains/1.88.0-x86_64-unknown-linux-gnu/bin` —
`cargo 1.88.0 (873a06493 2025-05-10)`, `rustc 1.88.0 (6b00bc388 2025-06-23)`.
Every cargo invocation `--offline` inside `timeout 900`. Every build used an
**out-of-tree** `CARGO_TARGET_DIR` (`/tmp/gnv-*`), so no file in the repository
was created or modified: `git status --porcelain` is the same before and after
my run as it was when I started (`?? docs/VERIFY-REMAINING.md`,
`?? docs/WORKER-FINDINGS.md`). Nothing was built or written under
`/home/kabui/project5/genenetwork4` — it is only *read*, by the corpus runner
and by me. Nothing was pushed. Reviewer workspace created in `/tmp` only
(`/tmp/gnv-alt-hist/<sha>` per-commit trees via `git archive | tar -x`,
`/tmp/gnv-probe` for the grammar probe, `/tmp/gnv-alt-tgt` for builds).

Tree verified: `HEAD = 3051bf0`, `master`, 13 commits, no staged content.

---

## 1 · The gate, run by me

### 1.1 Counts (`passed / failed` per target)

| target | `--offline` | `--features serve` | `--features xapian` | `--all-features` |
|---|---|---|---|---|
| `unittests src/lib.rs` | 32 / 0 | 32 / 0 | 38 / 0 | 38 / 0 |
| `unittests src/bin/gn-search.rs` | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 |
| `tests/error_status.rs` | 4 / 0 | 4 / 0 | 4 / 0 | 4 / 0 |
| `tests/grammar_render.rs` | 26 / 0 | 26 / 0 | 26 / 0 | 26 / 0 |
| `tests/hit_roundtrip.rs` | 6 / 0 | 6 / 0 | 6 / 0 | 6 / 0 |
| `tests/manifest.rs` | 7 / 0 | 7 / 0 | 7 / 0 | 7 / 0 |
| `tests/parity_corpus.rs` | 9 / 0 | 9 / 0 | 7 / 0 | 7 / 0 |
| doc-tests | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 |
| **total** | **84 / 0** | **84 / 0** | **88 / 0** | **88 / 0** |
| exit | 0 | 0 | 0 | 0 |

`cargo fmt --check` → exit **0**. `cargo build --offline` → exit **0**, 0
warnings. `cargo clippy --offline --all-targets` → **0** warning lines in all
four feature combinations (default, `serve`, `xapian`, `all-features`).

These 32 cells and four totals are exactly `README.md:121-131` *re-derived*.

### 1.2 The corpus runner, `--nocapture`, both builds, verbatim

```
$ cargo test --offline --test parity_corpus -- --nocapture
parity corpus: 54 shapes from /home/kabui/project5/genenetwork4/test/data/parity-fixtures (fixture index: 5 documents)
…
54 shapes seen: 5 graded a set-equality/Jaccard comparison, 3 graded an error class, 7 VACUOUS (expected set empty: no evidence), 39 SKIP (the missing xapian backend's: no xapian index is compiled into this build, see docs/CORPUS-RUNNER.md); 8 PASS, 0 FAIL
exit rule: any FAIL fails the test, and so does fewer than MIN_GRADED = 5 graded comparisons; PASS, VACUOUS and SKIP alone are not evidence
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.92s
```

```
$ cargo test --offline --features xapian --test parity_corpus -- --nocapture
parity corpus: 54 shapes from /home/kabui/project5/genenetwork4/test/data/parity-fixtures (no fixture backend under this feature)
…
54 shapes seen: 0 graded a set-equality/Jaccard comparison, 3 graded an error class, 0 VACUOUS (expected set empty: no evidence), 51 SKIP (the missing fixture backend's: no fixture inventory is compiled into this build, see docs/CORPUS-RUNNER.md); 3 PASS, 0 FAIL
exit rule: any FAIL fails the test, and so does a graded match-set comparison in a build with no fixture backend to grade one against; the MIN_GRADED = 5 evidence floor is not applied to this run
no fixture backend under this feature: 51 shapes SKIP(no-fixture-backend-under-this-feature), 0 graded — a compile-and-error-class gate, not evidence about any match set
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.15s
```

Per-shape detail confirms the aggregates: default PASS = Q15, Q29, Q31, Q36,
Q37 (set-eq, of which Q31/Q36 are two-sided, 2-of-5) + Q42, Q51, Q52
(error-class); VACUOUS = Q16, Q17, Q19, Q28, Q30, Q38, Q39. `5+3+7+39 = 54`.
In the feature build every recorded-200 shape prints `Internal` in the *ours*
column (the xapian stub's empty-path refusal) and `SKIP(no-fixture-backend-…)`,
while Q42/Q51/Q52 still grade — i.e. the grammar decides them before any index
is opened, as documented.

---

## 2 · F1 … F7

The seven are the table in `docs/VERIFY-REMAINING.md` §Findings.

| # | verdict | proving file:line (re-derived) | note |
|---|---|---|---|
| **F1** two commits do not compile on their own tree | **FIXED** | history rewritten: `6e2bd16` (squash of `caf6c02`+`0b86e57`), `a83e298` (old `3028835` + the two `pub mod` lines), `src/lib.rs:50-54`, `src/index/mod.rs:23-24`. My run, `git archive` per commit, isolated `CARGO_TARGET_DIR` per commit: **13/13** `fmt --check` 0, `build` 0, `cargo test` 0 — `6e2bd16` 58 / 0, `a83e298` 65 / 0 (**71** under `--features xapian`), `6a16ce4` 84 / 0, … `3051bf0` 84 / 0 | every `pub mod` now lands in the commit that carries its body; the two old reds are gone. One residual, honestly disclosed (`WORKER-FINDINGS.md` risk 1): the *oldest* commit is red under `--features xapian` — `error[E0433]: could not find 'xapian' in 'index' → src/lib.rs:120:44, :121:12` — because that commit predates the module |
| **F2** skip reason named the wrong component | **FIXED** | `tests/parity_corpus.rs:122` `SKIP_ATTRIBUTION = if FIXTURE_BACKEND {…} else {…}`, interpolated at `:877`; module doc `:34-41`; both docs re-quoted (`README.md:167,171`, `docs/CORPUS-RUNNER.md:341,345`) | §3 below, side by side. Right component in each build. Residual nit: the default line is a 37-of-39 attribution, and it says "no xapian *index* is compiled" where the missing thing is the backend |
| **F3** the exit rule printed a floor the run does not apply | **FIXED** | `tests/parity_corpus.rs:887-898` (if/else print), and the two assertions that follow are exactly the two printed rules — `:900-908` any FAIL, `:918-921` `counts.graded == 0` under the feature, with `:927` `return` *before* `evidence_shortfall` at `:929` | measured in both builds (§1.2); the feature line names the two rules it enforces and says the floor is not applied |
| **F4** "the three pre-checks" for a five-check module | **FIXED** | `README.md:28` now "STUB behind `--features xapian`: the pre-checks, then unimplemented!" — the count is gone rather than restated | agrees with `src/index/xapian.rs:79-116` (five checks) and with `README.md:48-57` |
| **F5** the runner "needs no change" for a real backend | **FIXED** | `docs/CORPUS-RUNNER.md:368-390` ("How to widen it — the next step, and it is not a no-op"): names `FIXTURE_BACKEND = cfg!(not(feature = "xapian"))`, states that the feature which brings the backend is the one that switches grading off, and lists the two changes (a real `fixture_inventory()` behind the `Index` trait; drop the gate) | the false sentence is gone |
| **F6** gate block / 84-0 table are machine-local | **FIXED** | `README.md:104-112` (both preconditions: pinned `1.88.0` with `bin` first on `PATH`; corpus at `DEFAULT_CORPUS` or `GN4_PARITY_FIXTURES`, plus the "panics with `no Q*.json fixtures in …`" consequence) and the caption `README.md:116-119` ("on that toolchain, with that corpus") | **with one defect**: the citation in `README.md:109` is `tests/parity_corpus.rs:88`, and line 88 is the tail of `MUST_GRADE`'s doc comment — `const DEFAULT_CORPUS` is at `:92`. Same drift at `docs/CORPUS-RUNNER.md:360,377` (`:101` vs the const at `:105`). Finding (2) |
| **F7** `VERIFY-FINAL.md` still asserted README staleness | **FIXED** | `docs/VERIFY-FINAL.md` `3051bf0` is **+17 / −0**; two `[CORRECTED 2026-11-15]` blocks (after F4, and after the verdict's first paragraph), original text untouched, both corrections accurate against the tree (88 / 0 feature, the `#[cfg]` half of the fixture test, the `GRAMMAR.render` switch) | additions only, as the convention requires |

---

## 3 · F2 in detail: does the printed line describe the right absence, per build, and does the README quote the corrected text?

**Yes, both.** The prose, the source, and the measured output, side by side:

| what the docs say the build is missing | what the build prints | what the build actually lacks |
|---|---|---|
| `README.md:163-168` — "the missing xapian backend's: no xapian index is compiled into this build" | identical, byte for byte (§1.2, default line) | the `xapian` **feature** is off, so `src/lib.rs:104-108` selects `FixtureIndex`; there is no xapian code in the binary |
| `README.md:170-174` — "the missing fixture backend's: no fixture inventory is compiled into this build" | identical, byte for byte (§1.2, feature line) | `FIXTURE_BACKEND = cfg!(not(feature = "xapian"))` (`tests/parity_corpus.rs:105`) is `false`, so `corpus_report` opens no inventory (`:843-850`) and every recorded-200 shape short-circuits |

`docs/CORPUS-RUNNER.md:341-348` quotes the same five lines, also byte-identical;
so does `docs/WORKER-FINDINGS.md` §1. I compared all five against my captured
`--nocapture` output with `diff`, not by eye: **no difference**.

The README paragraph that frames them (`README.md:176-182`, "Read the two `SKIP`
parentheticals against each other: each names the absence *its own* build has")
is true of the output.

Two residuals, neither of which re-opens F2:

* **37 of 39, not 39 of 39** (nit). In the default build, Q48 and Q49 are
  skipped because "GN4's route rejects an empty `q` with 400 before the engine
  ever sees it (SYNTHESIS §2d)" — a route-owned gap, not the missing xapian
  backend. The per-shape text is right; only the aggregate parenthetical
  generalises. Smallest fix: leave the parenthetical, add "39 SKIP (…; 2 of
  them the route's own validation, see the per-shape reasons)" — or say nothing
  and let the per-shape lines carry it.
* **"index" vs "backend"** (nit). "no xapian index is compiled into this build"
  — an index is data, not compiled; the per-shape reasons say "no xapian
  **backend** is compiled into this build", which is the accurate noun.
  Smallest fix: one word in `tests/parity_corpus.rs:124`, then re-quote the two
  lines (which also un-sticks finding (4)'s "quoted as evidence" problem).

---

## 4 · Did anything get weakened to pass?

**No assertion, skip, or gate changed shape in this pass.** The whole pass is
three doc-only commits, one history rewrite, and two `println!` calls. What I
checked, per commit (`git show --stat`, then every hunk read):

| commit | files | executable change? |
|---|---|---|
| `6e2bd16` | 9 files, +2767 (squash) | snapshot tree only — see below |
| `a83e298` | 5 files, +570 | none (adds `src/index/xapian.rs`, `src/manifest.rs`, `tests/manifest.rs`, 2 `pub mod` lines) |
| `6a16ce4` | 4 files, +1470 | the parity runner arrives (no prior assertion to weaken) |
| `83f0be5` | 1 file, +403 | the dev server arrives |
| `f425901` | 18 files, +4233 | docs only |
| `2151a56` | `Cargo.lock`, +107 | lock only |
| `5e48455` | 2 files, **+47 / −0** | docs only, additions only |
| `aec5877` | 7 files, +102/−27 | the nine clippy sites — each read, each equivalent: `map_or(false, f)`→`is_some_and(f)` on the same `Option`s; `None => (None, None)`→`unwrap_or_default()` on `Option<(Option<f64>,Option<f64>)>`; the `Display` arm now matches `xapian_error_type: Some(e)` and falls to the same `_` arm for `None`; `#[allow(clippy::type_complexity)]` on a private fn; `&PathBuf`→`&Path`; one inline format arg. Zero behaviour moved |
| `280a438` | 2 files, +151/−22 | docs only (deletions are the stale sentences replaced) |
| `c0b4ec3` | 1 file, +168 | a report |
| `478bbf7` | 3 files, +71/−20 | **two `println!` calls**: one gains `{SKIP_ATTRIBUTION}`, the other becomes an `if FIXTURE_BACKEND {} else {}`. No assertion touched, no verdict logic touched, both branches of the runner's decision being already asserted below (`:900`, `:918`) |
| `9343458` | 2 files, +43/−12 | docs only |
| `3051bf0` | 1 file, +17/−0 | docs only, additions only |

Two things a reader should still name as "shape", not text:

1. **The history rewrite changed two snapshots, not one message.** Verified by
   tree diff, not by reading the report: `git diff a83e298 3028835` is **empty**
   (same tree, same message), `git diff 6e2bd16 0b86e57` is exactly the 4 lines
   `+pub mod manifest;` / `+#[cfg(feature = "xapian")]` / `+pub mod xapian;` and
   nothing else, `git diff 3051bf0 2e8f69e` is **empty** (delivered tree
   byte-identical), and the other eight pairs are empty. So no content was
   lost, and the only tree that ever changed is the oldest snapshot, which
   *lost* two declarations naming files that do not exist at that point. The
   squashed message keeps both original bullets and both bodies verbatim.
   All 13 subjects are `* path (entity): what changed.`, 54–72 chars, bodies are
   the *why*, no attributions, no dates **re-derived**.
2. **A narrowing that predates this pass, named because the question asks about
   gates:** `src/index/fixture.rs:392-410` runs the end-to-end half of
   `a_non_empty_index_path_is_refused` only `#[cfg(not(feature = "xapian"))]`,
   and `tests/grammar_render.rs`'s seven "not a 422" sites assert
   `GRAMMAR.render(q).is_ok()` rather than `gn_search::search(...).is_ok()`.
   Both were disclosed when made (`VERIFY-GATE-TRUTH.md` §3.1); under the
   default build they are equivalent (render is the only fallible step on an
   empty path), under the feature they are the difference between 7 reds and
   green. This pass neither widened nor narrowed them. There is no `#[ignore]`
   anywhere in `src/` or `tests/` (`grep -rn "ignore\]"` → 0).
3. **No test was deleted and no count moved backwards**: 58 → 65 → 84 → 84 …
   per commit, and 71/88 under the feature from `a83e298` on.

---

## 5 · Integration honesty

### 5.1 Is there any evidence in this repo that GN4 compiles against this crate?

**No. Zero, in this repo or anywhere on this machine.** Measured:

```
$ grep -rn "gn-search\|gn_search" --include=*.toml --include=*.rs /home/kabui/project5/genenetwork4
(no output — not in Cargo.toml, not in Cargo.lock, not in src/, not in tests/)
$ grep -rl --include=Cargo.toml "gn-search" /home/kabui   →  ./gn-search/Cargo.toml   (only)
```

So the crate has exactly one library consumer that has ever compiled against
it: its own `src/bin/gn-search.rs`. GN4 has never seen it. Nothing here is
"compiled against GN4" or "by GN4": the corpus is *data* read from GN4's
checkout (`tests/parity_corpus.rs:92`), and the scout documents are *prose*
about GN4's source. The integration is entirely untested.

And the claim is not merely untested — the seam is not yet *sufficient* in three
concrete ways, none of which any doc here says:

1. **The types are different types.** `gn_search::SearchHit` (`src/hit.rs:27-81`,
   19 fields) is a separate struct from `genenetwork4::search::SearchHit`
   (`src/search/mod.rs:119-152`, same 19 fields + a `utoipa::ToSchema` derive);
   likewise `gn_search::SearchError` (`src/error.rs`) vs GN4's `SearchError`
   (`src/search/mod.rs:155-186`). `Engine::search` returns **GN4's** types, so
   an adapter with two exhaustive conversions has to exist, in GN4 (this crate
   cannot depend on GN4, and does not).
2. **Async vs blocking.** GN4's `Engine::search` is `-> impl Future<…> + Send`
   (`src/search/mod.rs:191-203`) and its native impl wraps the blocking xapian
   work in `tokio::task::spawn_blocking` (`src/search/native.rs:435`, comment at
   `:430-432`: "keep them off the async runtime's workers"). This crate's entry
   point is blocking (`src/lib.rs:84-110`). An adapter that calls it directly
   inside the async fn would block an axum worker — the crate documents no such
   caveat anywhere.
3. **Backend selection is a Cargo feature here, config there.** `DECISIONS.md`
   row 15 / `src/lib.rs:104-108` pick the backend at build time; GN4 picks the
   engine at runtime with `SEARCH_ENGINE` (`src/search/mod.rs:101-110`). As
   shipped, *either* feature choice makes a real GN4 `/search` fail: default →
   `SearchError::Internal` for any non-empty path (`src/index/fixture.rs:96-101`,
   the F1 fix), `--features xapian` → `unimplemented!()` at
   `src/index/xapian.rs:47-58`. Linking it today buys GN4 a 500 or a panic, not
   a search.

`README.md:3-6` says the crate is "enough for GN4 to link it behind its existing
`Engine` trait without touching the route's contract" — a claim about
*sufficiency* that nothing tests, and `README.md:199-204` ("What the gate does
*not* prove") lists the xapian binding and a populated index but **not** this.
That is the F2 defect class one level up: the headline sentence is the only
claim in the tree that is stated as a property of the deliverable and is
entirely unexercised.

### 5.2 What an integration test would have to add

Minimum to turn "can be linked" into "is linked, compiles, and behaves":

| where | what | size |
|---|---|---|
| `genenetwork4/Cargo.toml` | one `[dependencies]` line: `gn-search = { path = "../gn-search" }` (or a vendored copy under `vendor/`, since the crate lives outside GN4's tree); `Cargo.lock` then gains one `[[package]]` entry | +1 line, +~7 lock lines |
| `genenetwork4/src/search/gnindex.rs` (new) | `pub struct CrateEngine { index_path: String }` — reading `XAPIAN_DB_PATH` per request, as `native.rs:494-503` does; `impl From<gn_search::SearchHit> for SearchHit` (19 fields, 1:1); `impl From<gn_search::SearchError> for SearchError` (5 variants, `InvalidQuery{message, xapian_error_type}` maps 1:1); `impl Engine for CrateEngine` whose body is `tokio::task::spawn_blocking(move ‖ gn_search::search(&path, &q, &type_, limit, page)).await` + `map_err(Into::into)` | ~70–90 lines |
| `genenetwork4/src/search/mod.rs` | `+pub mod gnindex;` and one dispatch arm in `search()` next to `"native"` (`:101-110`) | +2 lines |
| `genenetwork4/tests/search_crate_engine.rs` (new) | three assertions the crate can actually serve today: (a) a grammar-rejected query (`"alcohol AND"`, `"mean:abc..def"`, `"Hs:foo:1M"`) comes back as `InvalidQuery` with `xapian_error_type == Some("QueryParserError"/"InvalidOperationError")`; (b) a non-empty `index_path` is `Internal` with the refusal message (default features); (c) the conversion round-trips a hit with an unknown key through `extra` (`pubmed_link`) with the key order preserved | ~40 lines |
| commands | `cd genenetwork4 && cargo fmt --check && cargo build --offline && cargo test --offline --test search_crate_engine`; then the same with `--features gn-search/xapian` if the dependency is renamed, to prove the crate's *other* backend also links (`unimplemented!()` panics, so its assertion must be `std::panic::catch_unwind`, or the test is `#[should_panic]`); on this side `cargo test --offline --features xapian` already covers the panic path | 3–4 commands |

A *parity* integration test (the one `docs/SYNTHESIS.md` §5 step 5 describes) is
**not possible today** for a second, independent reason: the crate has no
backend that can open an index (`src/index/xapian.rs:44-58`), so no recorded
match set can be produced on this side of the seam at all — which is exactly
what `docs/CORPUS-RUNNER.md:368-390` now says.

### 5.3 Where OpenAPI and auth live after extraction, and what this crate must do about them

* **OpenAPI stays 100 % in GN4.** `#[derive(utoipa::IntoParams)] SearchParams`
  (`src/routes/search.rs:34`), `#[utoipa::path(get, path = "/search", …)]` with
  `body = Envelope<Vec<SearchHit>>` and the four documented error responses
  (`:154-168`), `#[derive(…, utoipa::ToSchema)] SearchHit`
  (`src/search/mod.rs:119`), and the document itself
  `#[derive(utoipa::OpenApi)]` in `src/lib.rs:40,204`. The crate has **zero**
  occurrence of `utoipa` (`grep -rn utoipa src tests Cargo.toml` → 0) and must
  stay that way: adding it would break `Cargo.toml:5-8` / `README.md:37-39`
  ("no third-party dependency beyond serde/serde_json … in any feature
  combination") for a schema that describes *GN4's* route type, which GN4 keeps.
  Nothing here needs to expose or check an OpenAPI schema.
* **Auth stays 100 % in GN4.** `use gnauth_macros::gnauth` + `#[gnauth(token_only)]`
  + the `AuthToken` extractor (`src/routes/search.rs:22-26,170-173`), `Unauthorized`
  in the documented responses (`:166`), and the route's `ApiError` mapping
  (`:172-269`). The crate must **not** check it, and its own docs already say so
  in three places: `docs/SYNTHESIS.md:122-123` ("the extracted delegate therefore
  needs: no auth"), `docs/DECISIONS.md` row 27 ("GN4 owns the real route,
  envelope and auth"), `src/bin/gn-search.rs:8-14` ("no TLS, no auth") with
  `README.md:66-72` repeating it. That is correct and consistent; the only
  related nit is (6) in the findings.

---

## 6 · Findings

| # | sev | file:line | why | smallest fix |
|---|---|---|---|---|
| 1 | **medium** | `README.md:3-6` (claim) vs `README.md:199-204` ("What the gate does *not* prove") | The crate is introduced as "enough for GN4 to link it behind its existing `Engine` trait without touching the route's contract", and **nothing in either tree has ever compiled GN4 against it** (§5.1: 0 references in `genenetwork4/Cargo.toml`, `Cargo.lock`, `src/`; the only consumer is this crate's own bin). Worse, the claim as stated needs three things that do not exist: two type conversions (`gn_search::SearchHit`/`SearchError` → GN4's distinct types), a `spawn_blocking` adapter (GN4's `Engine` is `impl Future + Send`, `src/search/mod.rs:191-203`, `native.rs:435`; this entry point is blocking), and a runtime backend choice (feature here, `SEARCH_ENGINE` there). As shipped, a real GN4 request gets `Internal` (default) or an `unimplemented!()` panic (`--features xapian`). This is the same defect class as F2 — a sentence about the deliverable that no gate exercises — one level up, and it is the one claim in the tree that a maintainer would act on first. | One sentence at the end of "What the gate does not prove": *"GN4 has never compiled against this crate, and nothing here proves it can. Linking it needs a GN4-side adapter that does not exist in either tree: `From<gn_search::SearchHit>`/`From<gn_search::SearchError>` for GN4's own types, and a `spawn_blocking` wrapper around `search()` because GN4's `Engine` is async. Until the `xapian` backend is real, that adapter would answer every search with `Internal` (default build) or panic (feature build)."* Plus one `DECISIONS.md` row recording it. |
| 2 | low | `README.md:109`; `docs/CORPUS-RUNNER.md:360` and `:377` | Both cite `tests/parity_corpus.rs` line numbers that the *same pass* moved: `README.md:109` points at `:88` for `DEFAULT_CORPUS` (it is at `:92`; `:88` is the tail of `MUST_GRADE`'s doc comment), and the two CORPUS-RUNNER cites say `:101` for `FIXTURE_BACKEND` (it is at `:105`; `:101` is its doc comment). `478bbf7` inserted 4 lines above the block, then `9343458` wrote the citations — so the F6 recovery instruction lands a maintainer four lines off the item it names, in a tree whose docs promise `file:line` precision. | `88` → `92` in `README.md:109`; `101` → `105` in `docs/CORPUS-RUNNER.md:360,377`. (Or drop the line numbers and cite the const names, which is what the sentence already does.) |
| 3 | low | `docs/WORKER-REMAINING.md:16-18` (committed at `c0b4ec3`) vs `docs/WORKER-FINDINGS.md` (untracked) | The committed report names three commits — `e9f8267`, `5c436bb`, `566bba2` — that are **not on `master`** after the F1 rewrite (`git merge-base --is-ancestor` → false for all three; they exist only on the local `backup-before-rehist` branch). A clone or `git archive` of `master` cannot resolve one word of that table, and the old→new map that fixes it lives in the one file the pass left **untracked** (`docs/WORKER-FINDINGS.md` §2.3 — the previous round's report, `WORKER-REMAINING.md`, *was* committed, so this looks like an oversight). | Replace the three SHAs with `5e48455`, `aec5877`, `280a438`, and commit `docs/WORKER-FINDINGS.md` (one `* docs/WORKER-FINDINGS.md (report): …` bullet) so the mapping ships with the history it describes. |
| 4 | low | `tests/parity_corpus.rs:122` and `:887-898` | The entire deliverable of F2 and F3 is those two printed sentences, and **no test asserts either**. `README.md:167,171,174` and `docs/CORPUS-RUNNER.md:341,346,348` quote them "verbatim" as the evidence the gate is honest; if a later edit changes `SKIP_ATTRIBUTION` or an exit-rule string, the docs go stale silently and every run stays green — which is precisely how F2 happened the first time (a string drifted from the code's behaviour and a doc quoted it as canonical). | Three lines in `mod metric_tests`: `assert_eq!(SKIP_ATTRIBUTION.contains("xapian backend"), FIXTURE_BACKEND);` plus `assert_eq!(SKIP_ATTRIBUTION.contains("fixture backend"), !FIXTURE_BACKEND);` — cheap, and it pins the component, not the wording. |
| 5 | nit | `tests/parity_corpus.rs:124` and `README.md:167`/`CORPUS-RUNNER.md:341` | In the default build the summary attributes all 39 SKIPs to "the missing xapian backend's", but **Q48/Q49 are route-owned** (empty/whitespace `q`, rejected with 400 before the engine — their own reason string says so), so the aggregate is a 37-of-39 characterisation; and "no xapian **index** is compiled into this build" names an index where the missing thing is the backend (the per-shape reasons say "backend"). | One word (`index` → `backend`) and optionally "; 2 of them the route's own parameter validation" in the parenthetical; then re-paste the two quoted lines. |
| 6 | nit | `src/bin/gn-search.rs:66,71` and `README.md:66-72` | The dev server is documented as having "no auth, no TLS", and its bind address is overridable (`GN_SEARCH_ADDR`), with the default `127.0.0.1:8080` loopback — but nothing refuses or warns on a non-loopback override, so `GN_SEARCH_ADDR=0.0.0.0:8080` publishes an unauthenticated search endpoint over the LAN with no prompt. Predates this pass (not introduced here), and it is a dev tool, but it is exactly a thing a compiler cannot catch. | One `eprintln!` warning when the parsed `SocketAddr` is not `is_loopback()`, one line in the README's "no auth" sentence. |

**No logic defect found in this pass.** The only executable changes are two
`println!` calls whose both branches are asserted downstream; all five grammar
residuals the earlier round left open re-checked clean in a live probe
(`/tmp/gnv-probe`): `alcohol AND NOT dependence` and `a AND (NOT b)` render
(the spurious 422 is gone, `DECISIONS.md` row 31), `year:2014` renders as the
phrase `year`+`2014` rather than one `year:2014` token (row 34),
`mean:"1..2"` is phrase content, `mean:1.."2"` is the one-sided range `1..`
plus the phrase word `2` (row 37), `Hs:chr2:1M..1.2M` renders
`XShuman ∧ XC2 ∧ slot-2 1.0..1.2` and `Hs:foo:1M` is `InvalidLocation` (F4 of
the original review). The dev server's live behaviour matches the README block
exactly (`/healthz` 200 `ok`; `/search?query=alcohol&…` 200, `"10199"` first;
`/index` 503 `index_unavailable`), and its doc now matches its code on the
`Internal`→500 / `Unreachable`→503 split (`src/bin/gn-search.rs:22-29` vs
`:290,293`; `DECISIONS.md` row 32).

---

## 7 · The judgement

Is this a fair thing to hand a maintainer as "the extraction skeleton, gates
green, here is what it does not do"? **Close to yes, and closer than the last
round.** The gate is literally what the README says it is, and I re-derived all
32 cells and both corpus blocks. History is now bisectable: 13 of 13 commits
pass `fmt` + `build` + `test` on their own tree, which the old history failed at
two of them, and the delivered tree is byte-identical to the pre-rewrite tree.
F1–F7 are all genuinely closed, the disagreements between prose and printed
output that F2/F3 were about are gone, and neither the runner nor any test was
weakened to get there.

What is still not true is the first sentence of the README. Everything else here
is careful about what has and has not been exercised — the xapian backend, the
index, the phrase class, the feature-vs-evidence floor — except the one claim
that matters most to the consumer: that GN4 can link this. It has never been
tried, and as shipped it would not work (wrong types on both sides of the seam,
blocking entry point behind an async `Engine`, and no backend that can answer a
real path).

**Verdict: PASS-WITH-FIXES.**

**The single remaining thing:** put `README.md:3-6`'s integration claim on the
same honesty footing as everything else — say, in "What the gate does not
prove", that **GN4 has never compiled against this crate**, and that linking it
needs a GN4-side adapter (two type conversions, a `spawn_blocking` wrapper) that
exists in neither tree and could not produce a hit today even if it existed.

---

## 8 · Residual risks (mine, after this verification)

1. **No populated xapian index exists on this machine**, so the five pre-checks
   in `src/index/xapian.rs` and the phrase requirement are still unexercised
   against a real glass database; the `postlist.glass` check is stricter than
   GN4's `native.rs:551-556` and has never been run against an index GN4 accepts.
2. **The evidence half of the corpus is thin, by construction and by disclosure**:
   5 graded comparisons (2 two-sided) out of 54 shapes, 7 vacuous, 39 skipped; a
   grammar regression in `dataset:`/`group:`/`mean:`(slot 0) prints PASS. Named
   in `docs/CORPUS-RUNNER.md:203-245`; still true.
3. **The green default gate is machine-local**: `DEFAULT_CORPUS` is an absolute
   path into GN4's checkout (`tests/parity_corpus.rs:92`). Now documented
   (`README.md:104-112`), not fixed.
4. **The oldest commit is red under `--features xapian`** (`E0433`,
   `src/lib.rs:120-121`) — disclosed, and unfixable without moving code that
   belongs to the next commit.
5. **`backup-before-rehist` still holds the pre-rewrite history** locally. If
   anything is pushed, the new SHAs are what must be pushed, and finding (3)
   should be fixed first so the committed report does not cite the old ones.
6. **`docs/WORKER-CRATE.md:76-79` and §4.3 remain stale** (27/32 counts, the
   `mean:5` reading) — out of scope for F1–F7, named in the corrections, so not
   silent, but a reader who opens that file alone is still misled.