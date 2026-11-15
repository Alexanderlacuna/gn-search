# WORKER-FINDINGS — F1…F7 of `VERIFY-REMAINING.md`, closed

The findings are the seven rows of the table in `docs/VERIFY-REMAINING.md`.
Everything below was **run** on this tree with
`~/.rustup/toolchains/1.88.0-x86_64-unknown-linux-gnu/bin` (cargo / rustc 1.88.0,
clippy 0.1.88, rustfmt 1.8.0-stable), every cargo invocation `--offline` inside
`timeout 900`. Nothing was built or written inside
`/home/kabui/project5/genenetwork4` (it is read only, by the corpus runner).
Nothing was pushed. `git rev-parse --short HEAD` after the work: **3051bf0**.
The working tree is clean except two untracked artifacts (`docs/VERIFY-REMAINING.md`,
this file); nothing is staged.

Three commits close F2–F7, and the F1 fix is a **history** rewrite, not a
prose edit (this file itself is left untracked, like `docs/VERIFY-REMAINING.md`):

| # | finding | closed by |
|---|---|---|
| F1 | two commits do not compile on their own tree | history: `caf6c02`+`0b86e57` squashed into `6e2bd16`, the `pub mod` declarations moved into the commits that carry their module bodies — §2 |
| F2 | the skip reason named "the missing xapian backend" in the build where xapian is compiled in | `tests/parity_corpus.rs` `SKIP_ATTRIBUTION` (`:122`), re-pasted quotes — §3 |
| F3 | the exit rule printed the `MIN_GRADED` floor in the run that does not apply it | `tests/parity_corpus.rs:887-898` — §3 |
| F4 | "the three pre-checks" for a five-check module | `README.md:28` — §4 |
| F5 | the runner "needs no change" for a real backend | `docs/CORPUS-RUNNER.md:368` — §4 |
| F6 | the gate block and the 84/0 table are machine-local | `README.md:104-112`, `:116-119` — §4 |
| F7 | `VERIFY-FINAL.md` still asserted README staleness | two `[CORRECTED 2026-11-15]` blocks — §4 |

---

## 1 · The full gate on the delivered tree (HEAD = 3051bf0)

| check | result |
|---|---|
| `cargo fmt --check` | exit **0** |
| `cargo build --offline` | exit **0**, **0** warnings |
| `cargo test --offline` | **84 / 0** — lib 32, bin 0, error_status 4, grammar_render 26, hit_roundtrip 6, manifest 7, parity_corpus 9, doc 0 |
| `cargo test --offline --features xapian` | **88 / 0** — lib **38**, parity_corpus **7**, every other target unchanged |
| `cargo clippy --offline --all-targets` | **0** warnings |
| `cargo clippy --offline --features xapian --all-targets` | **0** warnings |
| corpus, `--nocapture`, default | see the corrected line below |
| corpus, `--nocapture`, `--features xapian` | see the corrected lines below |

The corrected corpus summary lines, verbatim from `cargo test --offline --test
parity_corpus -- --nocapture` (default, then the same command with `--features
xapian`):

```
54 shapes seen: 5 graded a set-equality/Jaccard comparison, 3 graded an error class, 7 VACUOUS (expected set empty: no evidence), 39 SKIP (the missing xapian backend's: no xapian index is compiled into this build, see docs/CORPUS-RUNNER.md); 8 PASS, 0 FAIL
exit rule: any FAIL fails the test, and so does fewer than MIN_GRADED = 5 graded comparisons; PASS, VACUOUS and SKIP alone are not evidence

54 shapes seen: 0 graded a set-equality/Jaccard comparison, 3 graded an error class, 0 VACUOUS (expected set empty: no evidence), 51 SKIP (the missing fixture backend's: no fixture inventory is compiled into this build, see docs/CORPUS-RUNNER.md); 3 PASS, 0 FAIL
exit rule: any FAIL fails the test, and so does a graded match-set comparison in a build with no fixture backend to grade one against; the MIN_GRADED = 5 evidence floor is not applied to this run

no fixture backend under this feature: 51 shapes SKIP(no-fixture-backend-under-this-feature), 0 graded — a compile-and-error-class gate, not evidence about any match set
```

Those five lines are byte-for-byte what `README.md` (fenced block under "The
default build is the evidence gate") and `docs/CORPUS-RUNNER.md` (under "Under
`--features xapian`") now quote; checked with `diff` against the captured
`--nocapture` output, not by eye.

---

## 2 · F1 — fixed at the history level

### 2.1 The finding was a misreading of bare `pub mod` lines — the defect underneath was real

F1's `file:line` refs (`src/lib.rs:51,54`, `src/index/mod.rs:24`) are the module
declarations themselves:

```
src/lib.rs:51            pub mod grammar;
src/lib.rs:54            pub mod manifest;
src/index/mod.rs:23-24   #[cfg(feature = "xapian")]
                         pub mod xapian;
```

They carry no doc comments, and no doc comment anywhere in the four files F1
names still describes a pre-fix contract: I checked each claim against the code
(the backend text and the fixture's refusal of a non-empty index path in
`src/lib.rs`; the trait, `offset_for_page` and `to_usize` docs in
`src/index/mod.rs`; the v3/v4 version gate and `age_seconds` in `src/manifest.rs`;
the pre-checks' messages in `src/index/xapian.rs`, which I also checked
byte-for-byte against `genenetwork4/src/search/native.rs:536-556`; and the cited
GN4 line numbers `native.rs:80`, `:511`, `:514`, `routes/search.rs:111,214-233`).
**No prose was changed to satisfy F1** — padding doc comments to match a
misreading would have been worse than an empty diff.

What the row *did* report, and what I reproduced before touching anything,
`git archive <sha> | tar -x` into `/tmp/gnw-commits/<sha>` then
`cargo build --offline`:

| old commit | result |
|---|---|
| `caf6c02` | **`error[E0583]: file not found for module 'grammar'`** and **`'manifest'`** |
| `0b86e57` | **`error[E0583]: file not found for module 'manifest'`** |
| `3028835` | clean |

Cause: the `mod` declarations landed in `caf6c02` and the module bodies in
`0b86e57` (`src/grammar.rs`) and `3028835` (`src/manifest.rs`,
`src/index/xapian.rs`). So `git bisect` stepped on build errors, and reverting
`3028835` alone broke the build.

### 2.2 What the rewrite did

Only the three named commits changed content; the other eight keep their trees
and messages **byte-for-byte** (verified: `git diff --stat <old> <new>` empty and
`git log -1 --format=%B <old>` identical, for all twelve 1:1 pairs).

* `caf6c02` + `0b86e57` are **squashed** into `6e2bd16`. They could not be made
  self-consistent by moving declarations: `caf6c02`'s `src/lib.rs` calls
  `GRAMMAR.render`, so the crate does not compile without `src/grammar.rs`, and
  the grammar body is what `0b86e57` adds. A smaller honest history beats a
  larger dishonest one. The squashed message keeps both original bullets and
  both original bodies verbatim (11 of 11 non-blank lines of the two messages are
  present); the subject is `caf6c02`'s bullet, and `0b86e57`'s bullet +
  body are kept as the second entry of the body, which is why `git log`'s
  one-line subject stays inside the house 72-char limit.
* Inside that squashed tree, `src/lib.rs` no longer declares `pub mod manifest;`
  and `src/index/mod.rs` no longer declares `pub mod xapian;` — neither module
  exists yet at that point.
* `3028835` → `a83e298` keeps its tree and message exactly, so its diff now
  *adds* those two declarations (`+pub mod manifest;`, `+#[cfg(feature =
  "xapian")]` / `+pub mod xapian;`) together with the module bodies they name.
  Every `pub mod` in the new history lands in the same commit as its module.
* `3028835..2e8f69e` replay unchanged; `docs/WORKER-REMAINING.md` reports
  `3028835` as 65 / 0, and the replayed `a83e298` measures 65 / 0.

Removing `pub mod xapian;` from the squashed tree was not optional: `cargo fmt
--check` fails at a tree that declares a module whose file is absent ("Error
writing files: failed to resolve mod `xapian`"), and rustfmt does not evaluate
`cfg`.

### 2.3 Old → new SHA map

| old | new | what changed |
|---|---|---|
| `caf6c02` + `0b86e57` | **`6e2bd16`** | squashed; `pub mod manifest;` / `pub mod xapian;` removed (their bodies arrive later) |
| `3028835` | **`a83e298`** | tree + message identical; diff now adds the two declarations |
| `e1f661e` | `6a16ce4` | unchanged |
| `2ebbfa4` | `83f0be5` | unchanged |
| `6c00fd2` | `f425901` | unchanged |
| `8504e69` | `2151a56` | unchanged |
| `e9f8267` | `5e48455` | unchanged |
| `5c436bb` | `aec5877` | unchanged |
| `566bba2` | `280a438` | unchanged |
| `12d36fc` | `c0b4ec3` | unchanged |
| `37534fe` | `478bbf7` | F2/F3 (this pass) |
| `7e28c66` | `9343458` | F4/F5/F6 (this pass) |
| `2e8f69e` | `3051bf0` | F7 (this pass) |

The pre-rewrite history is kept locally as branch `backup-before-rehist` (14
commits, HEAD `2e8f69e`). Nothing was pushed anywhere; the old history is not
reachable from `master` and can be deleted with `git branch -D
backup-before-rehist`.

### 2.4 Per-commit proof of the new history

For **every** commit in the new history, not just HEAD: `git archive <new-sha> |
tar -x` into a fresh `/tmp/gnw-hist/<sha>`, `CARGO_TARGET_DIR=/tmp/gnw-t/<sha>`
(out of tree), then `cargo fmt --check`, `cargo build --offline`,
`cargo test --offline`, `cargo test --offline --features xapian`.

| new | subject | fmt | build | test (pass/fail) | `--features xapian` |
|---|---|---|---|---|---|
| `6e2bd16` | `* src (gn-search): the engine seam with a fixture backend.` | 0 | 0 | **58 / 0** | **101** — `E0433: could not find 'xapian' in 'index'` |
| `a83e298` | `* src (index): the xapian pre-checks and the sidecar manifest reader.` | 0 | 0 | 65 / 0 | 0 |
| `6a16ce4` | `* tests (parity): grade the committed corpus without building GN4.` | 0 | 0 | 84 / 0 | 0 |
| `83f0be5` | `* src/bin/gn-search.rs (serve): an optional dev server behind a feature.` | 0 | 0 | 84 / 0 | 0 |
| `f425901` | `* docs (gn-search): the scout reports, decisions, and what is not done.` | 0 | 0 | 84 / 0 | 0 |
| `2151a56` | `* Cargo.lock (deps): pin the two offline-resolvable dependencies.` | 0 | 0 | 84 / 0 | 0 |
| `5e48455` | `* docs (correction): the pre-fix red gate in two review reports.` | 0 | 0 | 84 / 0 | 0 |
| `aec5877` | `* src (clippy): nine lint fixes, one recorded as debt.` | 0 | 0 | 84 / 0 | 0 |
| `280a438` | `* README.md, docs (gate): the feature build is not the evidence gate.` | 0 | 0 | 84 / 0 | 0 |
| `c0b4ec3` | `* docs/WORKER-REMAINING.md (report): the four remaining items, closed.` | 0 | 0 | 84 / 0 | 0 |
| `478bbf7` | `* tests/parity_corpus.rs (report): name the absence this build has.` | 0 | 0 | 84 / 0 | 0 |
| `9343458` | `* README.md, docs (claims): the gate's preconditions, the widening path.` | 0 | 0 | 84 / 0 | 0 |
| `3051bf0` | `* docs/VERIFY-FINAL.md (F4, verdict): dated corrections, additions only.` | 0 | 0 | 84 / 0 | 0 |

The default-feature gate — fmt, build, test — is green on all thirteen commits,
which is what the old history failed at two of them. The single remaining red
cell is the **feature** column of the oldest commit, and it is honest rather
than a defect: `6e2bd16` predates the xapian backend, so `--features xapian`
there fails with `E0433: could not find 'xapian' in 'index'`. The feature's
module, stub and `#[cfg]` wiring arrive in `a83e298`, the very next commit, and
the column is green from there on. (The old history had the same property at
`caf6c02` and `0b86e57` *and* failed the default gate at both.)

---

## 3 · F2 / F3 — the corpus runner's own sentences

The source of the sentence was fixed, not the quotes.

**F2.** `tests/parity_corpus.rs` now carries

```rust
const SKIP_ATTRIBUTION: &str = if FIXTURE_BACKEND {
    "the missing xapian backend's: no xapian index is compiled into this build"
} else {
    "the missing fixture backend's: no fixture inventory is compiled into this build"
};
```

(`:122`) and the summary print interpolates it (`:877`), so each build names the
absence *it* has. The module doc (`:36-41`) says the same, and no longer claims
"every skip is attributable to the missing backend (no xapian is compiled into
this build)". The per-shape `SKIP` reasons were already correct — they only
print in the default build, where `needs_xapian = "no xapian backend is compiled
into this build"` is true — so they are untouched.

**F3.** The exit-rule print is now the rule the code actually enforces (`:887-898`):
the default build prints the `MIN_GRADED = 5` floor; the feature build prints
"any FAIL fails the test, and so does a graded match-set comparison in a build
with no fixture backend to grade one against; the `MIN_GRADED = 5` evidence floor
is not applied to this run", which is exactly the two assertions that follow it
(`failed.is_empty()`, `counts.graded == 0`). The trailing "no fixture backend
under this feature" note (`:922`) keeps its measurement and drops the floor
clause it no longer owns — which is what let both docs quote the feature block
whole instead of dropping the line that contradicted it.

Re-pasted, byte-for-byte: `README.md` (the two fenced runs under "The default
build is the evidence gate") and `docs/CORPUS-RUNNER.md` (under "Under
`--features xapian`"), each with a sentence about which absence each parenthetical
names. `README.md`'s prose "under `--features xapian` … the evidence floor does
not apply" and `docs/CORPUS-RUNNER.md:18-25` still agree with the printed lines.

## 4 · F4 / F5 / F6 / F7 — the doc claims

* **F4** — `README.md:28` now reads "the pre-checks, then unimplemented!". The
  count is gone rather than restated: `src/index/xapian.rs` makes five checks
  (empty path, stat failure, not a directory, no `iamglass`, no
  `postlist.glass`), and the README's own "What is stubbed" paragraph describes
  them without a number, so nothing is left to disagree with.
* **F5** — `docs/CORPUS-RUNNER.md:368` is now "How to widen it — the next step,
  and it is not a no-op". It states the mechanism (`FIXTURE_BACKEND =
  cfg!(not(feature = "xapian"))`, `tests/parity_corpus.rs:101`, so the feature
  that brings a real backend is the one that switches grading off), says what
  must change (a real inventory behind the same `Index` trait, replacing
  `fixture_inventory()`; and the `FIXTURE_BACKEND` gate removed), and marks it as
  the next step. The false "needs no change" sentence is gone.
* **F6** — `README.md:104-112` states the two preconditions of the gate block
  (the pinned `1.88.0` toolchain with its `bin` first on `PATH`; the corpus at
  `DEFAULT_CORPUS`, an absolute path into GN4's checkout, or `GN4_PARITY_FIXTURES`),
  including what happens elsewhere ("the `parity_corpus` target panics with 'no
  Q*.json fixtures in …', and the default gate is red out of the box"), and the
  measured table's caption (`:116-119`) now says "on that toolchain, with that
  corpus".
* **F7** — `docs/VERIFY-FINAL.md` gets two `[CORRECTED 2026-11-15]` blockquotes:
  after F4 (`:239`) and after the verdict's first paragraph (`:271`). Additions
  only — `git show 3051bf0 --numstat` is `17 0` — the original text, including
  the stale counts and the "red in seven tests" verdict, stays readable as the
  record, exactly the convention §1 of that file already uses.

## 5 · Residual risks

1. **The oldest commit is not green under `--features xapian`** (§2.4). It is the
   commit that predates the backend, and the failure is `E0433` naming that
   absence. Making it green would need code that belongs to the next commit.
2. **The feature build still grades nothing** even with a hypothetical backend:
   `FIXTURE_BACKEND` keys *can grade* to the absence of the feature (F5). That
   is now documented as the next step, not fixed — it needs a real inventory
   source, which needs the binding.
3. **No populated xapian index exists here**, so the five pre-checks and the
   phrase-class requirement stay unexercised against a real glass database.
4. **The corpus is an absolute path into another checkout** (`DEFAULT_CORPUS`,
   `tests/parity_corpus.rs:92`); the green gate is machine-local until it is
   parameterised. Named in `README.md` (F6), not fixed.
5. **`docs/WORKER-CRATE.md:76-79` and §4.3 are still stale** (27/32 test counts,
   the `mean:5` reading). Out of scope for F1–F7; named here so it is not silent.
6. **The rewrite is local only.** `backup-before-rehist` holds the pre-rewrite
   history; if anything is pushed, the new SHAs are what get pushed.
