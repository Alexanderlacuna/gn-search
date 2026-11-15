Verification complete. Report below.

---

# VERIFY-FINAL — `gn-search`, verified by running

> **SUPERSEDED 2026-11-13 — the xapian gate is green; see `docs/VERIFY-GATE-TRUTH.md`
> section 1 for the measured table.** This review measured the tree *before* the gate-truth
> fix, so §1's `--features xapian` result (exit 101, red in seven places) and the six
> `grammar_render` reds it reports are the pre-fix state. The red numbers are kept below as
> history and corrected in place, marked `[CORRECTED 2026-11-13]`, so the finding F1 that
> drove the fix still reads.

Reviewer: `reviewer-alt` (non-GLM family). Everything below was **executed on this machine**, not read.
Toolchain: `~/.rustup/toolchains/1.88.0-x86_64-unknown-linux-gnu/bin/{cargo,rustc}` (cargo 1.88.0 / rustc 1.88.0),
all runs `timeout 900 … cargo … --offline`. `--offline` is the only deviation from the literal command: the
`serde`/`serde_json` pins are in `~/.cargo/registry`, so nothing was fetched and nothing was vendored. **No GN4
rlibs were needed** — `cargo` resolves and links from the local registry cache. Nothing was written inside
`/home/kabui/project5/genenetwork4` (reads and `SELECT`-only mysql). Workspace source files are unchanged
(`find … -newer /tmp/gs-out.log` → empty); only `target/` was populated by the builds, plus scratch copies
under `/tmp/gns_mut_{a,b,c,d}` and `/tmp/gns_fix`.

## 1 · Build and test — exact counts

`cargo test --offline` (default features), exit **0**:

| target | passed | failed |
|---|---|---|
| `unittests src/lib.rs` | 32 | 0 |
| `unittests src/bin/gn-search.rs` | 0 | 0 |
| `tests/error_status.rs` | 4 | 0 |
| `tests/grammar_render.rs` | 26 | 0 |
| `tests/hit_roundtrip.rs` | 6 | 0 |
| `tests/manifest.rs` | 7 | 0 |
| `tests/parity_corpus.rs` | 9 | 0 |
| doc-tests | 0 | 0 |
| **total** | **84** | **0** |

`cargo test --offline --features serve`: identical 84/0, all eight targets ok (the `serve` binary compiles and
its unit target is empty).

`cargo test --offline --features xapian` → **exit 101, RED**:

> **`[CORRECTED 2026-11-13]`** — this run measured the pre-fix tree. The same command on the
> current tree is **exit 0**: `unittests src/lib.rs` **38 / 0**, `tests/parity_corpus.rs`
> **7 / 0**, every other target unchanged, **88 / 0** in total, `--all-features` identical.
> The pre-fix red table is kept as the record of what F1 below was about. Measured per-target
> table: `docs/VERIFY-GATE-TRUTH.md` §1.

| target | passed | failed |
|---|---|---|
| `unittests src/lib.rs` | 36 | **1** — `index::fixture::tests::a_non_empty_index_path_is_refused` |
| (cargo aborts after the `--lib` failure) | | |

`--all-features` (`serve,xapian`) with that one test gated in a scratch copy (`/tmp/gns_fix`) so the other
targets could run:

> **`[CORRECTED 2026-11-13]`** — the scratch copy is no longer needed: the `crate::search` half
> of the failing test is `#[cfg(not(feature = "xapian"))]` in the delivered tree
> (`src/index/fixture.rs:394`), the six `grammar_render` reds are gone (26 / 0 under the feature),
> and `tests/parity_corpus.rs` is 7 / 0 because the two inventory-gated tests are compiled out
> rather than failing. The table below is the pre-fix measurement.

| target | passed | failed |
|---|---|---|
| `src/lib.rs` | 37 | 0 |
| `tests/error_status.rs` | 4 | 0 |
| `tests/grammar_render.rs` | 20 | **6** |
| `tests/hit_roundtrip.rs` | 6 | 0 |
| `tests/manifest.rs` | 7 | 0 |
| `tests/parity_corpus.rs` | 6 | **3** (documented: `CORPUS-RUNNER.md`, default-feature-only) |

So the `xapian` feature build is red in **seven** places, not the one `docs/WORKER-BLOCKERS.md` §8.4 records and
not the zero `README.md` claims. The six `grammar_render` reds are new information — see finding **F1**.

**`[CORRECTED 2026-11-13]`** — both of those readings are now the opposite way round: the feature build is
**green** (`88 / 0`, exit 0, all four columns measured), §8.4 and §8.5 of `WORKER-BLOCKERS.md` carry their own
correction banners, and the `README.md` it called wrong is now the measured table. What survives the fix is the
part that mattered: `88 / 0` under `--features xapian` is **less** evidence than the default `84 / 0` — 0
match-set comparisons, the floor not applied (`README.md`, "Expected results of that gate";
`docs/CORPUS-RUNNER.md`, "Under `--features xapian`").

## 2 · The corpus runner

`cargo test --offline --test parity_corpus -- --nocapture`, **exit 0**. Summary line, verbatim:

```
54 shapes seen: 5 graded a set-equality/Jaccard comparison, 3 graded an error class, 7 VACUOUS
(expected set empty: no evidence), 39 SKIP (the missing xapian backend's, see docs/CORPUS-RUNNER.md);
8 PASS, 0 FAIL
```

| bucket | n | ids |
|---|---|---|
| graded set-eq, PASS | 5 | Q15 (5 hits), Q29 (5), Q31 (2), Q36 (2), Q37 (5) |
| graded error-class, PASS | 3 | Q42 `InvalidQuery(InvalidOperationError)`, Q51/Q52 `InvalidQuery(QueryParserError)` |
| VACUOUS | 7 | Q16, Q17, Q19, Q28, Q30, Q38, Q39 |
| SKIP | 39 | the rest |

I recomputed the graded sets from the raw corpus JSON rather than trusting the PASS column (`identifiers ∩
fixture's 5`): Q15/Q29/Q37 → all five `C57BL-6JxC57BL-6NJF2Publish` traits; Q31 → `10199,10201`; Q36 →
`10200,10211`; Q19 → 0 of 13 757. That matches `docs/CORPUS-RUNNER.md` and `MUST_GRADE` exactly, and the
corpus is **not** truncated (`stored_ids == hit_count` for every shape). Two of the five are two-sided, as
claimed.

## 3 · Is there a real xapian index on this machine? **No.**

**16 directories with an `iamglass` marker** exist (`/home/kabui/spikes/**`, plus 14 stale
`/tmp/gn-search-xapian-*-glass`). Every one of them is an **orphaned shell**: `iamglass` + `flintlock` and
nothing else (8 KB). Proof, not inference — `xapian-delve` 1.4.19 from the guix store:

```
$ xapian-delve /home/kabui/spikes/index-self-build/xapian-index
Error opening database '…': DatabaseOpeningError: Couldn't open
…/xapian-index/postlist.glass to read (No such file or directory)
```

All 16 → `openable=NO`; no `postlist.glass`/`record.glass`/`docdata.glass` exists anywhere under
`/home/kabui /srv /var /tmp /opt`. **There is no populated index**, so there is no way to answer
"does a query return hits, do the counts look sane" — and I will not invent an answer.

There is a `.sidecar`-free `XAPIAN_DB_PATH` in **no** `.env*` (`genenetwork4` has none) and no
`~/.config/gn4/*` reference to xapian.

**What *is* on the machine (and it is more than the docs assume):**

* **libxapian**: `libxapian.so.30` (system) + guix `xapian-1.4.19` and `1.4.29` with `xapian-delve`,
  `xapian-check`, `xapian-compact`, `xapian-config`.
* **The xapian-rs fork**: `~/.cargo/git/checkouts/xapian-rs-b6a39ddc92bfd2ad/70c5eac` — the binding the stub
  says is missing is *vendored in the cargo git cache*.
* **A reachable database.** MariaDB 10.6 is listening on `127.0.0.1:3306`, socket `/run/mysqld/mysqld.sock`,
  `db_webqtl_local` present: 70 publish datasets, `PublishXRef` 20 982, `GeneRIF_BASIC` 1 671 827,
  `Phenotype` 21 192, `ProbeSet`/`ProbeSetXRef` **exactly 100 000** (an aborted load — `MAX(ProbeSet.Id)=100000`,
  gene side truncated).
* **The corpus's own dataset is in it.** `PublishFreeze` Id 21 = `C57BL-6JxC57BL-6NJF2Publish`, `InbredSetId` 43,
  224 traits. The five fixture documents are there **row for row**:
  `10199 mean=3.0200798533775886 additive=0.377068891450584 LRS=6.62875131299756` — identical to
  `src/index/fixture.rs`. That is an independent corroboration of the fixture data I did not expect to get.
* A previously **successful** full build is logged: `spikes/index-self-build/index-build2.log` ends
  `INFO: Index built … Time to Index: 0:02:21`, against this same DB via
  `mysql://webqtlout:webqtlout@localhost/db_webqtl_local?unix_socket=/run/mysqld/mysqld.sock`. Its output
  directory is now empty — the tables were removed (parent dirs all `Okt 24 10:45`).

**What blocks the crate from using an index** (both features compile: `--features serve,xapian` builds clean):
the backend is a stub, so there is no code path to point anywhere. Demonstrated live, pointed at the
"glass-shaped" directory above:

```
$ GN_SEARCH_ADDR=127.0.0.1:18083 XAPIAN_DB_PATH=/home/kabui/spikes/index-self-build/xapian-index \
    ./target/debug/gn-search
$ curl '…/search?query=alcohol&type=phenotype&limit=2&page=1'
HTTP 000   (curl exit 52, empty reply)
# server log:
thread '<unnamed>' panicked at src/index/xapian.rs:43:9:
not implemented: … The index at /home/kabui/spikes/index-self-build/xapian-index passed the pre-checks;
the next step is Database::open. …
```

Two things to note: the stub says the directory **passed the pre-checks** about a directory `xapian-delve`
cannot open at all (finding **F3**), and a panicking handler yields a dropped connection rather than a 5xx body.

**To produce a real index** you need one populated glass DB. The ingredients are present; the missing piece is
the indexer binary, not the data: `gn-indexer full` needs `SQL_URI` **and** a build of the crate that carries
`xapian_rs` (no `target/debug/gn-indexer` exists; building GN4 is out of bounds here), or the gn3 python route
that already worked (`spikes/index-self-build/index-genenetwork-batched` + the guix `genenetwork3` shell + the
171 MB `rdf-cache.json`, which is still on disk). Caveat before anyone spends the 2–20 minutes: an index built
from this DB is **not** a parity source (2025-02-18 snapshot, genes truncated at 100 k), and the corpus is a
production recording — it would widen the *fixture*, not replace the production index.

## 4 · The three blockers, judged, and the one answer

Proving lines verified in place (line numbers drift by ≤3 from `docs/WORKER-FINAL.md`):

| # | verdict | proving line | my check |
|---|---|---|---|
| **B-A** quote state is per-QUERY | **FIXED** | `src/grammar.rs:803` `quoted_tokens` (one `in_quotes` walk, token tagged by its first char), consumed at `:579`; `parse_range:868` `value.split('"').next()` | Reverted both halves in `/tmp/gns_mut_a` (`q.split_whitespace().map(|t|(t,false))` + `if value.starts_with('"') { return Ok(None) }`): **exactly one** test fails — `tests/grammar_render.rs:248::a_range_looking_token_inside_a_quote_is_not_a_422`. The fix does what it says. |
| **B-B** phrase honesty | **FIXED (honesty only)** | `src/grammar.rs:328` `pub phrase: bool`, set at `:546`/`:835`; skipped by name at `tests/parity_corpus.rs:385` `is_phrase_shape` | Reverted the two `text_words(…, true)` calls at `src/grammar.rs` to `false` in `/tmp/gns_mut_b`: **exactly two** tests fail — `grammar_render.rs:179`, `parity_corpus.rs:1079`. The class is now expressible and named; it is still ungraded, which the docs say. |
| **B-C** must_grade ids | **FIXED** | `tests/parity_corpus.rs:85` `MUST_GRADE`, enforced in `evidence_shortfall:537` (named layer) on top of `MIN_GRADED:70` | Deleted the named layer in `/tmp/gns_mut_c`: **`corpus_report` still passes** with the count satisfied, and only `parity_corpus.rs:993` fails. The layer is real and the id list matches the live report. |

**Can the suite pass while a real regression is present? Yes — I made it happen three times.** The corpus
runner is not the net the docs imply; the six named unit tests are, one per blocker.

The path, quoted: `tests/parity_corpus.rs:667-680` — every 200-shape is either compared or
`row(…, Graded::Nothing, Verdict::Skip(reason))`, and `summarize:500-517` counts only
`row.is_graded_evidence()` toward the floor. A renderer regression that lands in any of the 39 SKIPped classes
changes no verdict and no count:

* `/tmp/gns_mut_a` (B-A fully reverted — a spurious 422 restored on `"alcohol mean:abc..def"`, a quoted range
  no longer a range): `corpus_report … ok`, report `8 PASS, 0 FAIL`, exit 0.
* `/tmp/gns_mut_b` (B-B reverted — `year:2014` an AND of words again): `corpus_report … ok`, `8 PASS, 0 FAIL`.
* `/tmp/gns_mut_c` (B-C's named layer deleted): `corpus_report … ok`, `8 PASS, 0 FAIL`.

The runner *does* bite where it claims: `/tmp/gns_mut_d`, `peak` → slot 4, prints
`Q31 | peak:5..15 | phenotype | 11931 | 0 hits | set-eq | FAIL(…)` and `1 shape(s) diverged: ["Q31: …"]`,
exit 1. So: green-with-a-regression is possible exactly for the **quoted/phrase/operator/free-text** classes and
for any regression that removes the guard itself; it is not possible for a wrong value slot on a graded shape.
`docs/CORPUS-RUNNER.md:195` says "Proved: the grammar rejects exactly the classes GN4 rejects **and nothing
more**" — that sentence is falsified by `gns_mut_a`; the corpus has no quoted-range shape to see it.

## 5 · Findings

**F1 — `medium`, blocker for a green gate: `cargo test --features xapian` is red in 7 tests, README says 0.**
`src/index/fixture.rs:391` (`match crate::search("/var/genenetwork/xapian", …)` — un-gated, so under `xapian`
it reaches the stub and gets the *correct* `Unreachable`, not the `Internal` asserted) and
`tests/grammar_render.rs:141,165,226,264,289,336,574` (six tests that prove "not a 422" by asserting
`gn_search::search("", q, …).is_ok()` — an assertion about *backend availability* that is only true in the
default build). `README.md` "Expected results of that gate" promises 32 passing unit tests under
`--features xapian`; measured 37 with 1 failed, and 6 more once that one is fixed. `docs/WORKER-BLOCKERS.md`
§8.4 records 1 of the 7. *Smallest fix (~10 lines, proved in `/tmp/gns_fix`):* add
`#[cfg(not(feature = "xapian"))]` to the `match` at `fixture.rs:391` (lib then goes 37/37 green), and in those
six tests assert `GRAMMAR.render(q).is_ok()` instead of `gn_search::search("", …)` — the property under test is
grammar acceptance, which needs no backend.

**F2 — `medium`, doc overclaim that I falsified: "the grammar rejects exactly the classes GN4 rejects and
nothing more."** `docs/CORPUS-RUNNER.md:195`, given `tests/parity_corpus.rs:385` + `:537`. B-A's whole bug class
(spurious 422 on quoted ranges) can be reinstated with the report at `8 PASS, 0 FAIL`. *Smallest fix:* replace
that clause with what the run actually shows — "the grammar's verdict on the 54 recorded queries agrees with
GN4's, and its quoted/range-inside-quote handling is pinned only by `tests/grammar_render.rs:248`" — and add
B-A/B-B to the "Not proved" list (items 1–6 at `:200-230`), where item 5 covers only the binding.

**F3 — `low/medium`, a pre-check that green-lights unopenable indexes.**
`src/index/xapian.rs:93` treats `iamglass` presence as "a glass index". 16/16 `iamglass` directories on this
machine have no `postlist.glass` and `xapian-delve` refuses every one; pointed at one, the stub announces
"passed the pre-checks; the next step is Database::open" (`xapian.rs:36-43`). Since the module's own premise is
that "`Database::open` aborts the PROCESS on failure … so every reachable failure has to be an error before the
binding is touched", a half-compacted or partially-deleted index — exactly what this filesystem is full of, and
what an aborted `xapian-compact` leaves — would abort a wired-in backend. It is inherited verbatim from
`genenetwork4/src/search/native.rs:551-556`, so this is fidelity, not invention. *Smallest fix:* in the same
pre-check, also require `path/postlist.glass` to be a readable file (`Unreachable`, "…has no postlist.glass"),
or if you prefer byte-fidelity to GN4, weaken the doc claim to "the failures GN4's pre-checks catch".

**F4 — `low`, README expected-results table is stale in 3 of 5 rows and its premise is obsolete.**
`README.md` "Running the tests": "**Not with `cargo`.**" (it does — 84 green offline), "27 unit tests (32 with
`--features xapian`)" (32 / 37), "14 in `tests/grammar_render.rs`" (26), "5 in `tests/hit_roundtrip.rs`" (6),
and no mention of `parity_corpus`'s 9. `docs/WORKER-BLOCKERS.md` §8.5 says grammar_render is "24" — also stale.
A runner using this table to distinguish "not run" from "failed" concludes five tests vanished. *Smallest fix:*
paste the measured table (`32 · 0 · 4 · 26 · 6 · 7 · 9 = 84`) and delete the rustc-harness section.

> **`[CORRECTED 2026-11-15]`** — fixed in the tree this reads. `README.md` has no "Not with `cargo`"
> section any more; "Running the tests" is the gate block itself, and "Expected results of that gate"
> (`README.md:114-131`) is the measured per-target table — `32 · 0 · 4 · 26 · 6 · 7 · 9 = 84` in the
> default column, plus the `serve`, `xapian` and `all-features` columns — and it now names its two
> preconditions (the pinned `1.88.0` toolchain, and the corpus at `DEFAULT_CORPUS` or
> `GN4_PARITY_FIXTURES`; `VERIFY-REMAINING.md` F6). `docs/WORKER-BLOCKERS.md` §8.5 carries its own
> correction. The stale counts above stay as the record of what F4 was.

**F5 — `nit`, dev-server comment vs code.** `src/bin/gn-search.rs:164` (`type` defaults to `phenotype`, never
validated) under a comment saying "Parameter validation mirrors the bounds GN4's route applies". Verified live:
`?query=alcohol&type=banana` → `200 []`, where GN4 answers `400 invalid_parameter`. Deliberately out of the
library's scope, so fix the comment, not the code.

**F6 — `nit`, the stub's test litters `/tmp`.** `src/index/xapian.rs:157-165` says the scratch directory "is
deliberately left behind"; there are now **14** `gn-search-xapian-*-glass` directories, and because they carry
`iamglass` they are *also* false-positive bait for F3. One `remove_dir_all` in a wrapper after the
`catch_unwind`, or a `TempDir`, removes both problems.

**Checked and clean:** `cargo test --features serve` 84/84 (bare-array `/search` returns the 5 recorded traits,
`10199` first, `page=2&limit=2` → `["10201","10210"]`, `/index` → 503 with no `XAPIAN_DB_PATH`, `/healthz` →
`ok`); `manifest.rs` version gating and `age_seconds`; `error.rs`/`tests/error_status.rs` 422/502/502/502/500
(and the dev server's deliberate `Unreachable`→503 divergence is recorded in `DECISIONS` row 28, not a
contradiction); the fixture's refusal of a non-empty path in the default build; the corpus's completeness.

## Verdict

**PASS-WITH-FIXES.** The default-feature library is genuinely green (84/84), the corpus runner does exactly what
`CORPUS-RUNNER.md` says (8 PASS/0 FAIL, floor and named layer both live, and it turns red on a real slot
regression), and all three blockers are real fixes at the named lines rather than prose. What keeps this off a
clean PASS: a documented gate (`--features xapian`) is red in seven tests while two docs say it is green or
nearly so (F1), and the runner's strongest "Proved" sentence is one I could falsify in ten minutes (F2).

> **`[CORRECTED 2026-11-15]`** — the first of those two reasons is gone: `cargo test --offline --features
> xapian` is **exit 0, 88 / 0** (`unittests src/lib.rs` 38 / 0, `tests/parity_corpus.rs` 7 / 0, every other
> target unchanged) and `--all-features` is the same 88 / 0, so the documented gate is literally true — by
> the change recommended just below: the `crate::search` half of `src/index/fixture.rs`'s test is
> `#[cfg(not(feature = "xapian"))]` and those six `grammar_render` tests assert `GRAMMAR.render(q).is_ok()`.
> The second (F2's overclaim) is corrected in place above. What survives the fix, and is now said plainly in
> `README.md` and `docs/CORPUS-RUNNER.md`: `88 / 0` under the feature is *less* evidence than the default
> `84 / 0` — 0 match-set comparisons, and the `MIN_GRADED` floor not applied.

**If this crate were mine, the single next action:** make the documented gate literally true — gate
`fixture.rs:391` on `#[cfg(not(feature = "xapian"))]` and switch the six `.is_ok()` assertions in
`tests/grammar_render.rs` to `GRAMMAR.render(q).is_ok()`, then paste the measured per-target table into
`README.md`. ~10 lines; it converts "the docs claim" into `cargo test --features xapian` green, which is the
only thing a future reader will actually run.