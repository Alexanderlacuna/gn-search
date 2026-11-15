# VERIFY-GATE-TRUTH — independent verification of the gate-truth fix

Reviewer: `reviewer-alt` (non-GLM family). Everything below was **run**, not read.
Toolchain `~/.rustup/toolchains/1.88.0-x86_64-unknown-linux-gnu/bin/{cargo,rustc}` (cargo 1.88.0 /
rustc 1.88.0), every cargo invocation `--offline` and inside `timeout 900`. Nothing was built or
written under `/home/kabui/project5/genenetwork4` (it is only *read*, by the corpus runner).

**How "before/after" was obtained.** The workspace is not a git repository (`git status` → *not a
git repository*), so there is no `git diff` to appeal to. The pre-fix tree survives at
`/tmp/gns_fix` (the pre-fix tree with only the reviewer's F1(a) `#[cfg]` applied) and
`/tmp/gns_mut_{a..d}` (pre-fix tree + one deliberate mutation each, left by the previous review).
Every quote below is a byte `diff` against those snapshots, not recollection:

```
$ diff -u /tmp/gns_mut_d/src/index/fixture.rs  …/src/index/fixture.rs   # → the #[cfg] + comment only
$ diff -u /tmp/gns_fix/tests/grammar_render.rs …/tests/grammar_render.rs # → the 7 assertion sites only
$ diff -u /tmp/gns_fix/tests/parity_corpus.rs  …/tests/parity_corpus.rs  # → the FIXTURE_BACKEND gate only
$ diff -u /tmp/gns_fix/src/index/xapian.rs     …/src/index/xapian.rs     # → postlist check + 2 tests + ScratchDir
```

Files **byte-identical** to the pre-fix snapshot: `src/grammar.rs`, `src/lib.rs`, `src/hit.rs`,
`src/error.rs`, `src/manifest.rs`, `src/index/mod.rs`, `tests/error_status.rs`,
`tests/hit_roundtrip.rs`, `tests/manifest.rs`. So the fix touched no production logic except the
new `postlist.glass` pre-check; everything else is a test gate, a comment or a doc.

---

## 1 · The four gates — counts, pasted

Run in the workspace (`target/`), and again from a **cold** `CARGO_TARGET_DIR=/tmp/rsv-tgt`; the two
runs are identical per target, so nothing below is a stale binary.

| target | `--offline` | `--offline --features serve` | `--offline --features xapian` | `--offline --all-features` |
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

Verbatim tail of the `--features xapian` run (the one that was exit 101 / 7 red before this fix):

```
     Running unittests src/lib.rs (target/debug/deps/gn_search-9217849ac742b1c8)
test result: ok. 38 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
     Running tests/parity_corpus.rs (target/debug/deps/parity_corpus-041642527f06332a)
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.13s
   Doc-tests gn_search
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Compiler warnings and formatting, measured, not taken on trust:

```
$ CARGO_TARGET_DIR=/tmp/rsv-tgt2 cargo build --offline --all-features --all-targets 2>&1 | grep -c '^warning'
0
$ cargo fmt --check ; echo $?
0
```

**The README table (`README.md:108-118`) is exactly these numbers** — I compared all 32 cells (8
targets × 4 columns) and the four totals: match. The `WORKER-GATE-TRUTH.md` counts also match
(84/84/88/88), including the two "explained" deltas (`38` = 32 + the six stub tests; `7` = 9 − the
two inventory-gated metric tests).

The corpus report, both builds, verbatim:

```
# default
54 shapes seen: 5 graded a set-equality/Jaccard comparison, 3 graded an error class, 7 VACUOUS
(expected set empty: no evidence), 39 SKIP (the missing xapian backend's, see docs/CORPUS-RUNNER.md); 8 PASS, 0 FAIL
# --features xapian
54 shapes seen: 0 graded a set-equality/Jaccard comparison, 3 graded an error class, 0 VACUOUS
(expected set empty: no evidence), 51 SKIP (the missing xapian backend's, see docs/CORPUS-RUNNER.md); 3 PASS, 0 FAIL
no fixture backend under this feature: 51 shapes SKIP(no-fixture-backend-under-this-feature), 0 graded;
the evidence floor MIN_GRADED = 5 is not applied to this run
```

---

## 2 · F1–F6, each with the proving line

| # | verdict | proving file:line | what still invites a wrong reading |
|---|---|---|---|
| **F1** `--features xapian` red in 7 | **FIXED** | `src/index/fixture.rs:394` (`#[cfg(not(feature = "xapian"))]` on the `crate::search` half only — the `FixtureIndex::open` half still asserts `Internal` under the feature); `tests/grammar_render.rs:141,165,226,264,289,336,574`; `tests/parity_corpus.rs:101,683-705,813-826,879-897,993,1125,1160` | see V-1, V-3 and §2.1: the aggregate `88 / 0` for `--all-features` reads as *stronger* than the default `84 / 0`, and is strictly weaker in evidence |
| **F2** the "rejects exactly the classes … and nothing more" overclaim | **FIXED** | `docs/CORPUS-RUNNER.md:195-208` (replaced by the measured sentence) and `:229-245` (the falsification written down as *Not proved* item 6, quoting the previous review's measurement, and naming `tests/grammar_render.rs::a_range_looking_token_inside_a_quote_is_not_a_422` as the real guard). `grep -rn "exactly the classes"` now hits only the two review documents that quote it as falsified. | V-2: the paragraph immediately below (`:261-267`) is now false in the other direction |
| **F3** pre-check green-lights unopenable indexes | **FIXED** | `src/index/xapian.rs:108-113` (`postlist.glass` required after the `iamglass` check); new test `:193-205`; doc `:18-27`, `:101-107`. End-to-end re-verified below. | V-4: the header still claims *every* operator-visible failure is caught; the check is by file name |
| **F4** README expected-results table stale | **FIXED** | `README.md:80` ("Not with `cargo`." gone; rustc-harness section deleted), `:100-118` (the measured table), `:120-152` (the two deltas explained), `:49-57` (`postlist.glass` added to "What is stubbed") | V-5: `README.md:28`, same file, still says "the three pre-checks" |
| **F5** dev-server `type` comment vs code | **FIXED (comment only, as asked)** | `src/bin/gn-search.rs:147-150` (names the three params that are checked) and `:165-171` (explicit `NOT validated` block) | none material |
| **F6** stub test litters `/tmp` | **FIXED** | `src/index/xapian.rs:216-224` (`ScratchDir` + `Drop`), held at `:180` and `:195`; the "deliberately left behind" comment is gone (`:170-177`) | none |

Independent re-proof of F3 and F5, run now (not quoted from the report):

```
$ GN_SEARCH_ADDR=127.0.0.1:18099 XAPIAN_DB_PATH=/home/kabui/spikes/index-self-build/xapian-index \
    ./gn-search   # built --features serve,xapian
$ curl -s -w ' <- HTTP %{http_code}\n' '…/search?query=alcohol&type=phenotype&limit=2&page=1'
{"detail":"xapian index at /home/kabui/spikes/index-self-build/xapian-index has no postlist.glass: the
 iamglass marker is present but the database is not openable (…interrupted compact…)","error":"index_unavailable"} <- HTTP 503

$ GN_SEARCH_ADDR=127.0.0.1:18097 ./gn-search        # default backend, --features serve
?query=alcohol&type=banana   -> []                      <- HTTP 200
?query=alcohol&type=phenotype&limit=2&page=1 -> ["10199","10200"] <- HTTP 200
/index                       -> {"detail":"XAPIAN_DB_PATH is not set…","error":"index_unavailable"} <- HTTP 503
/healthz                     -> ok
```

The stale shell directory is `iamglass` (126 B) + `flintlock`, no `postlist.glass` — exactly the
shape that used to panic at `xapian.rs:43` and drop the connection.

F6, counted on disk before/after my two `--features xapian` lib runs: `ls -d /tmp/gn-search-xapian-*`
→ **14 → 14**. No new directory. The 14 pre-existing ones are now harmless, since a marker alone no
longer passes the pre-check.

### 2.1 The one F1 reading I would still expect a maintainer to get wrong

`--all-features` is the gate people reach for when they want "the strongest one". After this fix it
is the **weakest**: with `xapian` on, the corpus grades **0** match-set comparisons and the
`MIN_GRADED` floor is deliberately not applied (`tests/parity_corpus.rs:879-897`). `88 / 0` is more
green tests than `84 / 0` and less evidence. `README.md:128-140` does say this, in two bullets that
have to be read together; a one-word marker in the table row (`7 / 0 — grades nothing`) would remove
the trap. Not a blocker, but it is the exact failure mode (`a gate that looks green`) this task
exists to kill.

---

## 3 · Was anything weakened to get green?

### 3.1 The six `grammar_render` assertions — before / after, quoted

Diff of `/tmp/gns_fix/tests/grammar_render.rs` (pre-fix) vs the delivered file. **The queries are
identical, the loop data is identical, and every term-shape assertion is untouched** — the diff
touches nothing but the last `assert!` of each test:

```diff
@@ a_bare_numeric_value_renders_as_its_words   (for (q, words) in year:2014, mean:5, peak:1.5, additive:-2)
-            gn_search::search("", q, "phenotype", 20, 1).is_ok(),
-            "{q:?} must be searchable, not rejected"
+            GRAMMAR.render(q).is_ok(),
+            "{q:?} must render, not be rejected"

@@ a_non_numeric_value_without_a_range_is_not_a_422   (for q in mean:abc, year:twenty, peak:abc)
-            gn_search::search("", q, "phenotype", 20, 1).is_ok(),
+            GRAMMAR.render(q).is_ok(),

@@ a_quoted_value_is_never_a_range
-        gn_search::search("", "mean:\"1..2\"", "phenotype", 20, 1).is_ok(),
+        GRAMMAR.render("mean:\"1..2\"").is_ok(),

@@ a_range_looking_token_inside_a_quote_is_not_a_422
-        gn_search::search("", "\"alcohol mean:abc..def\"", "phenotype", 20, 1).is_ok(),
+        GRAMMAR.render("\"alcohol mean:abc..def\"").is_ok(),
-        gn_search::search("", "mean:1..\"2\"", "phenotype", 20, 1).is_ok(),
+        GRAMMAR.render("mean:1..\"2\"").is_ok(),

@@ an_empty_field_value_is_the_label_word
-    // And it is searchable rather than rejected.
-    assert!(gn_search::search("", "position:", "phenotype", 20, 1).is_ok());
+    // And it renders rather than being rejected.
+    assert!(GRAMMAR.render("position:").is_ok());

@@ a_not_after_a_binary_operator_is_not_dangling
-        assert!(gn_search::search("", q, "phenotype", 20, 1).is_ok(), "{q:?} must be searchable, not a 422");
+        assert!(GRAMMAR.render(q).is_ok(), "{q:?} must render, not be a 422");
```

**Verdict: loosened in form, not in content — and provably so.** In the default build the two forms
are equivalent, because `gn_search::search` is exactly *render, then open, then search*:

* `src/lib.rs:110-113` — `search()` returns `Err(InvalidQuery)` iff `GRAMMAR.render` fails;
* `src/lib.rs:114-117` — `open_index("")` is `FixtureIndex::open("")`, `Ok` for the empty path
  (`src/index/fixture.rs:92-104`);
* `src/index/fixture.rs:118-135` — `FixtureIndex::search` has **one** return, `Ok(...)`: it cannot
  fail.

So `search("", q, "phenotype", 20, 1).is_ok() == GRAMMAR.render(q).is_ok()` for every `q`, today.
Nothing that was asserted before is now unasserted in the default gate, and the six tests keep their
render-shape assertions (`assert_eq!(term.prefix, …)`, `rendered.len()`, `term.kind`), which is where
"not a 422" actually lives. What *is* lost is a future-proofing guard (a `lib::search` that started
rejecting a renderable query, or a fixture that started returning `Err`, would no longer be caught for
these six shapes) — reported as V-3, because one of the seven sites is now a tautology.

The four `gn_search::search` **error** assertions the report says it left alone are in fact left
alone: `tests/grammar_render.rs:434, 507, 542, 602` (the report says `:605`; the file moved by 3).
They stay green under the feature because the render happens before any backend is opened.

### 3.2 Can the SKIP path pay into the evidence floor? **No — three independent reasons.**

1. A skip row is `Graded::Nothing` + `Verdict::Skip` (`tests/parity_corpus.rs:701-705`) and
   `Row::is_graded_evidence()` requires `graded.is_match_set() && Verdict::Pass` (`:472-475`), so
   `summarize`'s `counts.graded` (`:531-533`) cannot see it.
2. `corpus_report` asserts `counts.graded == 0` on that path (`:886-890`) — if a skip ever started
   paying in, the assertion fails rather than the floor passing.
3. It then `return`s **before** `evidence_shortfall` (`:896` vs `:898`), so `MIN_GRADED` is not
   applied at all, and the run says so in print (see §1). The named `MUST_GRADE` layer is unreachable
   on that path for the same reason.

And the floor still bites where it is supposed to: the two tests that prove it —
`a_thinned_run_falls_below_the_evidence_floor` (an all-SKIP synthetic run must fail) and
`one_must_grade_shape_turned_into_a_skip_fails_the_run` — both still run in the default gate (they are
2 of the 9 in `tests/parity_corpus.rs`), and neither is feature-gated. Only the two tests that need
the fixture's inventory are (`:1125`, `:1160`), which is what turns 9 into 7.

The default-build behaviour is byte-for-byte unchanged, which the diff confirms: the `!FIXTURE_BACKEND`
block (`:683-705`) and the `if FIXTURE_BACKEND` inventory (`:813-826`) are compile-time-known to be
off, and the printed header still reads `(fixture index: 5 documents)`.

**One real cost, disclosed and accepted:** under `--features xapian` the corpus now *skips* where it
used to *fail*. That is the right call — the three old failures were the fixture's absence, not the
crate's behaviour — but it means the `xapian` gate is a compile-and-error-class gate only. Both
`README.md:139-141` and `docs/CORPUS-RUNNER.md:247-259` say so.

---

## 4 · Findings (nothing above `low`; no logic defect found)

**V-1 · low · the `--features xapian` run misnames the backend it is missing, in print.**
`tests/parity_corpus.rs:851-857` prints `… 51 SKIP (the missing xapian backend's, see
docs/CORPUS-RUNNER.md)` in a build where the xapian backend *is* compiled in; what is missing is the
fixture. Same sentence in the module doc at `:36-38` ("Every skip is attributable to the missing
backend (no xapian is compiled into this build)"). *Why it matters:* this is the line a maintainer
pastes into a bug report, and F1 was precisely about the feature build describing itself wrongly.
*Smallest fix:* make that parenthetical a `if FIXTURE_BACKEND { "…" } else { "…no fixture backend…" }`
and reword `:36-38`.

**V-2 · low · `docs/CORPUS-RUNNER.md:261-267` is now false: "The runner needs no change for that —
only a way to point `FIXTURE_PATH` at an index."** With `FIXTURE_BACKEND =
cfg!(not(feature = "xapian"))` (`tests/parity_corpus.rs:101`), the day a real xapian backend lands the
const stays `false`, every recorded-200 shape short-circuits to `SKIP` (`:683-705`) and the floor is
skipped (`:879-897`) — i.e. the documented growth path silently yields a no-evidence run. *Why it
matters:* the previous review's F2 was "the doc claims more than the runner does"; this is the same
defect one paragraph further down, introduced by the F1(c) fix. *Smallest fix:* append "…and drop the
`FIXTURE_BACKEND` gate (`tests/parity_corpus.rs:101`), which today keys *can grade* to the absence of
the `xapian` feature."

**V-3 · low · `tests/grammar_render.rs:574` is now a tautology.** The loop already did
`GRAMMAR.render(q).unwrap_or_else(|e| panic!(…))` at `:569-571`; the new
`assert!(GRAMMAR.render(q).is_ok(), …)` can no longer fail, so that line asserts nothing and a reader
reasonably assumes a second property is checked. *Smallest fix:* delete `:574`, or wrap one end-to-end
assertion in `#[cfg(not(feature = "xapian"))]` to keep the seam covered.

**V-4 · nit · `src/index/xapian.rs:19-21` still over-reaches one layer below F3.** "every failure an
operator can hit has to be caught before the call" — the new check is by file *name*
(`:108-113`), so a truncated or zero-length `postlist.glass` (a build killed mid-write, a partial
copy) passes and reaches `Database::open`, which the module says aborts the process. The test at
`:184` even codifies a 0-byte `postlist.glass` as "glass-shaped". *Smallest fix:* say "every failure
visible without opening the database", and name the residual in the module header.

**V-5 · nit · `README.md:28` still says "the three pre-checks"** for a module that now makes five
checks (`src/index/xapian.rs:76-115`). `README.md:49-57` was corrected by this task; this line in the
same file was not, and it is the line that summarises the module. *Smallest fix:* "the pre-checks".

Nothing else: `src/grammar.rs` and every file outside the change list are byte-identical to the
pre-fix snapshot, so the B-A/B-B/B-C fixes from the earlier round are intact, and no test was deleted
— the default gate is still 84, the same 84 as before.

---

## 5 · The judgement

**Is this a fair thing to hand a maintainer as "extraction skeleton, gates green, here is what it does
not do"?** *Close to yes.* The default gate is genuinely green and genuinely evidenced: 84 tests, of
which 5 shapes compare a real match set and 2 of those are two-sided, plus 3 error classes, with a
floor and a named layer that both turn red on a real slot regression (measured last round, and still
the case — the machinery is untouched). The corpus is real data from the real GN4 tree, not invented.
And the "what it does not do" list is now *specific*: 7 items in `docs/CORPUS-RUNNER.md:203-245`, six
residual risks in `WORKER-GATE-TRUTH.md`, and the README says in one sentence that `--features xapian`
is a stub and proves nothing about an index. A maintainer who reads the README top-to-bottom gets an
honest picture, including the uncomfortable part (no real index has ever been opened on this machine).

For *this* task, the truthfulness bar was "the documented gate is literally true". It is: I ran all
four gates cold, and the four counts in the README are the four counts I measured.

**Verdict: PASS-WITH-FIXES.** No logic defect, no hidden weakening, no deleted evidence — five
documentation/print defects, four of them text that now contradicts code the same task changed.

**The single thing I would not ship without:** V-1 + V-2 together — *the `--features xapian` story
still lies about itself in the two places a maintainer reads first* (the run's own summary line, and
`docs/CORPUS-RUNNER.md` §How to widen it). Everything else on this list is cosmetic; those two are the
same defect class the task was opened to fix, and both are one string each.

---

### Residual risks (mine, after independent verification)

1. **`--features xapian` is a compile gate, not an evidence gate** — 0 match-set comparisons, floor
   not applied, backend a stub. Green there says "it builds and the 3 recorded error classes hold",
   nothing more.
2. **Quoted/phrase/operator classes are guarded by one unit test each**, not by the corpus (the corpus
   has no quoted-range shape). Documented at `CORPUS-RUNNER.md:229-245`; still thin.
3. **No populated xapian index exists on this machine**, so nothing here — including the F3 pre-check —
   has been exercised against a real glass database; the new `postlist.glass` requirement is stricter
   than GN4's `native.rs:551-556` and could in principle refuse an index GN4 accepts (it will not
   refuse a normal one: every real glass DB has `postlist.glass`).
4. **Docs staleness the task chose to leave** (`docs/WORKER-BLOCKERS.md` §8.4/§8.5,
   `docs/VERIFY-FINAL.md` §1 which now reports the *pre-fix* red xapian gate). A maintainer reading
   those before the README will believe the opposite of the current state.
5. **Clippy is not clean** (pre-existing, disclosed in `WORKER-GATE-TRUTH.md` risk 5); `fmt` is clean
   and `rustc` emits 0 warnings, both re-measured.
