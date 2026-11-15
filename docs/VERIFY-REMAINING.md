All measurements are done. Here is the artifact for `/home/kabui/gn-search/docs/VERIFY-REMAINING.md` (no write-capable tool was available to this session, so it is returned inline for the runtime to persist).

---

# VERIFY-REMAINING — independent check of the four-item pass

Reviewer: `reviewer-alt` (non-GLM family), independent of the author. Everything
below was **run** on this tree, not read off a report.

Toolchain: `~/.rustup/toolchains/1.88.0-x86_64-unknown-linux-gnu/bin` (cargo /
rustc 1.88.0, clippy 0.1.88, rustfmt 1.8.0-stable), every cargo invocation
`--offline` inside `timeout 900`, every run with an **out-of-tree**
`CARGO_TARGET_DIR` (`/tmp/gnv-*`) so no file in the repository changed and
nothing was built inside `/home/kabui/project5/genenetwork4` (it is only read,
by the corpus runner). `git status --porcelain` is empty before and after.

Method: `git show` for every commit (docs corrections checked line-by-line for
deleted original text), `git archive <sha> | tar -x` into `/tmp` for the
per-commit gate, and a full cold-measured gate on `HEAD`.

---

## 1 · The four items

Source of the four: `docs/VERIFY-GATE-TRUTH.md` §5 residual risks 1, 3, 4, 5
(feature-vs-evidence gate, the phrase class, docs staleness, clippy).

### 1 · DOC STALENESS — **DONE**

No original text was rewritten. `git show e9f8267` is **additions only** (47
`+`, 0 `-`): two dated banners plus `[CORRECTED 2026-11-13]` blocks appended
under the finding they correct.

| file:line | what is there now |
|---|---|
| `docs/WORKER-BLOCKERS.md:3-7` | SUPERSEDED banner, points at `VERIFY-GATE-TRUTH.md` §1, states the §8.4/§8.5 correction convention |
| `docs/WORKER-BLOCKERS.md:360-368` | §8.4 corrected: `#[cfg(not(feature = "xapian"))]` at `src/index/fixture.rs:394`; `--features xapian` is 38 / 0 lib, 88 / 0 total — the red numbers remain above it as history |
| `docs/WORKER-BLOCKERS.md:373-381` | §8.5 corrected: README counts are the measured table; `WORKER-CRATE.md:76-79` and §4.3 named as still stale, out of scope |
| `docs/VERIFY-FINAL.md:7-12` | banner, same sentence |
| `docs/VERIFY-FINAL.md:44-48`, `:58-62`, `:76-81` | three in-place corrections; the pre-fix red tables stay |

Cross-checked against the tree, not the review: `src/index/fixture.rs:394` is
the `#[cfg(not(feature = "xapian"))]` on the `crate::search` half, and
`cargo test --offline --features xapian` is exit 0 at 38 / 0 lib and 7 / 0
corpus — exactly what the corrections claim. The line windows cited in
`docs/WORKER-REMAINING.md` §1 all land on the text they describe (spot-checked
all twelve; `:3-8` and `:360-367` are off by one, harmless).

### 2 · THE FEATURE GATE IS NOT AN EVIDENCE GATE — **DONE** (prose), with two defects in the print it quotes

- `README.md:143-168`: the subsection exists, says "the corpus run grades 0
  match-set comparisons and the evidence floor does not apply", quotes both runs.
- `docs/CORPUS-RUNNER.md:18-25` and `:326-353`: same statement, plus the
  mechanism (`FIXTURE_BACKEND`, `tests/parity_corpus.rs:101`).
- `README.md:97` adds clippy to the gate block; `README.md:181-187` no longer
  claims "a handful of style lints".
- Both quoted summary lines reproduce the runs **exactly** (compared
  byte-for-byte against `--nocapture` output, below).

Two defects in the lines quoted, both reported as findings (F2, F3).

### 3 · CLIPPY — **DONE**

`cargo clippy --offline --all-targets` prints **0** warnings on `HEAD` in all
four feature combinations, from cold target dirs. The pre-fix tree
(`e9f8267`) reports **9 distinct lints** (3 `unnecessary_map_or`, 2 `ptr_arg`,
1 `type_complexity`, 1 `uninlined_format_args`, 1 `collapsible_match`,
1 `manual_unwrap_or_default`), plus the feature-only `duplicated_attributes` =
10 sites; 9 fixed, 1 `#[allow]`ped at `src/grammar.rs:862` and argued in
`docs/KNOWN-DEBT.md:14-49`. `grep -rn KNOWN-DEBT src/ tests/` returns exactly
one hit (`src/grammar.rs:861`), matching the documented convention.

The nine fixes are behaviour-preserving — I read each: `map_or(false, f)` →
`is_some_and(f)` on `Option<&[T]>`/`Option<&str>` is the same predicate;
`unwrap_or_default()` on `Option<(Option<f64>, Option<f64>)>` yields
`(None, None)`, the same value the deleted `None => (None, None)` arm produced
(`src/grammar.rs:656-665`); the collapsed `Display` arm
(`src/error.rs:83-90`) writes the identical strings; dropping the inner
`#![cfg(feature = "xapian")]` is safe because the module is gated on its
declaration (`src/index/mod.rs:23-24`) and the feature build compiles 38 / 0.

### 4 · THE PHRASE CLASS — **DONE**

`docs/CORPUS-RUNNER.md:253-322` exists and its claims verify:

- Q04/Q11/Q12 each record `"hit_count": 0` in the corpus JSON (re-read
  directly), and each prints `SKIP(phrase-not-expressible…)` — exactly 3 of the
  54 shapes, measured.
- The three governing tests named in the guards table all exist, and the
  field-token branch is reached only by the constructed Q55/Q56
  (`tests/parity_corpus.rs:1167-1168`).
- The cited `genenetwork4/docs/search-engines.md:98-102` does say quoted /
  `NEAR` / `ADJ` queries match zero documents on a positionless index.

---

## 2 · The gate after the work (measured on `HEAD`, cold)

| check | result |
|---|---|
| `cargo fmt --check` | exit **0** |
| `cargo build --offline` | exit **0**, **0** warnings |
| `cargo test --offline` | **84 / 0** — lib 32, bin 0, error_status 4, grammar_render 26, hit_roundtrip 6, manifest 7, parity_corpus 9, doc 0 |
| `cargo test --offline --features serve` | **84 / 0** |
| `cargo test --offline --features xapian` | **88 / 0** — lib **38**, parity_corpus **7** |
| `cargo test --offline --all-features` | **88 / 0** |
| `cargo clippy --offline --all-targets` | **0** warnings (×4 feature combos, cold) |
| corpus, default | `54 shapes seen: 5 graded a set-equality/Jaccard comparison, 3 graded an error class, 7 VACUOUS …, 39 SKIP …; 8 PASS, 0 FAIL` |
| corpus, `--features xapian` | `54 shapes seen: 0 graded …, 3 graded an error class, 0 VACUOUS …, 51 SKIP …; 3 PASS, 0 FAIL` + `no fixture backend under this feature: 51 shapes SKIP(no-fixture-backend-under-this-feature), 0 graded; the evidence floor MIN_GRADED = 5 is not applied to this run` |

Every number in `README.md:110-120` and in `WORKER-REMAINING.md` §"The full
gate" matches what I measured. Zero clippy warnings, so nothing needs a
`KNOWN-DEBT` row beyond the one that is there and argued.

---

## 3 · The commits

### Message style — PASS, all eleven

Every subject is `* path (entity): what changed.` at ≤ 72 characters (max 72,
`2ebbfa4` and `0b86e57`), every body is the *why*, no attributions, no date
stamps, no chat text.

### Does each commit pass the gate on its own tree — NO, two do not compile

`git archive <sha> | tar -x -C /tmp/gnv-commits/<sha>`, then `cargo test --offline`:

| commit | build+test | fmt --check | note |
|---|---|---|---|
| `caf6c02` | **FAIL, exit 101** | **FAIL** | `error[E0583]: file not found for module 'grammar'` (`src/lib.rs:51`) and `'manifest'` (`src/lib.rs:54`); fmt cannot resolve `src/grammar.rs` |
| `0b86e57` | **FAIL, exit 101** | **FAIL** | `error[E0583]: file not found for module 'manifest'` (`src/lib.rs:54`); fmt cannot resolve `src/index/xapian.rs` |
| `3028835` | ok, 65 / 0 | ok | clippy still red (pre-fix) |
| `e1f661e` | ok, 84 / 0 | ok | clippy 9 distinct warnings |
| `2ebbfa4` | ok, 84 / 0 | ok | |
| `6c00fd2` | ok, 84 / 0 | ok | |
| `8504e69` | ok, 84 / 0 | ok | |
| `e9f8267` | ok, 84 / 0 | ok | clippy 9 distinct warnings (measured) |
| `5c436bb` | ok, 84 / 0 | ok | clippy 0 — the documented gate starts here |
| `566bba2` | ok, 84 / 0 | ok | |
| `12d36fc` | ok, 84 / 0 (88 / 0 with `xapian`) | ok | HEAD, all four columns measured |

Cause: the module declarations were committed in the *first* commit
(`src/lib.rs:51,54`, `src/index/mod.rs:24`) while the module bodies arrive two
commits later (`src/grammar.rs` in `0b86e57`; `src/manifest.rs` and
`src/index/xapian.rs` in `3028835`). The seven commits stamped
`2026-11-15 20:33:05` were therefore created from one final tree in a batch,
not one concern at a time with a gate between them — the report's "each made
only after fmt · build · test were green" is true only for the three commits of
*this* pass (`e9f8267`, `5c436bb`, `566bba2`), all of which do pass.

### Independently revertible — NO, for `3028835`

Reverting `3028835` alone deletes `src/manifest.rs` and `src/index/xapian.rs`
while `src/lib.rs:54` / `src/index/mod.rs:24` still declare them → same E0583.
Same root cause as F1. `5c436bb` reverts cleanly but leaves `README.md` (from
`566bba2`) citing a `docs/KNOWN-DEBT.md` that the revert removes — a dangling
reference, not a build break.

### Mixed concerns

`566bba2` carries **two** of the four items: item 2 (the feature-gate wording,
`README.md:143-168`, `CORPUS-RUNNER.md:18-25`) and item 4 (the whole phrase-class
section, `CORPUS-RUNNER.md:253-322`, 70 of its 127 added lines). Both are docs
and the message names both, so it is defensible, but it is the one commit a
strict one-concern-per-commit rule would split. No other commit mixes concerns.

---

## 4 · The judgement

**Can a maintainer pick it up cold?** Mostly. `README.md` is true where it
matters (the measured table, what is stubbed, what the gate does not prove), the
debt is named (`docs/KNOWN-DEBT.md`), and the corpus runner's panic message
tells you how to point it at a corpus. Three things still bite:

1. The README now quotes, as "verbatim" evidence, a summary line that attributes
   the skips to "the missing xapian backend's" in the paragraph about the build
   where xapian *is* compiled in (F2). That is the same defect class the earlier
   independent review flagged as V-1 and said it would not ship without; this
   pass promoted the line into the README instead of fixing it.
2. `docs/CORPUS-RUNNER.md:358-366` ("How to widen it") still says the runner
   "needs no change" for a real backend, while `FIXTURE_BACKEND =
   cfg!(not(feature = "xapian"))` (`tests/parity_corpus.rs:101`) means the day
   that backend lands the corpus still grades 0 (the earlier review's V-2,
   also still open).
3. `README.md:28` still says "the three pre-checks" for a five-check module
   (`src/index/xapian.rs:79-116`), contradicting the same README's own "What is
   stubbed" four paragraphs later. Named in `WORKER-REMAINING.md` risk 3, which
   is honest but is a one-word fix that was left.

**The single thing that would still embarrass us in review:** F2 — the README
pastes `… 51 SKIP (the missing xapian backend's, …)` directly under the sentence
"in that build the fixture … is not compiled in". A reviewer reads the prose and
the quoted line in the same paragraph and they disagree, on the one axis (which
component is missing) that the whole pass was about.

**Verdict: PASS-WITH-FIXES.**

---

## Findings

| # | sev | file:line | why | smallest fix |
|---|---|---|---|---|
| F1 | **medium** | `src/lib.rs:51,54` (commit `caf6c02`), `src/index/mod.rs:24` (same); `src/manifest.rs`, `src/index/xapian.rs` (commit `3028835`) | Two of eleven commits do not compile on their own tree (`E0583`): the `mod` declarations precede the files. `git bisect` lands on a build error, and reverting `3028835` alone breaks the build. The seven `20:33:05` commits were cut from one final tree, so no gate ran between them. | Local, unpushed history: rewrite the two snapshots so each `mod` lands in the commit that adds its file (or squash `caf6c02`+`0b86e57`+`3028835` and re-split by file). Then re-run the per-commit gate loop above. |
| F2 | low | `tests/parity_corpus.rs:853` (print) and `:37-38` (module doc); quoted at `README.md:160`, `docs/CORPUS-RUNNER.md:345` | Under `--features xapian` the skips are the **fixture's** absence, but the line says "the missing xapian backend's" — in the build where xapian is compiled in. The new docs quote it as evidence, so the wrong attribution is now canonical. | Make the parenthetical conditional: `if FIXTURE_BACKEND { "the missing xapian backend's…" } else { "the missing fixture backend's…" }`, reword `:37-38`, re-quote the line in both docs. |
| F3 | low | `tests/parity_corpus.rs:863-867` | The "exit rule … fewer than MIN_GRADED = 5 graded comparisons" line prints unconditionally, so under `--features xapian` the report states a floor that the same function then says "is not applied". Both docs quote the feature run "verbatim" while dropping exactly that line, so the quote hides the contradiction. | Move the print inside `if FIXTURE_BACKEND`, or append "(default build only)"; then the block can be quoted whole. |
| F4 | low | `README.md:28` | "the three pre-checks" for a five-check module; contradicts `README.md:41-50` in the same file. Already named as residual risk 3, still false. | "the pre-checks". |
| F5 | low | `docs/CORPUS-RUNNER.md:361-365` | "The runner needs no change for that — only a way to point `FIXTURE_PATH` at an index" is false while `FIXTURE_BACKEND = cfg!(not(feature = "xapian"))` (`tests/parity_corpus.rs:101`): a real backend arrives with the feature on, the const stays `false`, every recorded-200 shape short-circuits to `SKIP` and the floor stays off. Prior review's V-2, still open; the new section nearby states the mechanism but this paragraph was not touched. | Append: "…and drop the `FIXTURE_BACKEND` gate, which today keys *can grade* to the absence of the `xapian` feature." |
| F6 | nit | `README.md:88-120` | "The gate is `cargo test`, offline" and the 84 / 0 table are only true where GN4's tree exists at `DEFAULT_CORPUS` (`tests/parity_corpus.rs:88`), an absolute path; on any other machine `parity_corpus` panics and the default gate is red out of the box. | One sentence in "Running the tests": the parity target needs the corpus at that path or `GN4_PARITY_FIXTURES`. |
| F7 | nit | `docs/VERIFY-FINAL.md:232-236`, `:258-262` | F4 and the verdict still assert README staleness ("Not with `cargo`", "27 unit tests") as current fact; §1 got in-place markers, these did not, so the flagging is inconsistent inside one file. | Add a one-line `[CORRECTED …]`: F4 is fixed in `README.md:104-120`. |

No logic defect, no silent behavioural drift: the only code this pass touched
was nine lint sites, each verified equivalent, and the 84 / 88 totals are
unchanged before and after (`e9f8267` 84 / 0 → `HEAD` 84 / 0).

## Residual risks (mine, after this check)

1. **No populated xapian index exists here**, so the stub's pre-checks and the
   phrase-class requirement are still unexercised against a real glass database.
2. **The phrase class has no oracle** — correct by construction (0 recorded
   hits), and the docs now say so rather than implying a fixture gap remains.
3. **The corpus is an absolute path into another checkout** (F6); the crate's
   green gate is machine-local until that is parameterised.
4. **`docs/WORKER-CRATE.md:76-79` and §4.3 remain stale** (27/32 test counts,
   the `mean:5` reading). Named in the corrections and in
   `WORKER-REMAINING.md`, so not silent, but a reader who opens that file alone
   is still misled.
5. **History is not bisectable as delivered** (F1) until the two snapshots are
   rewritten or the split is re-cut.