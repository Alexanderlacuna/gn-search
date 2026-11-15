# BASELINE — can GN4's parity harness run today, hermetically?

Measured 2026-11-13 on this machine, GN4 at `a89781c`. Written after interrupting a scout that spent its budget on `guix time-machine`; the facts below
are the ones that matter, each reproducible with the command shown.

## 1 · The toolchain (this is the blocker, and it is environmental)
```
cargo --version                       cargo 1.99.1 (5f94df478 2026-08-27)           ✓ on PATH (~/.cargo/bin)
rustc --version (same PATH)           ✗ "the 'rustc' binary ... is not applicable to the 'stable-x86_64-unknown-linux-gnu' toolchain"
~/.rustup/toolchains/                 stable-… AND 1.88.0-x86_64-unknown-linux-gnu   ← 1.88.0 has a WORKING rustc
~/.rustup/toolchains/1.88.0-…/bin/rustc --version   rustc 1.88.0 (6b00bc388 2025-06-23)   ✓
~/project5/genenetwork4/target       1.6 GB of artefacts (the tree HAS been built here before)
```
The repo's convention is the guix toolchain (`guix time-machine -C ~/.config/gn4/channels.scm … shell …`, docs/dev/SKILL.md; and the scout's own note:
"the hermetic path is the guix shell (clang-13)"). Two probes of that path were killed after 4+ minutes each without reaching `rustc --version` — it is
not a fast way in, whatever else it is.

## 2 · The build, measured
```
RUSTC=~/.rustup/toolchains/1.88.0-…/bin/rustc cargo test --test search_parity --no-run   → exit 124 at a 400 s timeout
  still compiling DEPENDENCIES at that point (autocxx-bindgen 0.71.1, libloading, zerovec…), i.e. a cold rebuild of the tree, not a test failure.
```
So: **the parity harness is not runnable on this machine inside 400 s.** It needs either a longer window (a cold Rust build of this tree, easily
several×400 s) or the pinned guix/clang toolchain.

## 3 · The harness itself — static facts (no build needed)
```
fixture corpus      test/data/parity-fixtures/       7.1 MB · 54 files (Q01…Q54) · format "reduced-v1"     (also parity-fixtures-dev, 5.2 MB)
override            GN4_PARITY_FIXTURES (optional) — unset => the committed corpus (tests/search_parity.rs:27-30): "runs hermetically, no downloads"
recorded sets       total 230,031 recorded hits across the 54 shapes (51×200 · 2×400 · 1×500); largest Q02 = 57,632 (matches the comment at tests/search_parity.rs:71)
statuses            recorded 200 / 4xx / one 500 (Q42, re-baselined to 422 by AMENDMENT E)
constants           JACCARD_FLOOR = 0.8 (:64) · ENGINE_FETCH_LIMIT = 1_000_000 (:76) · SHAPES![…] (:140) · REDUCED_FORMAT = "reduced-v1" (:868)
bad fixture         fails LOUD: the `expect("id")` / `expect("raw_query")` accessors (:931-933) and `expect("full fixture array serialisable")` (:1062)
```

## 4 · Verdict
```
IS THE BASELINE FREEZABLE TODAY?   NOT ON THIS MACHINE, not in a normal window.
   the one blocking thing: a working, pinned Rust toolchain for this tree — either give the guix shell enough time to come up (it was >4 min without
   ever printing `rustc --version`), or accept a cold build with rustup's 1.88.0 and give it a long window.
WHAT CAN BE DONE ANYWAY (and was): the corpus is committed and self-describing (54 shapes, per-shape identifier lists, recorded statuses), so the
   acceptance test for the extraction does not depend on this machine building gn4 — it needs the CORPUS plus the crate, not gn4 itself.
```

## 5 · The lesson worth keeping
```
two lanes and I burned ~20 minutes compiling the same tree and killing guix probes. Rule for the next baseline attempt: ONE build, one window, some-
body watches it; never two lanes building the same target dir; and never `guix time-machine` as a way to answer "does the test run".
```
