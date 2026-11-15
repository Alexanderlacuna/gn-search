# WORKER-CRATE — the `gn-search` crate, as delivered

Report for the build task: files written, what is stubbed, and the six
places a reviewer should check first.

Written after reading, in order, `project5/genenetwork4/docs/dev/SKILL.md`
and `.pi/skills/gn4-local/SKILL.md`, then `docs/SCOUT-NATIVE.md`,
`SCOUT-PROXY.md`, `SCOUT-INDEXER.md`, `SYNTHESIS.md`, `BASELINE.md`. Every
module header names the SCOUT section it implements; `docs/DECISIONS.md`
has one row per choice with the alternative rejected.

---

## 1. Files written

All under `/home/kabui/gn-search`; nothing outside it, nothing inside
`project5/genenetwork4`.

| File | Lines | What it is |
|---|---|---|
| `Cargo.toml` | 36 | package + `serde` 1.0.229 / `serde_json` 1.0.151 (GN4's own pins); features `xapian`, `serve`, both dependency-free |
| `src/lib.rs` | 114 | `pub fn search(index_path, q, type_, limit, page) -> Result<Vec<SearchHit>, SearchError>`; render-then-open order; the backend is chosen by feature |
| `src/hit.rs` | 103 | `SearchHit`: the 18 named fields of SCOUT-NATIVE §4 verbatim + `#[serde(flatten)] extra: BTreeMap<String, serde_json::Value>` — GN4's own type, so key order cannot depend on feature unification (DECISIONS row 19) |
| `src/error.rs` | 96 | `SearchError::{InvalidQuery{message, xapian_error_type}, Unreachable, Misbehaving, Backend, Internal}` + `Display`/`Error`/`message()`/`xapian_error_type()` |
| `src/grammar.rs` | 753 | the grammar as DATA (`GRAMMAR`: languages, 8 boolean prefixes, 3 probabilistic + 2 aliases, 5 numeric slots, position slot 2 ±50 kb, operators) and a pure renderer producing `(prefix, term, kind)`; no xapian; 20 unit tests |
| `src/index/mod.rs` | 63 | `trait Index { open(path); search(&ParsedQuery, limit, page) }`, `offset_for_page`, `to_usize`; the contract is the FULL match set from the page's offset, with truncation left to the caller (DECISIONS row 33) |
| `src/index/fixture.rs` | 433 | in-memory index over the **five recorded alcohol hits**, honouring `XT{type}` and the page's offset — `limit` places the offset and is NOT a cap on the result (DECISIONS row 33); 5 unit tests |
| `src/index/xapian.rs` | 177 | `#[cfg(feature="xapian")]` STUB: empty path / not a directory / no `iamglass` (+ the stat failure), GN4's message text, then `unimplemented!` naming the missing binding; 5 tests |
| `src/manifest.rs` | 157 | reads `<index>.sidecar.json`: v3, and the v4 `build_id`/`built_at` shape; missing → `Ok(None)`; `age_seconds` from the sidecar's mtime |
| `src/bin/gn-search.rs` | 384 | `#[cfg(feature="serve")]` std-only dev server: `/search` (bare JSON array), `/index` (manifest + age), `/healthz`; 400/404/405/422/500/503 with named reasons — 503 `index_unavailable` for an unopenable index, 500 `internal_error` for a build with no backend (DECISIONS rows 28, 32) |
| `tests/grammar_render.rs` | 269 | one case per prefix and per slot through the renderer + the 422 for a bad numeric bound |
| `tests/hit_roundtrip.rs` | 118 | fixture round-trip, every named field read, unknown key survives, absent→`null`, `null` in `extra` |
| `tests/error_status.rs` | 119 | the 422/502/502/502/500 table (exhaustive match: a new variant does not compile) |
| `tests/manifest.rs` | 185 | v3 load, missing, v4-with-`built_at`, unknown version, corrupt, versionless, path derivation |
| `tests/fixtures/search_hit.json` | 31 | one recorded hit (trait 17985, BXD serotonin) + the route's `lod`/`locus`/`abbreviation` + `pubmed_link` |
| `README.md` | 143 | what exists, what is stubbed, how to run the tests (cargo gate **and** the rustc harness actually used) |
| `docs/DECISIONS.md` | 67 | 29 rows: decision, alternative rejected, why, report line |

## 2. What is stubbed

1. **The xapian backend** (`src/index/xapian.rs`, `--features xapian`).
   It does the pre-checks GN4 needs before it may touch a binding whose
   `Database::open` *aborts the process* — empty path → `Internal("XAPIAN_DB_PATH
   is not configured; …")`, stat failure / not a directory / no `iamglass`
   marker → `Unreachable("xapian index at {path} …")`, all with GN4's
   wording (SCOUT-NATIVE §2) — and then `unimplemented!`s, naming
   `xapian_rs` (the git-pinned fork with `parse_query_checked`, the
   `NumberRangeProcessor` f64 fix, the decline→`MatchNothing` bridge and a
   `Box::leak`ing `FieldProcessor`; SCOUT-NATIVE §8) as the thing that has
   to be wired in. It is loud on purpose: a silent `Err` would let a
   deployment treat "no backend" as a runtime condition.
2. **The dev HTTP server** (`--features serve`). It works (see §3) but is a
   debugging shell, not the production surface: `std::net`, one thread per
   connection, no auth, no envelope, no TLS, no `lod`/`locus`/`sort`. GN4
   owns that route (SCOUT-NATIVE §8, SYNTHESIS §3/§6). Without the feature
   the binary prints why and exits 2.
3. **Not implemented anywhere in the crate** (deliberate, see
   `docs/DECISIONS.md`): `Hs:`/`Mm:` location shorthands and liftover
   (outside the assigned grammar; GN4's native engine has no liftover
   either, OPEN-4 — **needs an owner answer before this crate could be a
   parity drop-in**), BM25 parameterisation/boosting, the `tissue:`/
   `peakchr:`/`rif:`/`wiki:`/`peakmb:` matching in the *fixture* (those
   fields are not on `SearchHit`), and any database enrichment.

## 3. Validation actually performed

`cargo` was never run (the hard rule, and `BASELINE.md` §1: `cargo` on
`PATH` has no usable `rustc`). Instead the delivered tree was copied to
`/tmp` and compiled with `rustc 1.88.0`
(`~/.rustup/toolchains/1.88.0-x86_64-unknown-linux-gnu/bin/rustc`), linking
the `serde`/`serde_json` rlibs already in GN4's
`target/debug/deps` (same crate versions this crate pins: serde 1.0.229,
serde_json 1.0.151). Everything below ran and passed:

| Build | Result |
|---|---|
| `rustc --test src/lib.rs` (default features) | **27 passed**, 0 failed |
| `rustc --test src/lib.rs --cfg 'feature="xapian"'` | **32 passed**, 0 failed (includes the 4 pre-check tests and the `#[should_panic]` one) |
| `tests/grammar_render.rs` | **14 passed** |
| `tests/hit_roundtrip.rs` | **5 passed** |
| `tests/error_status.rs` | **4 passed** |
| `tests/manifest.rs` | **7 passed** |
| `src/bin/gn-search.rs` — default build | compiles; prints "built without the `serve` feature", exit 2 |
| `src/bin/gn-search.rs --cfg 'feature="serve"'`, fixture backend | compiles; smoke-tested with `curl`: `/healthz` 200 `ok`; `/search?query=alcohol&limit=2&page=1` → the 5 recorded traits (`10199` first); `page=3` → `10211`; `type=gene` → `[]`; `query=` → 400; no `query` → 400; `mean:abc..def` → 422 `invalid_query` "Unknown range operation"; `/nope` → 404; POST → 405; `%3A` decoding works; `/index` with a v4 sidecar → 200 `{manifest, age_seconds}`; `/index` with no index → 503 `index_unavailable` |
| `src/bin/gn-search.rs --cfg 'feature="serve"'`, xapian stub backend, `XAPIAN_DB_PATH=/tmp/does-not-exist` | compiles; `/search` → **503** `{"error":"index_unavailable","detail":"xapian index at /tmp/does-not-exist could not be opened: No such file or directory (os error 2)"}` — a named reason, never an empty array |
| `rustfmt --check --edition 2021` on every `.rs` file (1.88.0's rustfmt) | clean |

One real bug was caught this way and fixed: a unit test in `src/hit.rs`
asserted `extra` was empty for a JSON body that contained an unknown key
(`gen_chr`). That is the argument for the harness — see the README for how
to repeat it without cargo.

**Not covered by any of this:** cargo's own resolution and lockfile,
doctests, clippy, and the xapian binding itself (there is none). The
`--features xapian` build only proves the stub module compiles and its
pre-checks behave.

## 4. The six places I am least sure of

These are ordered by how much a wrong guess would cost, and each says what
would settle it.

1. **`#[serde(flatten)] extra` type — RESOLVED.** The field is
   `BTreeMap<String, serde_json::Value>`, GN4's own type (`src/hit.rs`;
   DECISIONS row 19), so a GN4 `SearchHit` literal and this one agree
   field-for-field and `serde_json/preserve_order` being enabled anywhere in
   a workspace cannot reorder it. The round-trip tests pass unchanged, as
   predicted here.
2. **Whether a new backend must convert slot-2 units again
   (`src/grammar.rs`, `PositionRule`).** The renderer converts position
   bounds from bases to **megabases** because slot 2 holds megabases
   (SCOUT-NATIVE §3 / `add_mb`; GN4's own `position_value_query` divides by
   10^6 at query time). A backend that takes `TermKind::ValueRange{slot:2,
   …}` and divides by 10^6 *again* would be wrong by 10^6 in every position
   query, and no test here can catch that (the fixture compares against
   `geno_mb`, which is already megabases). Settle it when the xapian
   backend is written: compare one `position:` query against GN4's native
   engine on the dev index.
3. **The numeric-range forms no recorded shape pins (`src/grammar.rs`,
   `parse_range`).** Three judgement calls: a bare `mean:5` renders as the
   point range `5..5`; `mean:` with an empty value is a `RenderError`
   (422); `mean:..` is a range with no bounds (matches nothing, 200). The
   recorded corpus pins only `mean:abc..def` → 422 (Q52) and the Q53
   `position:` empty form. If the parity harness ever grades these three,
   the answers come from xapian, not from this file.
4. **The bin's `Unreachable → 503` while GN4 maps the same variant to 502
   (`src/bin/gn-search.rs::status_for`).** Deliberate: the index is this
   process's local state, and the requirement is a named 503 when the index
   is absent. It is also a second, bin-local copy of the status table (the
   crate itself exports no mapping; the canonical one is in GN4's route and
   mirrored by `tests/error_status.rs`). If a reviewer would rather have one
   table, the crate-level table is already in `src/error.rs`'s doc comment
   and in the test file — the bin's copy exists only to keep the numbers
   out of the library API.
5. **The xapian stub's message strings (`src/index/xapian.rs`).** They are
   GN4's `native.rs` text, verbatim, including the parenthetical
   `(SEARCH_ENGINE=native)` — which names a GN4 config key this crate never
   reads (the backend is a build feature here). I chose message parity with
   what operators see today over accuracy about this crate's own selection
   mechanism; the alternative is a crate-specific wording that would change
   a 500 body GN4 currently emits verbatim.
6. **The manifest's tolerance policy (`src/manifest.rs`).** An unsupported
   version returns `Ok(None)` — indistinguishable from "no manifest" — while
   bytes that are not JSON, or JSON without a numeric version, are
   `Misbehaving` (502). That split is my reading of SYNTHESIS §5 step 4
   ("old manifests → absent, not an error", "responses succeed unchanged
   with no sidecar present") plus SCOUT-INDEXER §8's field list; the
   alternative reading (any unusable manifest is an error) is defensible and
   would only touch `read()` and `tests/manifest.rs`.

Two smaller ones worth a glance: `age_seconds` uses the sidecar's **mtime**
(the manifest is written last, so its mtime bounds build completion) rather
than parsing `built_at`, because a hand-written timestamp parser is a
wrongness risk with no date crate available; and the grammar's renderer is
token-level (a value ends at whitespace, grouping is not modelled,
`ParsedQuery::raw` keeps the query for the backend's own parser).

## 5. Notes for whoever reviews next

- **Finding:** SYNTHESIS §2b says the hit struct has "19 named fields"; the
  struct it quotes (SCOUT-NATIVE §4, GN4 `src/search/mod.rs:66-95`) has
  **18** (name, dataset, dataset_fullname, group, species, authors, year,
  description, mean, additive, lrs, lod, locus, abbreviation, geno_chr,
  geno_mb, inbredsetcode, pubmed_id). `src/hit.rs` follows the verbatim
  struct. A report miscount, not a discrepancy in the code.
- The fixture is the **five recorded alcohol hits** and nothing else: no
  synthetic gene document, so the `XT{type}` filter is proven by exclusion
  (a `gene` query returns `[]`). If a future test needs a `gene` hit, it
  needs a recorded one, not an invented one.
- Everything the crate deliberately does *not* own is listed once in
  `src/lib.rs`'s module doc (envelope, `lod`, enrichment, `sort`/`order`,
  400-validation, status mapping) with the report lines; `docs/DECISIONS.md`
  rows 1–4 are the same list from the "alternative rejected" side.
- The next honest step is not more code here: it is (a) an owner answer on
  `Hs:`/`Mm:` (DECISIONS row 13), and (b) the xapian binding behind
  `src/index/xapian.rs`, compared against GN4's native engine on the dev
  index for the two unit questions above (slot-2 units, range forms).
