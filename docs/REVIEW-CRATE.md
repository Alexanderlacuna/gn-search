I have everything I need. Here is the review artifact (I have no write-capable tool in this role — bash is read-only — so the runtime persists this).

---

# REVIEW-CRATE — independent review of the `gn-search` crate

Reviewer: alt-family adversarial pass (artifact authored by a GLM-family lane).
Scope reviewed: `Cargo.toml`, `src/**`, `tests/**`, `docs/DECISIONS.md`,
`docs/WORKER-CRATE.md`, `README.md` against ground truth
`/home/kabui/project5/genenetwork4` (read-only) and the scout reports
(`SCOUT-NATIVE.md`, `SCOUT-PROXY.md`, `SCOUT-INDEXER.md`, `SYNTHESIS.md`,
`BASELINE.md`). No `cargo`, no writes outside this file.

**Verdict: PASS-WITH-FIXES** — not yet a parity drop-in. Three of the four
lower layers (hit, error, manifest) verify cleanly against ground truth; the
grammar and the index seam carry real divergences from `native.rs`, and the
default build can fabricate search results. Nothing here is false-green: every
claim in `WORKER-CRATE.md` that can be checked statically is true.

---

## 1 · GRAMMAR FIDELITY

Checked field-by-field against `src/search/native.rs` (GN4) and the 54-shape
recorded corpus (`test/data/parity-fixtures/Q01…Q54`).

**Covered, correctly:**

| Requirement | Crate | GN4 |
|---|---|---|
| `A/XS/XG/XI/XDS/XY/XC/XPC` | `grammar.rs:154-200` | `native.rs:120-129` |
| `XD/XRF/XWK` + `RIF`/`WIKI` aliases | `grammar.rs:201-218` | `native.rs:134-138` |
| slots 0/1/3/4/5 (`mean:`,`peak:`,`peakmb:`,`additive:`,`year:`) | `grammar.rs:219-245` | `native.rs:58-63` |
| `position`/`pos` on slot 2, ±50 kb, clamp at 0, bases→Mb | `grammar.rs:522-553` | `native.rs:225-252` |
| `en` + `STEM_ALL_Z`, stem prefix `Z` | `grammar.rs:139-145` | `native.rs:106-108` |
| lowercase-on-value, all eight boolean prefixes | `grammar.rs:161` etc. | `native.rs:111-129` (GN4 also lowercases all eight) |
| operators `AND/OR/NOT/XOR/NEAR/ADJ`, uppercase only | `grammar.rs:224` | `mod.rs:50-52` |
| numeric bound that is not a number → `Unknown range operation`/`QueryParserError` | `grammar.rs:436-441` | `native.rs:370-411` (Q52) |

**Divergences and omissions (all real, all citable):**

1. **`Hs:` / `Mm:` absent entirely** — `grep -n 'Hs\|Mm' src/grammar.rs` → no
   hits; the fall-through at `grammar.rs:474-479` renders them as free text.
   GN4 registers both (`native.rs:151-170`, `LocationProcessor` at
   `native.rs:310-350`). This is not just OPEN-4: OPEN-4 is about *liftover*,
   and GN4 ships `Hs`/`Mm` **without** liftover (`native.rs:151-156`).
   Graded consequences: **Q40** `Hs:chr2:1M..1.2M` (4296 hits) and **Q41**
   `Mm:chr1:5M` (491) become ~0 hits; **Q42** `Hs:foo:1M` (recorded 500,
   AMENDMENT-E 422 `invalid_query`) becomes a 200. DECISIONS row 13 declares
   this open — correct to flag, but the justification mis-cites OPEN-4.
2. **A bare numeric value is rendered as a point range; GN4 says it is text.**
   `grammar.rs:508-511` (`None => { let point = value.parse::<f64>()…; Some((Some(point), Some(point))) }`),
   pinned by `tests/grammar_render.rs:79-84` (`year:2013`) and DECISIONS row 9
   ("no recorded shape pins the form"). Ground truth contradicts it:
   `native.rs:722-725` — `assert!(!mean5.contains("VALUE"), "mean:5 is text…")`,
   with the comment "*`mean:5` is a `mean 5` phrase, never a range*". So
   `year:2014` (the corpus's own vocabulary — Q37's `year:2010..2020` returns
   12 442 hits) silently turns from a text query into a slot-5 range.
3. **A non-numeric bound *without* `..` is a spurious 422.** `parse_range` is
   only reached for the whole value, so `mean:abc` / `mean:` / `year:twenty`
   → `RenderError` (`grammar.rs:436-441`, `:498-512`). GN4's guard only
   inspects values containing `..` (`native.rs:389-405`, `value.split_once("..")`)
   → 200. Error-class drift, not covered by any fixture.
4. **An empty field value matches nothing instead of being text.** `position:`
   → `parse_position("")` → `apply_si_suffix("")` → `None` → `(None, None)` →
   `ValueRange` with no bounds (`grammar.rs:453-468`), which every backend reads
   as *match nothing*. Graded shape **Q53** `position:` = 200 with **576 hits**;
   `native.rs:264-270` states it "never reaches the processor" (i.e. free text).
   Same class: `author:` renders the term `A` (empty value).
5. **No syntax-error path at all.** `Grammar::render` is total
   (`grammar.rs:397-483`), so **Q51** `alcohol AND` (recorded 400 → graded 422
   `"Syntax: <expression> AND <expression>"`) returns 200. Of the graded error
   shapes the crate reproduces exactly one (Q52).
6. **Structure (phrases, wildcard, NEAR/ADJ arity, `-`/`+`) is not modelled and
   not delegated.** `TermKind::FreeText` is documented as "the token as written,
   to be stemmed and scored" (`grammar.rs:277-281`), so a backend following that
   doc stems `alcohol*` (Q05, 1197 recorded = same as `alcohol`) and
   `"alcohol dependence"` (Q04) into terms that cannot match. `ParsedQuery::raw`
   (`grammar.rs:290-296`) is carried for exactly this, but no code in the crate
   reads it and nothing tells a backend to prefer it.
7. Operator *semantics* live only in `raw`: the shipped fixture ANDs every term
   and drops operators (`index/fixture.rs:115-130`), so Q07–Q12 classes are
   wrong under the default backend (documented in `fixture.rs:22-30` — disclosed,
   not hidden).

## 2 · HIT COMPATIBILITY

**Yes.** `SearchHit` deserializes GN4-recorded JSON with unknown keys preserved.
Quoted tests:

- `tests/hit_roundtrip.rs:37-41` `recorded_hit_round_trips` — decode + re-encode
  equals the recorded `Value` exactly.
- `tests/hit_roundtrip.rs:64-77` `unknown_key_survives_in_extra` — `pubmed_link`
  kept in `extra` and re-emitted.
- `tests/hit_roundtrip.rs:80-102` — absent named field → `null`;
  `extra` present-with-null stays `null` (absent ≠ null inside `extra`).

I verified the fixture is genuine: `tests/fixtures/search_hit.json` is GN4's
`test/data/search_serotonin_midbrain_bxd.json` hit `17985` byte-for-byte on all
16 recorded keys, plus exactly `lod`, `locus`, `abbreviation` (the route's
computed fields) — i.e. no invented data.

Two caveats:
- The round-trip equality only holds because the fixture carries all 21 keys. A
  *raw* gn3 hit gains three `null`s on re-encode. GN4's own struct does the same
  (`SCOUT-PROXY.md` §3), so this is fidelity, not a bug — but the test name
  overstates what it proves.
- **Absent test:** nothing decodes GN4's *real recorded arrays* (119 hits in
  `search_serotonin_midbrain_bxd.json`, 5 in `search_alcohol_phenotype.json`);
  the fixture index hardcodes the five alcohol hits as structs
  (`index/fixture.rs:230-300`), so a corpus-wide type mismatch would be invisible.
  I ran that check by hand: **124 recorded hits, zero type mismatches** against
  the declared Rust types (`authors` list-of-str, `year` int in 2012–2020 → `u16`
  safe, `pubmed_id` ≤ 24 405 699 → `u32` safe), unknown keys exactly
  `{pubmed_link}`.

## 3 · ERROR PARITY

The five variants are GN4's own; `tests/error_status.rs:50-92` mirrors the table
422 / 502 / 502 / 502 / 500 with codes, and the match is exhaustive so a sixth
variant cannot compile silently. **No variant GN4 cannot map** — all five exist
in `src/search/mod.rs:62-68` and `src/routes/search.rs:84-103`.

What the crate *cannot produce*, which matters for graded error shapes:
`InvalidQuery` with `xapian_error_type: "InvalidOperationError"` (Q42's
AMENDMENT-E contract — see §1.1) and any `InvalidQuery` for a syntax error (§1.5).
`Backend` is unreachable in this crate (it is the proxy path's variant) — dead but
correct vocabulary to keep. `Misbehaving` is reachable only from
`manifest::read` (`manifest.rs:113-136`), never from a search.

## 4 · MANIFEST HONESTY

All three behaviors hold, quoted:

- **Missing → `Ok(None)`**: `manifest.rs:107-109` (NotFound arm) +
  `tests/manifest.rs:80-87` `missing_manifest_is_not_an_error`.
- **Unknown version tolerated**: `manifest.rs:121-128` returns `Ok(None)` before
  any field is read + `tests/manifest.rs:126-135` `unknown_version_is_treated_as_absent`
  (a body that would not even parse as v3).
- **v4 with `built_at` loads**: `tests/manifest.rs:89-124`
  `v4_manifest_with_build_identity_loads` (also proves an unmodelled
  `source_revision` is dropped, not fatal).

Additional checks I made: the struct's required/optional field split matches
GN4's own reader exactly (`src/indexer/sidecar.rs:72-95`: only
`versioning_marker`/`versioning_flavour` are `#[serde(default)]`), so no manifest
GN4 can write can fail this reader. And `age_seconds`'s mtime choice is sound
because incremental writes nothing when nothing changed
(`src/indexer/run.rs:846-847`), so mtime cannot mask a no-op run. The manifest is
never consulted from `search()` (`lib.rs:77-98`), so advisory metadata truly
cannot fail a search — only `/index`.

## 5 · THE FULL-SET PROBLEM

**The crate is proxy-shaped, not native-shaped, and that silently changes
`sort`.** `page_bounds` (`index/mod.rs:54-56`) = `((page-1)·limit, limit)`, and
`FixtureIndex::search` takes exactly `limit` hits (`index/fixture.rs:96-108`).
GN4's native engine instead fetches the **full** match set —
`FULL_MATCH_SET_MAX_ITEMS = u32::MAX`, `mset(first_item, u32::MAX)`
(`native.rs:80`, `native.rs:514`) — and the route truncates
(`routes/search.rs:243`); SYNTHESIS §3 calls that a per-page invariant.

Consequence, concretely: the route's sorted path calls the engine as
`(SORT_FETCH_LIMIT, 1)` = `(1000, 1)` and ranks in-process
(`routes/search.rs:212-219`, `sort_by_lrs` `:113-124`). With native as the engine
it ranks the **whole** match set (Q02 = 57 632); with this crate it ranks the
first 1000 *relevance* hits. So `sort=lrs` answers differently.

To be fair: that behaviour is **exactly what the gn3 proxy does today**
(`per_page=1000`, `SCOUT-PROXY.md` §7.1) and hence what the recorded baseline
is. So the crate is compatible with the *proxy* path and incompatible with
*native.rs*. It does not make the problem worse than the recorded baseline; it
makes the two engines disagree on a contract nothing grades (parity grades clause
(a) at the seam with `ENGINE_FETCH_LIMIT = 1_000_000`, and clause (b) top-10
Jaccard). Either the trait must say which one it is, or `sort` will drift the
moment `SEARCH_ENGINE` flips.

## 6 · WOULD IT COMPILE

I read for the classes rustc raises first and found **no definite compile
error**. The five nearest-miss areas, and why each is fine:

1. **`&'static` out of `&self`** (`grammar.rs:346-352` etc.): `pub fn language(&self, …) -> Option<&'static Language>` works because the field is itself
   `&'static [Language]` — the reborrow resolves to `'static`, so it compiles.
2. **`const GRAMMAR: Grammar` needing `Copy`** (`grammar.rs:126-225`): every
   field is `Copy` (`&'static [T]`, `&'static str`, `u32`, `f64`) and `PositionRule`
   derives `Copy` — consistent. `TermKind` deliberately has no `Eq` (it holds
   `f64`), which is correct.
3. **`#[serde(flatten)] extra: serde_json::Map<…>`** (`hit.rs:75-76`):
   `serde_json::Map` implements `Deserialize` as a map, so flatten works; no
   borrowed fields, no `deny_unknown_fields`, so none of the flatten footguns
   apply. (The *type choice* is a separate, minor finding — see fix 7.)
4. **Trait object / variance questions**: `trait Index: Sized` with an associated
   `open(…, ) -> Result<Self, _>` (`index/mod.rs:20-45`) is explicitly not
   object-safe, and nothing takes `dyn Index` — consistent. `open_index`'s two
   cfg-gated definitions (`lib.rs:104-112`) both exist in every build.
5. **`unimplemented!` in a `Result` fn** (`index/xapian.rs:41-50`): `!` coerces;
   the `path = self.path` named format arg in `:52-58` is legal; the inner
   `#![cfg(feature = "xapian")]` after `//!` docs is legal.

Warnings only (not errors): `--features xapian` leaves `XapianIndex` never
constructed and `src/index/fixture.rs` compiled-but-unused; `tests/manifest.rs`
takes `&PathBuf` (clippy `ptr_arg`).

Caveat on evidence: `README.md:99-135` and `WORKER-CRATE.md` §3 claim a
`rustc 1.88.0` harness run (27/32/14/5/4/7 tests, three binary builds, rustfmt
clean). No artifacts remain in the tree, so I cannot attest that run — only that
the static read finds nothing rustc would reject.

## 7 · WHAT BREAKS FIRST IN REAL USE

**The default build fabricates search results.** `FixtureIndex::open(_path)`
ignores the path entirely and returns the five recorded alcohol phenotypes
(`index/fixture.rs:84-88`), and `open_index` selects the fixture by *build
feature*, not by config (`lib.rs:104-108`). So a build without `--features
xapian` linked into GN4 answers
`search("/var/genenetwork/xapian", "alcohol", "phenotype", 20, 1)` with five
plausible, wrong hits — and `type=gene` with a confident empty array. Contrast
`src/index/xapian.rs:41-50`, which panics instead of lying. This is the failure
mode that costs most: it is *silent*, it looks like data, and the route will
happily wrap it in an envelope with `lod`, `locus`, `abbreviation` and a
`complete` state.

Second: `sort=lrs` (§5). Third: `Hs:`/`Mm:` queries returning zero hits with a
200 (§1.1).

---

## Findings (severity · file:line · why · smallest fix)

**F1 · BLOCKER — `src/index/fixture.rs:84-88`, `src/lib.rs:104-108`.**
Default build answers any real index path with the five recorded hits (see §7).
Smallest fix: `fn open(path: &str)` returns
`Err(SearchError::Internal("no search backend compiled in; rebuild with --features xapian, or use FixtureIndex only in tests"))`
unless `path.is_empty()` (or gate the module behind a `fixture` feature and make
the default build's `open_index` a loud error).

**F2 · MAJOR — `src/grammar.rs:508-511` (+ `:436-441`, tests `grammar_render.rs:79-84`).**
Bare numeric values render as point ranges; GN4 measures them as text
(`native.rs:722-725`). Diverges on `year:2014` (12 442-hit class) and produces a
false 422 for `mean:abc`. Fix: treat a numeric field value as a range **only**
when it contains `..`; otherwise fall through to the free-text arm. Update
DECISIONS row 9 — the ground truth already answers it.

**F3 · MAJOR — `src/grammar.rs:453-468`.**
Empty field values (`position:`, `author:`) collapse to match-nothing; Q53
records 576 hits. Fix: a token whose value is empty is not a field term — render
it as free text (one branch, before the field dispatch at `:433`).

**F4 · MAJOR — `src/grammar.rs` (table `:154-224`, renderer `:433-479`).**
`Hs:`/`Mm:` missing → Q40/Q41 hit-set failures and Q42's error class unreachable.
Fix: add `location_shorthands: [{field:"Hs", species_term:"XShuman"}, {field:"Mm", species_term:"XSmouse"}]`
to `GRAMMAR`, render `Hs:chr2:1M..1.2M` as species-term ∧ `XC2` ∧ slot-2 range
(native.rs:310-350), and add an `InvalidOperationError` render error for the
invalid-location leaf (`message: "InvalidOperationError: Query is invalid (invalid Hs/Mm location field)"`, `xapian_error_type: "InvalidOperationError"`). If the owner's answer (DECISIONS row 13) is "out of scope", record it as a *named parity gap* in `docs/DECISIONS.md` rather than leaving three graded shapes unexplained.

**F5 · MAJOR — `src/index/mod.rs:43-56` (+ `Index` doc).**
The seam is page-sized; GN4's native engine is full-set, and the route's sorted
path depends on that (§5). Fix: say which contract the trait has. Cheapest: add
`fn search_all(&self, q: &ParsedQuery, offset: u64)` (mirroring
`mset(first_item, u32::MAX)`) and have `search()` be `search_all` + `take(limit)`;
then the route's sorted call is correct by construction.

**F6 · MINOR — `src/grammar.rs:397-483`.**
No syntax-error path, so Q51-class 422s are lost and every query is "total". Fix:
after rendering, reject a dangling operator / unbalanced quote or paren with
`InvalidQuery { message: "Syntax: <expression> AND <expression>", xapian_error_type: Some("QueryParserError") }` (the exact recorded diagnostic).

**F7 · MINOR — `src/hit.rs:76`.**
`serde_json::Map` instead of GN4's `BTreeMap` (`search/mod.rs:150-152`). Harmless
today, but if any crate in the eventual GN4 workspace enables
`serde_json/preserve_order`, feature unification flips `extra` to insertion order
for the whole binary while GN4's field stays sorted. Fix: use
`std::collections::BTreeMap<String, serde_json::Value>` (the round-trip tests pass
unchanged, per `WORKER-CRATE.md` §4.1).

**Not findings** (checked, clean): manifest tolerance policy; `age_seconds`
mtime choice; error-variant completeness; fixture's `limit=0` and past-the-end
paging; `apply_si_suffix`, `parse_position` unit/clamp semantics; `sidecar_path`
trailing-slash trim; the xapian stub's message strings (verbatim GN4, deliberately).

---

## Fixes in priority order

1. `src/index/fixture.rs` — refuse a non-empty index path in the default build (F1).
2. `src/grammar.rs` — bare numeric value is text, not `<v>..<v>`; no 422 without `..` (F2).
3. `src/grammar.rs` — empty field value is free text, not match-nothing (F3).
4. `src/grammar.rs` — add `Hs:`/`Mm:` + the `InvalidOperationError` location error, or record the gap by name (F4).
5. `src/index/mod.rs` — state and implement the fetch contract (full-set tail vs page) (F5).
6. `src/grammar.rs` — syntax-error path for dangling operators/unbalanced quotes (F6).
7. `src/hit.rs` — `BTreeMap` for `extra`, matching GN4 (F7).

---