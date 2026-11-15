# WORKER-REMAINING — the four open items from `docs/VERIFY-GATE-TRUTH.md` §5

Scope: exactly the four items in the task (doc staleness, the feature-vs-evidence
gate, clippy, the phrase class). Nothing else was touched; no code changed except
the mechanical lint fixes.

Toolchain: `~/.rustup/toolchains/1.88.0-x86_64-unknown-linux-gnu/bin` (cargo 1.88.0,
clippy 0.1.88, rustfmt 1.8.0-stable). Every cargo run `--offline` and inside
`timeout 900`. Nothing was built or written under
`/home/kabui/project5/genenetwork4` — the corpus runner only *reads* it.

Commits (one per concern, each made only after fmt · build · test were green):

| commit | concern |
|---|---|
| `e9f8267` | docs correction — the pre-fix red gate in two review reports (item 1) |
| `5c436bb` | clippy — nine lint sites fixed, one recorded as debt (item 3) |
| `566bba2` | docs — the feature build is not the evidence gate; the phrase class (items 2, 4) |
| (this report) | `docs/WORKER-REMAINING.md` |

---

## 1 · DOC STALENESS — **DONE**

Two dated banners, and the false numbers corrected in place next to the argument
they belong to (history stays readable; the falsehood is no longer the first thing
read).

| file:line | what |
|---|---|
| `docs/WORKER-BLOCKERS.md:3-8` | banner: “**SUPERSEDED 2026-11-13 — the xapian gate is green; see `docs/VERIFY-GATE-TRUTH.md` section 1 for the measured table**”, plus the note that §8.4/§8.5 are corrected in place with `[CORRECTED 2026-11-13]` markers |
| `docs/WORKER-BLOCKERS.md:360-367` | §8.4 corrected: the owner decision was taken (`#[cfg(not(feature = "xapian"))]` at `src/index/fixture.rs:394`); `--features xapian` is **exit 0, 38 / 0** lib and **88 / 0** total, not red |
| `docs/WORKER-BLOCKERS.md:373-382` | §8.5 corrected: README’s counts are no longer hand-written (the measured 8×4 table); the `docs/WORKER-CRATE.md:76-79` counts remain stale and are named as such, out of this correction’s scope |
| `docs/VERIFY-FINAL.md:7-12` | banner (same sentence) + the note that §1's red table is the pre-fix measurement |
| `docs/VERIFY-FINAL.md:44-48` | the `--features xapian` → “exit 101, RED” table annotated: current tree is exit 0, 38 / 0 lib, 7 / 0 corpus, 88 / 0 total, `--all-features` identical |
| `docs/VERIFY-FINAL.md:58-62` | the “scratch copy `/tmp/gns_fix`” paragraph annotated: the scratch copy is no longer needed; the six `grammar_render` reds are gone |
| `docs/VERIFY-FINAL.md:76-81` | the “red in **seven** places … not the zero `README.md` claims” paragraph corrected to the current state, keeping the part that still matters (`88 / 0` under the feature is *less* evidence than the default `84 / 0`) |

Verified against the tree, not against the review: `src/index/fixture.rs:394` is the
`#[cfg(not(feature = "xapian"))]` on the `crate::search` half, and
`cargo test --offline --features xapian` is exit 0 at 38 / 0 lib / 7 / 0 corpus.

## 2 · THE FEATURE GATE IS NOT AN EVIDENCE GATE — **DONE**

Both documents now say it in one sentence, and quote both measured corpus summary
lines verbatim.

| file:line | what |
|---|---|
| `README.md:143-168` | new subsection “The default build is the evidence gate; `--features xapian` is not”: “under `--features xapian` the corpus run grades 0 match-set comparisons and the evidence floor does not apply … green there means ‘it builds, and the three recorded error classes still hold’ … **The evidence gate is the default build.**”, followed by the two summary lines |
| `README.md:97` | clippy added to the gate command block (it is now clean) |
| `README.md:35` | `docs/KNOWN-DEBT.md` added to the “What exists” map |
| `README.md:180-187` | “What the gate does not prove” corrected: the old text said clippy “carries a handful of style lints”, which is no longer true |
| `docs/CORPUS-RUNNER.md:18-25` | “The command” corrected: the target “carries evidence only in the DEFAULT build” (the old text said the run *fails* at `fixture_inventory` under the feature — it skips now) |
| `docs/CORPUS-RUNNER.md:326-353` | “Under `--features xapian`” rewritten: reads “**Read this before quoting a green feature run**”, names the two things green means, names the default build as the evidence gate, quotes both runs, and names the mechanism (`FIXTURE_BACKEND`, `tests/parity_corpus.rs:101`) |

The quoted lines, both measured on the delivered tree:

```
# default — the evidence gate: 5 match-set comparisons, floor applied
54 shapes seen: 5 graded a set-equality/Jaccard comparison, 3 graded an error class, 7 VACUOUS (expected set empty: no evidence), 39 SKIP (the missing xapian backend's, see docs/CORPUS-RUNNER.md); 8 PASS, 0 FAIL
exit rule: any FAIL fails the test, and so does fewer than MIN_GRADED = 5 graded comparisons; PASS, VACUOUS and SKIP alone are not evidence

# --features xapian — 0 match-set comparisons, the floor is NOT applied
54 shapes seen: 0 graded a set-equality/Jaccard comparison, 3 graded an error class, 0 VACUOUS (expected set empty: no evidence), 51 SKIP (the missing xapian backend's, see docs/CORPUS-RUNNER.md); 3 PASS, 0 FAIL
no fixture backend under this feature: 51 shapes SKIP(no-fixture-backend-under-this-feature), 0 graded; the evidence floor MIN_GRADED = 5 is not applied to this run
```

## 3 · CLIPPY — **DONE (0 warnings; 1 lint silenced at its site and argued in KNOWN-DEBT)**

The brief counted 13 findings; clippy reports them per target, so the count of
warning *lines* is larger than the count of places to edit. Deduplicated, there
were **ten sites**: six in the lib target, three in the test targets, and one
(`duplicated_attributes`) that only the `xapian` feature compiles — **nine fixed,
one allowed**. `docs/KNOWN-DEBT.md` §2 records the same count, so the two
documents cannot drift.

`cargo clippy --offline --all-targets` prints **0** warnings, in all four feature
combinations, and 0 from a cold target dir with `--all-features`.

| file:line | lint | fix |
|---|---|---|
| `src/error.rs:82-90` | `collapsible_match` | `Display` is one `match`: `Self::InvalidQuery { xapian_error_type: Some(t), .. }` → `"{msg} ({t})"`, `_` → `"{msg}"`. Same output for all three cases (`tests/error_status.rs`, 4/0) |
| `src/grammar.rs:659-665` | `manual_unwrap_or_default` | the unparseable-position branch is `parse_position(..).unwrap_or_default()`; the comment explaining why “no bounds” means “match nothing” moved above it |
| `src/grammar.rs:858-862` | `type_complexity` | **not fixed** — `#[allow(clippy::type_complexity)]` on `parse_range`, with the reason in a comment and in `docs/KNOWN-DEBT.md:13-49` (the nesting is the three-state contract `Ok(None)` / `Ok(Some((None, None)))` / `Err`; a `type` alias only renames it; the honest fix is a bounds struct that would refactor `parse_range` + `parse_position` + the `ValueRange` construction, which is backend work, not a lint sweep) |
| `src/index/fixture.rs:166-169, 242, 248` | `unnecessary_map_or` ×3 | `map_or(false, …)` → `is_some_and(…)` (stable since 1.70, the crate’s `rust-version`) |
| `src/index/xapian.rs:28-31` | `duplicated_attributes` | the inner `#![cfg(feature = "xapian")]` is gone; the module is gated once on its declaration (`src/index/mod.rs:23`) |
| `tests/manifest.rs:10, 26, 31` | `ptr_arg` ×2 | `&PathBuf` → `&Path` (+ `use std::path::{Path, PathBuf}`) |
| `tests/parity_corpus.rs:864` | `uninlined_format_args` | `MIN_GRADED = {}` + positional arg → `MIN_GRADED = {MIN_GRADED}` |
| `docs/KNOWN-DEBT.md` (new) | — | the one survivor, its reason, the rejected alternatives, and a list of the nine that were fixed, so the file cannot become a dumping ground |

The clippy run is reproducible as “clean or justified”: `grep -rn KNOWN-DEBT src/
tests/` returns exactly one site, and it is the row above.

## 4 · THE PHRASE CLASS — **DONE**

New section `docs/CORPUS-RUNNER.md:253-322`, “What this runner cannot grade and
why”, which states it as the crate’s main representational limit (every other gap
is “the fixture is a double”; this one is “the seam cannot say it”):

* **the class** (`:263-275`): quoted regions (`"alcohol dependence"`), positional
  operators (`NEAR`/`ADJ`), and field tokens the grammar does not consume as ranges
  (`year:2014` → the phrase `year 2014`) — with the note that the marker
  (`RenderedTerm::phrase`, DECISIONS rows 34/36) makes the adjacency *expressible*
  but nothing in this crate evaluates it;
* **the evidence** (`:278-299`): the positionless index (`genenetwork4/docs/
  search-engines.md:98-102`), **Q04, Q11 and Q12 each recorded 0 hits** (re-read from
  the corpus JSON for this report), 0 is `VACUOUS` by the runner’s own rule, and
  grading it would bless exactly the AND-of-words query the marker exists to
  prevent; 3 of the 54 shapes are skipped by name `phrase-not-expressible`
  (`is_phrase_shape`, `tests/parity_corpus.rs:396`), and the field-token branch is
  reached only by the constructed Q55/Q56 in
  `tests/parity_corpus.rs::a_phrase_shape_is_skipped_as_unrepresentable`;
* **what guards it today** (`:301-312`): a table of the three tests
  (`a_bare_field_value_is_a_phrase_not_an_and`, `a_quoted_value_is_never_a_range`,
  `a_phrase_shape_is_skipped_as_unrepresentable`) and, one column over, what each
  does *not* prove — “the phrase class has a shape and no oracle”;
* **the requirement** (`:315-322`): “handing the phrase-marked terms to xapian’s own
  parser as ONE phrase — never as an AND of words” (`docs/DECISIONS.md` row 35), and
  the reason a wider fixture cannot close the gap: the positionless layout keeps
  answering 0.

No new code: the SKIP naming already existed. `README.md:166-167` and
`docs/CORPUS-RUNNER.md:249-251` point at the new section.

---

## The full gate after the changes (measured, not carried over)

| check | result |
|---|---|
| `cargo fmt --check` | **exit 0** (clean) |
| `cargo build --offline` | **0** warnings / 0 errors |
| `cargo test --offline` | **84 / 0** — lib 32, bin 0, error_status 4, grammar_render 26, hit_roundtrip 6, manifest 7, parity_corpus 9, doc 0 |
| `cargo test --offline --features serve` | **84 / 0** (same eight targets) |
| `cargo test --offline --features xapian` | **88 / 0** — lib **38**, parity_corpus **7** |
| `cargo test --offline --all-features` | **88 / 0** (identical to `--features xapian`) |
| `cargo clippy --offline --all-targets` | **0** warnings |
| `cargo clippy --offline --all-targets --features serve` | **0** | 
| `cargo clippy --offline --all-targets --features xapian` | **0** |
| `cargo clippy --offline --all-targets --all-features` | **0** (also 0 from a cold `CARGO_TARGET_DIR`) |
| corpus, default build | `54 shapes seen: 5 graded a set-equality/Jaccard comparison, 3 graded an error class, 7 VACUOUS …, 39 SKIP; 8 PASS, 0 FAIL` |
| corpus, `--features xapian` | `54 shapes seen: 0 graded …, 3 graded an error class, 0 VACUOUS …, 51 SKIP; 3 PASS, 0 FAIL` + the “floor not applied” line |

All eight totals match the table already in `README.md:110-120` and the counts in
`docs/VERIFY-GATE-TRUTH.md` §1, so no number in this pass’ docs contradicts a
measured run. The lint sweep moved no test count: **84 / 88 before, 84 / 88 after.**

## Residual risks / what this pass did not do

1. **`docs/WORKER-CRATE.md:76-79` is still stale** (27 default / 32 with
   `--features xapian`, 14 and 5 in the test files). Named in the
   `WORKER-BLOCKERS.md` §8.5 correction rather than fixed, because it is a
   measurement table in a report, not the gate definition a maintainer reads first.
2. **`docs/WORKER-CRATE.md:120-122` still carries the pre-F2 reading** of `mean:5`
   (point range `5..5`) and `mean:` (a 422). Same reason; flagged here so it is not
   lost.
3. **`README.md:28` still says “the three pre-checks”** for a module that makes five
   (`VERIFY-GATE-TRUTH.md` V-5, a nit). Out of the four items’ scope.
4. **The one silenced lint is real debt**: `parse_range`’s nested `Option` type is
   allowed, not fixed (`docs/KNOWN-DEBT.md`). If a reviewer prefers a `type` alias
   or a bounds struct, that is a deliberate change to the rendering seam, not a
   warning sweep — KNOWN-DEBT says so and names what the struct would touch.
5. **The phrase class still has no oracle**, by construction: 0 recorded hits on a
   positionless index means only a populated index with positions can verify it. The
   new doc says this instead of implying the gap is a fixture limitation.
6. **`git status` was clean before each commit and is clean now**; nothing staged
   and left behind.
