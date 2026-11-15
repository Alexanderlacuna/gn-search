# SYNTHESIS — GN4 search: delegate to native, keep the contract frozen

Decision document. Sources: SCOUT-NATIVE.md, SCOUT-PROXY.md, SCOUT-INDEXER.md
(all in `/home/kabui/gn-search/docs/`), cross-checked read-only against
GN4 `/home/kabui/project5/genenetwork4` and GN3 `/tmp/gn3-main`.
Every claim cites `repo file:line` or a scout report. Anything not verifiable
from those is marked **UNVERIFIED**.

Naming note: the task calls the engine switch `SEARCH_MODE`; the code's actual
config key is **`SEARCH_ENGINE`** (env key, default `"forward"`,
`src/config.rs:281`, dispatcher `src/search/mod.rs:107`). This document uses
the code's name; see Open Question 5 before renaming anything.

---

## 1. THE THREE-LAYER PICTURE

| Layer | Owns today | Key code / evidence |
|---|---|---|
| **Proxy engine** (`src/search/gn3.rs`, 170 lines) | Forwarding `(query, type, per_page, page)` to gn3 `GET {GN3_URL}search/` (`gn3.rs:110-125`; gn3 route `search.py:206-207`, blueprint prefix `/api/search` at `app.py:113`); translating gn3's statuses into GN4 `SearchError` variants, incl. AMENDMENT-E re-baseline of gn3's escaping-exception 500s to 422 (`gn3.rs:39-73,133-146`); decoding gn3's bare JSON array into `Vec<SearchHit>` via serde, unknown keys into `extra` (`gn3.rs:166-169`, `SearchHit` flatten `src/search/mod.rs:150-152`). No auth forwarded, 30 s timeout, no retry (`gn3.rs:74-88,128-131`; `GN3_TIMEOUT_SECS` `config.rs:264,501-503`). | SCOUT-PROXY §1–§3, §6 |
| **Native engine** (`src/search/native.rs`) | The full xapian grammar — stemmer en/STEM_ALL_Z, boolean prefixes (`A`,`XS`,`XG`,`XI`,`XDS`,`XY`,`XC`,`XPC`), probabilistic prefixes (`XD`,`XRF`,`XWK`), numeric range slots 0/1/3/4/5, `position:`/`pos:` slot 2 with ±50 kb expansion, `Hs:`/`Mm:` shorthands *without* liftover (SCOUT-NATIVE §3, mirroring gn3 `search.py:213-273`); per-request `open_database` with pre-validation because `xapian_rs Database::open` aborts the process (`native.rs:520-540`); `FULL_MATCH_SET_MAX_ITEMS = u32::MAX` fetch with `XT{type}` filter (`native.rs:80,514`); decode = `serde_json::from_slice::<SearchHit>` straight from stored doc data (`hit_from_doc_data`, SCOUT-NATIVE §4). Depends on the `xapian_rs` **fork** (`parse_query_checked`, decline→MatchNothing bridge) — SCOUT-NATIVE §8. | SCOUT-NATIVE §2–§4 |
| **Indexer** (`src/bin/gn-indexer.rs`, `src/indexer/`) | Building the glass index GN3-legacy shape: prefix map `XS/XG/XI/XDS/XY/XC/XPC/A/XRF/XWK`, slots 0–6, identity term `Q<doctype>:<name>:<dataset>`, type terms `XTgene`/`XTphenotype` (`src/indexer/schema.rs:14-75`); crawl queries over `Species⋈InbredSet⋈…⋈ProbeSetXRef` / `PublishXRef⋈Phenotype⋈Publication` with `confidentiality < 1 AND public > 0` (`rows.rs:182-243`); three modes — `full` (temp dir + atomic swap, `run.rs:133-176,589`), `incremental` (full re-hash, in place, `run.rs:848`), `delta` (system-versioning window, in place, `run.rs:1209`); sidecar manifest `<index>.sidecar.json` version 3 `{version, mode, doc_count, rif_hash, wiki_hash, versioning_marker, versioning_flavour, shards{label→{file,count,content_sha256}}}` (`sidecar.rs:7-12,47-70`); cron wrappers with flock and 99-min worst-case / 120-min lag SLA budgets (`scripts/gn-indexer-run.sh`, `scripts/gn-indexer-lag.sh`). | SCOUT-INDEXER §1–§5 |

The **route** (`src/routes/search.rs`) sits above all engines and owns the public
contract: envelope, `lod = lrs / 4.604`, PublishXRef `locus`/`abbreviation`
enrichment, `sort`/`order` (fetch 1000, rank client-side, `SORT_FETCH_LIMIT=1000`
at `routes/search.rs:111`), `limit` cap 100, `next` links, and the
`SearchError → ApiError` mapping (SCOUT-NATIVE §4–§6, SCOUT-PROXY §3).

---

## 2. WHAT EXTRACTION MUST PRESERVE

The consumer contract is defined by the parity test and the envelope. Quoted:

**a) Hit identity graded by parity — `(dataset, name)` pairs only.**
`hit_key` (`tests/search_parity.rs:420-431`); clause (a) set equality graded at
the engine seam with `ENGINE_FETCH_LIMIT = 1_000_000` (`search_parity.rs:70-77`,
`comparator::grade_match` `:601`); clause (b) top-10 **Jaccard ≥ 0.8**
(`JACCARD_FLOOR` `:66-69`, `clause_b_pass` `:531-540`) — a set of the first ten,
not order, not any other field (SCOUT-PROXY §4).

**b) The 19 named `SearchHit` fields + `extra` passthrough** (verbatim struct,
`src/search/mod.rs:66-95`): `name, dataset, dataset_fullname, group, species,
authors, year, description, mean, additive, lrs, lod, locus, abbreviation,
geno_chr, geno_mb, inbredsetcode, pubmed_id`, all `Option`, plus
`#[serde(flatten)] extra: BTreeMap<String, Value>`. Named fields collapse
absent-vs-`null` to `null` on re-serialize; unknown keys must survive verbatim
in `extra` (SCOUT-PROXY §3). **UNVERIFIED:** no automated test pins field-level
bytes — only the identity pairs (SCOUT-PROXY residual risk).

**c) The envelope, quoted from `tests/search.rs`:**
- note: `assert!(body["notes"]...contains(&json!("lod = lrs / 4.604")))` (`:215`);
- next links: `assert_eq!(&next[..2], ["/search?q=alcohol&limit=5&type=phenotype&page=2", "/traits/C57BL-6JxC57BL-6NJF2Publish/10199"])` (`:231`) plus root links `/groups`, `/species`, `/mapping-methods`, `/openapi.json` (`src/suggest.rs:287-300`);
- computed: `assert!((lod - lrs / 4.604).abs() < 1e-9)` and descending-lod order (`tests/search.rs:302-307`); `pubmed_id` 24102824, `locus` "rsm10000012461", `abbreviation` "5HT_MB_M" (`:344-348`); `geno_chr` "16" (`:330`);
- empty result: `assert_eq!(body["data"], json!([]))` (`:372`) — 200, not an error;
- sorted paging: page1=100, page2=19, page3=0 (`:377-379`).

**d) Status codes and bodies (route mapping, `src/routes/search.rs::engine_error`,
SCOUT-NATIVE §6, SCOUT-PROXY §6):**
- 400 `invalid_parameter` — empty `q` (`"q must not be empty"`), bad `type`, bad `sort`/`order`, `page < 1` — **with zero upstream calls** (parity surface shapes Q48/Q49, `grade_error_class` `search_parity.rs:641`);
- 422 `invalid_query` — title `"Query rejected by the search backend"`, typed body under `error` with `param: "query"`, `detail: {xapian_error_type}`, message e.g. `"Syntax: <expression> AND <expression>"` (`tests/search.rs:260-263`); recorded gn3 **500** (Q42, `InvalidOperationError`) re-baselined to the same 422 per AMENDMENT E (`search_parity.rs:659-663`); native's invalid `Hs:`/`Mm:` values also land here (`"InvalidOperationError: Query is invalid (invalid Hs/Mm location field)"`, SCOUT-NATIVE §6);
- 502 `backend_unreachable` / `backend_misbehaving` / `backend_error` — title `"Search backend failure"`;
- 500 `internal_error` — e.g. unset `XAPIAN_DB_PATH` message, join failure.

**e) Upstream (gn3-side) bytes the proxy path normalizes — an extracted service
must keep producing them or GN4's classification changes:** gn3's parser-reject
body `{"error_type": "b'QueryParserError'", "error": "<msg>"}` — the
**bytes-repr** form is what the recorded fixtures carry (`gn3.rs:7-22` strips
it; SCOUT-PROXY §7.4); gn3's escaping-exception 500 bodies must keep the
`{"error": "<ClassName>…"}` first-word shape so `looks_like_xapian_exception_class`
(`gn3.rs:39-57`) still classifies them (SCOUT-PROXY §7.4–7.5). gn3's phenotype
`pubmed_link` URL built at `search.py:294-299`
(`http://www.ncbi.nlm.nih.gov/entrez/query.fcgi?cmd=Retrieve&db=PubMed&list_uids=…&dopt=Abstract`)
is not a named `SearchHit` field and survives only via `extra`
(SCOUT-PROXY §4.2). **UNVERIFIED:** whether the native engine reproduces
`pubmed_link` is asserted nowhere (SCOUT-PROXY §4).

**f) Forwarded request shape:** exactly `query`, `type`, `per_page`, `page` as
form-encoded params; `per_page` = engine-level limit (route caps at 100; sorted
mode sends 1000 at page 1, `routes/search.rs:111,214`) (SCOUT-PROXY §7.1).
Parity fails loudly on unmatched forwarded params (`search_parity.rs:1136-1146,1900-1910`).

---

## 3. THE SEAM

The smallest interface that lets GN4 delegate already exists and should not
grow:

**Request/response contract** — the `Engine` trait
(`src/search/mod.rs:188-203`):

```rust
fn search(&self, q: &str, type_: &str, limit: u64, page: u64)
    -> impl Future<Output = Result<Vec<SearchHit>, SearchError>> + Send
```

plus the dispatch `crate::search::search` (`mod.rs:43-51/95-107`). Everything a
consumer sees — envelope, `lod`, `locus`/`abbreviation`, sort, paging, notes,
links, status mapping — lives in the route above this seam
(`src/routes/search.rs:88-135,171-289`) and must stay there (SCOUT-NATIVE §8).
The engine fetches the **full match set** (`FULL_MATCH_SET_MAX_ITEMS = u32::MAX`,
`native.rs:80`) and the route truncates — per-page invariant, PORT-DESIGN §5,
recorded Q02 set = 57 632 hits (SCOUT-NATIVE §5).

**Failure contract** — `SearchError` variants and their route mapping:
`InvalidQuery{message, xapian_error_type}` → 422 `invalid_query` (typed body);
`Unreachable` → 502 `backend_unreachable`; `Misbehaving` → 502
`backend_misbehaving`; `Backend` → 502 `backend_error`; `Internal` → 500
(`src/routes/search.rs::engine_error`, mapping table `src/search/mod.rs:62-68`).
Parse-before-open ordering means a malformed query yields 422 even with a
broken index path — preserve that order (SCOUT-NATIVE §6).

**The switch** — `SEARCH_ENGINE` (not `SEARCH_MODE`): config key default
`"forward"`; anything other than `"native"` selects the gn3 proxy
(`config.rs:281`, `mod.rs:107`, `tests/search_engine.rs`). Flip is one config
line, no code change; `XAPIAN_DB_PATH` is read **per request** inside
`search_blocking` (`native.rs:494-503`), which is what makes index swapping
survivable (SCOUT-INDEXER §7).

The extracted delegate therefore needs: no auth (the proxy path forwards no
`Authorization` — SCOUT-PROXY §5), no envelope, no `lod`/`locus`/`sort`/
`order` handling, and must accept exactly the four-tuple above.

---

## 4. THE INDEX PLAN

**Build by job.** The `gn-indexer` binary is already a standalone job with
three subcommands (`full` / `incremental` / `delta`, `src/bin/gn-indexer.rs:5-9`)
wrapped by cron + flock (`scripts/gn-indexer-run.sh:30-32,90-108`): delta every
10 min, incremental sweep every 3 h. Keep building in the job, never in the
serving process (SCOUT-INDEXER §1).

**Serve by service.** The search service only ever *reads*: it opens
`XAPIAN_DB_PATH` per request (`native.rs:494-503`), knows nothing of the flock
(SCOUT-INDEXER §7).

**Atomic swap.** Only `full` swaps: build into sibling temp dir
`.<name>.tmp.<pid>`, move old to `.<name>.old.<pid>`, rename temp into place,
restore on failure (`run.rs:133-176`). **Known gap:** between the two renames
the path does not exist; a request landing there gets 502 `backend_unreachable`
via `open_database`'s checks (`native.rs:520-540`). `incremental`/`delta` write
in place with `COMMIT_CHUNK_DOCS = 5000` transactional chunks — bounded
staleness, not corruption (SCOUT-INDEXER §3, §7).

**Manifest fields.** The sidecar manifest (`<index>.sidecar.json`) today:
`version: 3, mode, doc_count, rif_hash, wiki_hash, versioning_marker,
versioning_flavour, shards{label → {file, count, content_sha256}}`
(`sidecar.rs:7-12,47-70`), written last so a crash leaves the previous
self-consistent manifest (`run.rs:336-373`, `sidecar.rs:120-146`). For freshness
reporting it is missing, per SCOUT-INDEXER §8:
- `build_id` (unique per run) — not present today;
- `built_at` (UTC wall-clock of swap completion; `versioning_marker` is the DB
  coverage point, *not* build time — `sidecar.rs:12`);
- `source_revision` (indexer git commit) — not present;
- `record_count` — exists as `doc_count` (`sidecar.rs:80`).

**How `index_age` reaches the response.** Since the reader opens per request,
it picks up a swapped manifest automatically. Mechanism: at the natural read
point — alongside `open_database` (`native.rs:520-540`) — parse
`<index>.sidecar.json`, take `built_at` (or fall back to the file's mtime —
**UNVERIFIED:** no field exists to fall back on today), and surface it in the
envelope `meta` via the route (`src/routes/search.rs:171`). Manifest-write-last
means the age shown can never claim a build that didn't finish
(SCOUT-INDEXER §8). Nothing in the query path reports index age today — grep
for `sidecar|index_age|marker` in routes: no hits (SCOUT-INDEXER §6).
**UNVERIFIED:** no measured build durations exist in either repo; the 99-min
worst-case and 120-min lag SLA are budget constants, not measurements
(SCOUT-INDEXER §4, residual risks).

---

## 5. ORDER OF WORK

Each step ships alone and has its own acceptance test.

1. **Freeze the parity baseline (fixture first).** Commit/verify the reduced-v1
   fixture corpus under `test/data/parity-fixtures/` and get
   `tests/search_parity.rs` green hermetically on the current proxy path
   (`search_parity.rs:27-29,1749-1761`). *Acceptance:* `cargo test
   --test search_parity` passes with no env vars, and the report shows all 54
   shapes graded (fixture load failures are loud by construction,
   `search_parity.rs:1023-1025`). No behavior change shipped.
2. **Pin the seam's byte contract.** Parity grades only `(dataset, name)` pairs
   (§2a); add a serialization snapshot test for `SearchHit` + envelope notes at
   the existing engine seam (extend `tests/search.rs` assertions like
   `:344-348` to cover `extra` passthrough and `pubmed_link`). *Acceptance:*
   new test asserts unknown-key passthrough and the `lod`/`locus`/`abbreviation`
   fields for a stub hit; existing tests untouched and green.
3. **Add manifest freshness fields.** Indexer writes `build_id` + `built_at`
   into the sidecar manifest (schema bump to version 4, written last,
   `sidecar.rs:120-146` pattern). *Acceptance:* `cargo test` unit tests in
   `src/indexer/sidecar.rs` assert the new fields serialize and that a v3
   manifest still loads (backward-compat test mirroring the v2 test at
   `sidecar.rs:491-540`); one `gn-indexer full` run produces a v4 manifest with
   both fields populated.
4. **Expose `index_age` in the response.** Read the sidecar next to
   `open_database`; surface in envelope `meta` through the route only when the
   field is present (old manifests → absent, not an error). *Acceptance:*
   route test (env-gated like `tests/search_native_db.rs`) asserts `meta`
   carries the built-at value after a `full` build, and that responses succeed
   unchanged with no sidecar present.
5. **Delegate: flip `SEARCH_ENGINE=native` against the frozen corpus.** The
   dispatcher already selects native (`mod.rs:107`); the parity harness clause
   (a) grades native against the *recorded gn3* corpus, which is exactly the
   native-vs-recording detector (SCOUT-PROXY §4). *Acceptance:* full parity run
   with `SEARCH_ENGINE=native` passes clauses (a) and (b) for all match shapes
   and all error classes (422 mapping incl. Q42 AMENDMENT E,
   `search_parity.rs:659-663`), plus `tests/search_native_db.rs` set-equality
   vs the database (`:217`).

---

## 6. WHAT NOT TO DO

- **Do not grade parity on more than `(dataset, name)` or change
  `JACCARD_FLOOR`.** The 0.8 floor and identity-pair grading are the
  owner-approved metric (`parity-metric.md`, `search_parity.rs:66-69`); widening
  it silently redefines pass/fail against fixtures that were recorded under the
  old rule (SCOUT-PROXY §4).
- **Do not move the envelope, `lod`, `locus`/`abbreviation`, `sort`/`order`,
  or status mapping into the delegate.** These are GN4's public contract
  (`tests/search.rs:215,231,302-307,344-348`; `routes/search.rs:88-135`);
  the seam is the four-tuple of §3 (SCOUT-NATIVE §8).
- **Do not cache the xapian `Database` handle globally.** Per-request open is
  what survives the FULL swap's rename gap and picks up new indexes with zero
  coordination (`native.rs:494-503`; SCOUT-INDEXER §7).
- **Do not "fix" gn3's `b'QueryParserError'` bytes-repr or its `type="gene"`
  default.** Recorded fixtures carry the repr (`gn3.rs:7-15`), and GN4's
  `looks_like_xapian_exception_class` classification depends on the exact
  first-word shape (`gn3.rs:39-57`); changing gn3's defaults breaks untyped
  direct callers (SCOUT-PROXY §4.5, §7.4, residual risks).
- **Do not swap in `incremental`/`delta` modes.** Only `full` is atomic
  (`run.rs:133-176`); in-place writes with chunked commits are the correctness
  design — the 3-hourly sweep is the bound for the unversioned tables
  (`delta.rs:7-12`; SCOUT-INDEXER §5).
- **Do not replace the `xapian_rs` fork with upstream `xapian-rs`.** The whole
  grammar depends on fork-only API: `parse_query_checked`,
  `NumberRangeProcessor` f64 fix, decline→MatchNothing bridge, custom
  `FieldProcessor` (SCOUT-NATIVE §8).
- **Do not forward auth or surface-shaped queries to the delegate.** The proxy
  hop is unauthenticated (SCOUT-PROXY §5), and parity fails loudly if the route
  starts forwarding Q48/Q49 shapes (`search_parity.rs:1282-1283`).
- **Do not change `limit` semantics** (cap 100, `SORT_FETCH_LIMIT = 1000`,
  full-set fetch with route-side truncate, `routes/search.rs:111,214,243`;
  `native.rs:80`). `limit=0` currently yields a 200 with empty `data` and a
  next-link — inferred from code, **UNVERIFIED** by any test (SCOUT-NATIVE §5,
  residual risk); do not "fix" it without pinning the current behavior first.

---

## 7. OPEN QUESTIONS FOR PJOTR

1. Does `index_age` (from the sidecar `built_at`) belong in envelope `meta` or
   in `notes`, and do you approve that as a public-contract addition?
2. Is the delegate a separate HTTP service speaking gn3's bare-array contract,
   or a library crate behind the existing `Engine` trait in-process?
3. GN3 does human→mouse liftover in `Hs:`/`Mm:` handling; GN4 native does not
   (OPEN-4) — is liftover required for native sign-off, or is the current
   species-AND-chromosome semantics final?
4. Is AMENDMENT E final for the native engine too (Q42's recorded 500 → 422
   `invalid_query`), or must native ever reproduce gn3's raw 500s?
5. The lag SLA in `scripts/gn-indexer-lag.sh` defaults to 120 minutes — is that
   the number `index_age` freshness should be judged against publicly?
