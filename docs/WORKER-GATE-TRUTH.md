# WORKER-GATE-TRUTH — the documented gate, made literally true

Scope: only findings **F1–F6** of `docs/VERIFY-FINAL.md`. Nothing else was changed. Nothing was
written inside `/home/kabui/project5/genenetwork4` (that directory was only *read*: the corpus at
`test/data/parity-fixtures`).

Toolchain, every run: `~/.rustup/toolchains/1.88.0-x86_64-unknown-linux-gnu/bin/{cargo,rustc}`
(cargo 1.88.0 / rustc 1.88.0, `bin` first on `PATH` because the `rustc` on `PATH` is the broken shim
that `README.md` used to blame), `--offline`, each command wrapped in `timeout 900`.

**Every target is green. No target stays red.** `cargo test --offline` 84/0, `--features serve` 84/0,
`--features xapian` 88/0, `--all-features` 88/0 — exits 0, zero compiler warnings, `cargo fmt --check`
clean. Verbatim output in §F1.

---

## F1 — `cargo test --features xapian` green · **FIXED**

Three places were red under the feature; all three are backend-independent now.

**(a) the gate on the fixture's refusal test** — `src/index/fixture.rs:394`

```rust
        // And through the path a caller actually takes — but only in a build
        // where the fixture IS the backend. With `xapian` compiled in, this
        // path reaches the stub instead, and its refusal (`XAPIAN_DB_PATH is
        // not configured`) is that module's contract, not this one.
        #[cfg(not(feature = "xapian"))]
        match crate::search("/var/genenetwork/xapian", "alcohol", "phenotype", 20, 1) {
```

Only the `crate::search` half is gated, not the whole test: the first half
(`FixtureIndex::open("/var/genenetwork/xapian")` → `Internal("… no search backend is compiled into
this build … --features xapian")`) is a fact about the fixture in *every* build and still asserts
under the feature. That is why the lib target is 38, not 37, under `--features xapian` (37 + the one
new test from F3).

**(b) the six `.is_ok()` assertions that went through `gn_search::search`** —
`tests/grammar_render.rs:141, 165, 226, 264, 289, 336, 574` (seven sites, six tests) now ask the
renderer the same question directly, e.g. `tests/grammar_render.rs:141`:

```rust
        assert!(
            GRAMMAR.render(q).is_ok(),
            "{q:?} must render, not be rejected"
        );
```

| test | line(s) |
|---|---|
| `a_bare_numeric_value_renders_as_its_words` | 141 |
| `a_non_numeric_value_without_a_range_is_not_a_422` | 165 |
| `a_quoted_value_is_never_a_range` | 226 |
| `a_range_looking_token_inside_a_quote_is_not_a_422` | 264, 289 |
| `an_empty_field_value_is_the_label_word` | 336 |
| `a_not_after_a_binary_operator_is_not_dangling` | 574 |

The query and the assertion are unchanged — `year:2014`, `mean:"1..2"`,
`"alcohol mean:abc..def"`, `mean:1.."2"`, `position:`, `NOT alcohol` — only the entry point moved
from "search returns Ok" to "the grammar accepts it", which is the property those tests name. The
four `gn_search::search` *error* assertions in the same file (`:434`, `:507`, `:542`, `:605`) were
left alone deliberately: they assert `InvalidQuery`, which the crate decides by rendering *before*
any index is opened, so they are already backend-independent (and still green under both features).

**(c) the three `tests/parity_corpus.rs` failures** — the runner no longer touches the fixture's
inventory unless the fixture backend is compiled in.

* `tests/parity_corpus.rs:101` — `const FIXTURE_BACKEND: bool = cfg!(not(feature = "xapian"));`
* `tests/parity_corpus.rs:105` — `const NO_FIXTURE_BACKEND: &str =
  "no-fixture-backend-under-this-feature";`
* `tests/parity_corpus.rs:683-705` — in `grade()`, a recorded-200 shape short-circuits to
  `Verdict::Skip(NO_FIXTURE_BACKEND)` with `Graded::Nothing` when `!FIXTURE_BACKEND`. The error-class
  shapes above it still grade (the grammar decides them). Never `PASS`, never a panic.
* `tests/parity_corpus.rs:813-826` — `corpus_report` calls `fixture_inventory()` only inside
  `if FIXTURE_BACKEND { … }`; otherwise the inventory is the empty set.
* `tests/parity_corpus.rs:879-897` — and it then *returns before* `evidence_shortfall`, asserting
  `counts.graded == 0` and printing that the floor is not applied, so the skip lines cannot be
  counted into `MIN_GRADED` (the count is 0, and the floor would have failed the run on 5).
* `tests/parity_corpus.rs:993-994, 1125, 1160` — the two metric tests that grade *through* the
  inventory (`an_empty_expected_set_is_vacuous_not_graded`, `a_phrase_shape_is_skipped_as_unrepresentable`)
  and their `recorded_shape` helper are `#[cfg(not(feature = "xapian"))]`: with no fixture in the
  build there is nothing for them to grade, and inventing a PASS would be worse than not running them.
  That is why `tests/parity_corpus.rs` is 7 targets under the feature, not 9 (9 − 2).

The feature-gated report, verbatim (`cargo test --offline --features xapian --test parity_corpus
corpus_report -- --nocapture`):

```
parity corpus: 54 shapes from /home/kabui/project5/genenetwork4/test/data/parity-fixtures (no fixture backend under this feature)

Q01 | alcohol | phenotype | 1197 | Internal | - | SKIP(no-fixture-backend-under-this-feature)
Q02 | insulin | gene | 57632 | Internal | - | SKIP(no-fixture-backend-under-this-feature)
…
Q42 | Hs:foo:1M | phenotype | error 500 | InvalidQuery(InvalidOperationError) | error-class | PASS
Q51 | alcohol AND | phenotype | error 400 | InvalidQuery(QueryParserError) | error-class | PASS
Q52 | mean:abc..def | phenotype | error 400 | InvalidQuery(QueryParserError) | error-class | PASS

54 shapes seen: 0 graded a set-equality/Jaccard comparison, 3 graded an error class, 0 VACUOUS (expected set empty: no evidence), 51 SKIP (the missing xapian backend's, see docs/CORPUS-RUNNER.md); 3 PASS, 0 FAIL
exit rule: any FAIL fails the test, and so does fewer than MIN_GRADED = 5 graded comparisons; PASS, VACUOUS and SKIP alone are not evidence

no fixture backend under this feature: 51 shapes SKIP(no-fixture-backend-under-this-feature), 0 graded; the evidence floor MIN_GRADED = 5 is not applied to this run

test corpus_report ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 6 filtered out; finished in 3.20s
```

The **default** run is unchanged by all of this — same summary line as `VERIFY-FINAL.md` §2
(`cargo test --offline --test parity_corpus corpus_report -- --nocapture`):

```
parity corpus: 54 shapes from /home/kabui/project5/genenetwork4/test/data/parity-fixtures (fixture index: 5 documents)
Q15 | species:mouse | phenotype | 19285 | 5 hits | set-eq | PASS
Q29 | mean:..15 | phenotype | 13528 | 5 hits | set-eq | PASS
Q31 | peak:5..15 | phenotype | 11931 | 2 hits | set-eq | PASS
Q36 | additive:0.5..1.0 | phenotype | 710 | 2 hits | set-eq | PASS
Q37 | year:2010..2020 | phenotype | 12442 | 5 hits | set-eq | PASS
Q42 | Hs:foo:1M | phenotype | error 500 | InvalidQuery(InvalidOperationError) | error-class | PASS
Q51 | alcohol AND | phenotype | error 400 | InvalidQuery(QueryParserError) | error-class | PASS
Q52 | mean:abc..def | phenotype | error 400 | InvalidQuery(QueryParserError) | error-class | PASS
54 shapes seen: 5 graded a set-equality/Jaccard comparison, 3 graded an error class, 7 VACUOUS (expected set empty: no evidence), 39 SKIP (the missing xapian backend's, see docs/CORPUS-RUNNER.md); 8 PASS, 0 FAIL
```

### The four gates — real output, per target

`cargo test --offline` — **exit 0**, 0 warnings, **84 passed / 0 failed**:

```
     Running unittests src/lib.rs (target/debug/deps/gn_search-161036bc86e470ab)
test result: ok. 32 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
     Running unittests src/bin/gn-search.rs (target/debug/deps/gn_search-cfac40118a7e69a7)
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/error_status.rs (target/debug/deps/error_status-dd9afb519b6a8199)
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/grammar_render.rs (target/debug/deps/grammar_render-a9dd014c605c8351)
test result: ok. 26 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
     Running tests/hit_roundtrip.rs (target/debug/deps/hit_roundtrip-c2ec88bd8d51d2c7)
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
     Running tests/manifest.rs (target/debug/deps/manifest-34d3442b0ecbf221)
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/parity_corpus.rs (target/debug/deps/parity_corpus-c9642e4d911dc08d)
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.33s
   Doc-tests gn_search
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

`cargo test --offline --features serve` — **exit 0**, 0 warnings, **84 passed / 0 failed**:

```
     Running unittests src/lib.rs (target/debug/deps/gn_search-77b53a2143c259b7)
test result: ok. 32 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
     Running unittests src/bin/gn-search.rs (target/debug/deps/gn_search-3783508174de2bb5)
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/error_status.rs (target/debug/deps/error_status-3cc4685d8c521a2e)
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/grammar_render.rs (target/debug/deps/grammar_render-20dbb834999f9e2d)
test result: ok. 26 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
     Running tests/hit_roundtrip.rs (target/debug/deps/hit_roundtrip-6ce349968aead0c9)
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/manifest.rs (target/debug/deps/manifest-36190b3a0de0a73c)
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/parity_corpus.rs (target/debug/deps/parity_corpus-11e12a819391b7ca)
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.29s
   Doc-tests gn_search
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

`cargo test --offline --features xapian` — **exit 0**, 0 warnings, **88 passed / 0 failed** (was
exit 101 with 1 lib failure + 6 `grammar_render` + 3 `parity_corpus` failures):

```
     Running unittests src/lib.rs (target/debug/deps/gn_search-9217849ac742b1c8)
test result: ok. 38 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
     Running unittests src/bin/gn-search.rs (target/debug/deps/gn_search-d8e6f017dc7cd79a)
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/error_status.rs (target/debug/deps/error_status-85d7173e7e813fc1)
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/grammar_render.rs (target/debug/deps/grammar_render-d66230fe73e6773c)
test result: ok. 26 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
     Running tests/hit_roundtrip.rs (target/debug/deps/hit_roundtrip-61e3ffb0ea6df4e8)
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/manifest.rs (target/debug/deps/manifest-c466f5babd5ed3da)
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
     Running tests/parity_corpus.rs (target/debug/deps/parity_corpus-041642527f06332a)
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.82s
   Doc-tests gn_search
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

`cargo test --offline --all-features` (`serve,xapian`) — **exit 0**, 0 warnings, **88 passed / 0
failed**:

```
     Running unittests src/lib.rs (target/debug/deps/gn_search-d8a92f117cdd1dde)
test result: ok. 38 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
     Running unittests src/bin/gn-search.rs (target/debug/deps/gn_search-2e3a9389bbc4f9a6)
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/error_status.rs (target/debug/deps/error_status-85c8548b57ce5cad)
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/grammar_render.rs (target/debug/deps/grammar_render-73e7df9ffb0760a0)
test result: ok. 26 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
     Running tests/hit_roundtrip.rs (target/debug/deps/hit_roundtrip-5bd14d2703a64278)
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/manifest.rs (target/debug/deps/manifest-15694387d23a2149)
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/parity_corpus.rs (target/debug/deps/parity_corpus-946b3b6a3e869d40)
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.13s
   Doc-tests gn_search
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

`cargo fmt --check` — no output, exit 0.

Why the numbers differ from `VERIFY-FINAL.md` §1: `unittests src/lib.rs` is **38** under the feature
(its 37 + the one new F3 test), and `tests/parity_corpus.rs` is **7** (its 9 − the 2 gated metric
tests). Nothing was deleted to make a count go up.

---

## F2 — the overclaim · **FIXED**

`docs/CORPUS-RUNNER.md:195` — "Proved: the grammar rejects exactly the classes GN4 rejects and
nothing more" is gone. It now states what the run shows (same summary line as §F1's default run):

> Proved, and this is the whole of it — the report of the run on the committed corpus, verbatim:
> `54 shapes seen: 5 graded a set-equality/Jaccard comparison, 3 graded an error class, 7 VACUOUS
> (expected set empty: no evidence), 39 SKIP; 8 PASS, 0 FAIL`. That is: the grammar's verdict on the
> 54 recorded queries agrees with GN4's — the three recorded error shapes come back with the same
> exception class and no recorded 200 shape is rejected — …

and the falsification is written down, as a new "Not proved" item (`docs/CORPUS-RUNNER.md:229`),
quoting `VERIFY-FINAL.md` §4's measurement rather than paraphrasing it:

> `VERIFY-FINAL.md` §4 measured this rather than arguing it: with blocker B-A reverted in a scratch
> copy — a spurious 422 restored on `"alcohol mean:abc..def"`, a quoted range no longer a range —
> `corpus_report` still reported `8 PASS, 0 FAIL` and exited 0, and the same held for B-B reverted
> and for B-C's named layer deleted. The 39 `SKIP` classes absorb those regressions because **the
> corpus holds no quoted-range shape to see them with**, so "the grammar rejects exactly the classes
> GN4 rejects and nothing more" was falsifiable and false and is not claimed here … The guard for the
> quoted/range-inside-quote class is the unit test
> `tests/grammar_render.rs::a_range_looking_token_inside_a_quote_is_not_a_422`, not this runner.

Also added, because F1 changed the runner's behaviour under the feature:
`docs/CORPUS-RUNNER.md:247` "## Under `--features xapian`" — the skip reason, the floor not being
applied, and the two gated metric tests (`:309`).

---

## F3 — the pre-check must not green-light an unopenable index · **FIXED**

`src/index/xapian.rs:101-113` — after the `iamglass` marker check, the pre-check now requires a
`postlist.glass` table:

```rust
    if !std::path::Path::new(path).join("postlist.glass").is_file() {
        return Err(SearchError::Unreachable(format!(
            "xapian index at {path} has no postlist.glass: the iamglass marker is present but \
             the database is not openable (an interrupted compact or a partially deleted index)"
        )));
    }
```

The doc block above it (`:101-107`) says plainly that this is the one place the module is *stricter*
than GN4 (`native.rs:551-556`), and the module header (`:22-27`) now lists it among the checks.
New test — `src/index/xapian.rs:194`, `an_iamglass_marker_without_postlist_is_not_an_index`: a
scratch directory containing only `iamglass` + `flintlock` must be `Unreachable` mentioning
`no postlist.glass`.

Live proof, end to end, against the exact directory `VERIFY-FINAL.md` §3 says `xapian-delve` refuses
(`/home/kabui/spikes/index-self-build/xapian-index` = `iamglass` + `flintlock`, no tables):

```
$ cargo build --offline --features serve,xapian --bin gn-search
$ GN_SEARCH_ADDR=127.0.0.1:18099 XAPIAN_DB_PATH=/home/kabui/spikes/index-self-build/xapian-index ./target/debug/gn-search
gn-search: listening on 127.0.0.1:18099, index "/home/kabui/spikes/index-self-build/xapian-index"
$ curl -s -w ' <- HTTP %{http_code}\n' '…/search?query=alcohol&type=phenotype&limit=2&page=1'
{"detail":"xapian index at /home/kabui/spikes/index-self-build/xapian-index has no postlist.glass: the iamglass marker is present but the database is not openable (an interrupted compact or a partially deleted index)","error":"index_unavailable"} <- HTTP 503
```

Before this change the same request panicked with *"The index at … passed the pre-checks; the next
step is Database::open"* and the connection was dropped (HTTP 000). Note the pre-check message is
GN4's for the earlier cases and this one says what it is.

---

## F4 — README expected results · **FIXED**

`README.md:80` — "## Running the tests" no longer says "**Not with `cargo`.**"; the rustc-rlib
harness section is deleted (cargo runs the gate: 84 green offline). `README.md:102` is the MEASURED
per-target table for `default`, `--features serve`, `--features xapian`, `--all-features` — the four
columns are the four gate runs pasted in §F1, `passed / failed` per target, totals
`84 / 84 / 88 / 88`, all failed 0 (`README.md:118`), with the differences explained below the table
(the six xapian pre-check tests, the two gated corpus metric tests, and "`--features xapian` is
**not** evidence about a real index").

Also kept true under F3's change: `README.md:49-57` "What is stubbed" now says the module also
requires a `postlist.glass` (it said "the three pre-checks" and listed only three).

Stale counts left alone on purpose (dated work logs, not the gate table): `docs/WORKER-BLOCKERS.md`
§8.4 (1 of 7 red) and §8.5 (`grammar_render` "24") are records of the state at that time, as are
`docs/WORKER-CRATE.md:28` and `docs/WORKER-FINAL.md`. See residual risks.

---

## F5 — dev-server `type` default · **FIXED (comment, not code)**

`src/bin/gn-search.rs:147-150` — the handler doc no longer claims that validation mirrors GN4's
route in general; it names the three parameters that *are* checked:

```rust
    /// engine are mirrored for `query` (non-empty), `limit` (numeric, capped
    /// at 100) and `page` (numeric, at least 1) — SCOUT-NATIVE §1, §5.
```

and at the `type` default (`src/bin/gn-search.rs:165-171`):

```rust
        // NOT validated: `type` defaults to `phenotype` and any other value
        // goes through to the `XT{type}` filter unchanged, so `?type=banana`
        // answers 200 with an empty array where GN4 answers 400
        // `invalid_parameter`. The value domain is the route's, not this
        // crate's (`src/lib.rs`, "what this crate deliberately is not"), so
        // the server does not reject it either.
        let type_ = param(params, "type").unwrap_or("phenotype");
```

Verified live (default backend, `cargo run --offline --features serve`):

```
-- type=banana:      [] <- HTTP 200
-- type=phenotype:   [{"name":"10199",…},{"name":"10200",…}] <- HTTP 200     (10199 first)
-- healthz:          ok
-- /index:           {"detail":"XAPIAN_DB_PATH is not set, so there is no index (and no manifest) to report","error":"index_unavailable"} <- HTTP 503
```

The code is unchanged: the 400 belongs to GN4's route (`docs/DECISIONS.md` row 28 keeps the
deliberate `Unreachable`→503 divergence).

---

## F6 — the stub test must clean up `/tmp` · **FIXED**

`src/index/xapian.rs:217-224` — a `ScratchDir(PathBuf)` whose `Drop` does `remove_dir_all`, and both
scratch-directory tests hold it (`:180`, `:195`). The `#[should_panic]` test can keep its attribute
because cleanup is no longer a statement after the call. The test's own comment no longer says "the
scratch directory is deliberately left behind" (`:170-176`), and it now writes a `postlist.glass`
too, because an `iamglass`-only directory is exactly what F3 refuses.

Proof it stops the litter (`ls -d /tmp/gn-search-xapian-*` before and after a `--features xapian`
lib run):

```
before: 14   (the stale dirs VERIFY-FINAL.md §3/§F6 counted: /tmp/gn-search-xapian-190461-glass …-3810029-glass)
test result: ok. 38 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
after:  14   (unchanged — this run created none)
```

The 14 pre-existing directories were left on disk (removing someone else's `/tmp` state is not this
task); they are harmless now, because a marker alone no longer passes the pre-check.

---

## Files changed

| file | finding | change |
|---|---|---|
| `src/index/fixture.rs:390-395` | F1(a) | `#[cfg(not(feature = "xapian"))]` on the `crate::search` half of `a_non_empty_index_path_is_refused` |
| `tests/grammar_render.rs:141, 165, 226, 264, 289, 336, 574` | F1(b) | 7 assertions moved from `gn_search::search(…).is_ok()` to `GRAMMAR.render(q).is_ok()` |
| `tests/parity_corpus.rs:101, 105, 683-705, 813-826, 833-838, 879-897, 993-994, 1125, 1160` | F1(c) | `FIXTURE_BACKEND` guard, `SKIP(no-fixture-backend-under-this-feature)`, floor not applied, 2 metric tests gated |
| `src/index/xapian.rs:22-27, 101-113, 170-227` | F3, F6 | `postlist.glass` pre-check + new test; `ScratchDir` cleanup; `postlist.glass` in the stub test |
| `src/bin/gn-search.rs:147-150, 165-171` | F5 | handler doc + `type` default comment |
| `README.md:49-57, 80-152` | F4 (and F3's doc truth) | measured per-target gate table; rustc-harness section deleted; `postlist.glass` in "What is stubbed" |
| `docs/CORPUS-RUNNER.md:195-208, 229-245, 247-259, 308-310` | F2 (and F1(c)'s doc) | overclaim replaced with the measured claim + the falsification; "Under `--features xapian`" |
| `docs/WORKER-GATE-TRUTH.md` | — | this report |

---

## Residual risks

1. **`docs/WORKER-BLOCKERS.md` §8.4/§8.5 are now stale** (they record 1 of 7 red under `--features
   xapian`, and `grammar_render` "24"). They are dated work logs, not the gate, so they were not
   rewritten — but a reader who lands there first will read an out-of-date red count.
2. **A PASS is still never claimed where nothing was compared.** With the feature on, the corpus
   grades 3 error classes and 51 skips; that is by design, and `corpus_report` asserts
   `counts.graded == 0` so it cannot silently drift into grading a match set it does not have. But
   the run under `--features xapian` is *less* evidence than the default run — it is only proof that
   everything compiles and the error classes hold.
3. **F2's B-A/B-B quote-state classes remain graded by one unit test each**, not by the corpus. That
   is now written down (`CORPUS-RUNNER.md:229`), not fixed: a real quoted-range shape in the corpus
   would be the fix, and the corpus is a recording, not something this task may edit.
4. **The xapian pre-check is stricter than GN4's** (`postlist.glass`). If a future wired-in binding
   ever wants byte-fidelity to `native.rs:551-556`, this is the one intentional divergence, and both
   the code comment and `README.md` say so.
5. **Clippy is not clean** (pre-existing, unchanged by this task): 7 lib warnings (`duplicated
   attribute` on the `#[cfg]` + `#![cfg]` pair, `map_or`, `&PathBuf`), 2 in `tests/manifest.rs`, 1 in
   `tests/parity_corpus.rs` (`uninlined_format_args` on the report's own summary line). None is an
   error and none is in code this task added; `README.md:154` says clippy carries style lints.
6. **No real index was opened** — there is still none on this machine (every `iamglass` directory has
   no `postlist.glass`), so nothing here says anything about hit counts from a populated glass DB.
