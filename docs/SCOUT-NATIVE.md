# SCOUT-NATIVE — GN4 native search path (read-only scout)

Repo: `/home/kabui/project5/genenetwork4` (Rust). GN3 reference: `/tmp/gn3-main`
(not needed for most claims — GN4's code cites gn3's `gn3/gn3/api/search.py` line numbers inline).
No files outside `docs/` were modified.

## 1. ENTRY — request → dispatcher → native

- Route handler: `src/routes/search.rs:171` — `pub async fn search(ctx: Ctx, _token: AuthToken, Query(params): Query<SearchParams>)`. Registered via `src/routes/mod.rs:29` (`pub mod search;`).
- `SearchParams` is defined at `src/routes/search.rs:25-52` (`q`, `type`, `limit`, `sort`, `order`, `page`).
- The handler calls the dispatcher `crate::search::search(...)` at `src/routes/search.rs:225`.
- Dispatcher: `src/search/mod.rs:43-51`:

```rust
pub async fn search(
    q: &str, type_: &str, limit: u64, page: u64,
) -> Result<Vec<SearchHit>, SearchError> {
    match crate::config::Config::load().search_engine.as_str() {
        "native" => native::Native.search(q, type_, limit, page).await,
        _ => gn3::Gn3.search(q, type_, limit, page).await,
    }
}
```

- Config key: **`SEARCH_ENGINE`** (env key, default `"forward"`, `src/config.rs:281`, field `search_engine` at `src/config.rs:42-44`). Anything other than `"native"` → gn3 proxy.
- Both engines implement the `Engine` trait (`src/search/mod.rs:188-203`), whose method signature is `fn search(&self, q: &str, type_: &str, limit: u64, page: u64) -> impl Future<Output = Result<Vec<SearchHit>, SearchError>> + Send`.

## 2. INDEX HANDLE — xapian database opening

- Config key **`XAPIAN_DB_PATH`** (`src/config.rs:34-39`), default empty (`src/config.rs:280`). Field read at call time inside `search_blocking`: `src/search/native.rs` step 3 — `let db_path = crate::config::Config::load().xapian_db_path;`.
- Opening is **per request, not global/lazy-cached** — `open_database(&db_path)?` runs on every search (`src/search/native.rs`, `search_blocking` step 3). The only process-cached object is the `QueryParser` (`OnceLock<Mutex<Parser>>`, `parser()` in `src/search/native.rs:84-90`).
- `open_database` (`src/search/native.rs`, `fn open_database`) pre-validates because the `xapian_rs` binding's `Database::open` **aborts the process** on failure:
  1. empty path → `SearchError::Internal("XAPIAN_DB_PATH is not configured; set it to the xapian index directory to use the native search engine (SEARCH_ENGINE=native)")`
  2. `fs::metadata` failure → `Unreachable(format!("xapian index at {path} could not be opened: {e}"))`
  3. not a directory → `Unreachable("xapian index at {path} is not a directory")`
  4. no `iamglass` marker file (glass backend) → `Unreachable("xapian index at {path} does not look like a glass index (no iamglass marker)")`
  5. then `Ok(Database::open(path, None))`.
- The whole xapian call runs off the async runtime via `tokio::task::spawn_blocking` in `Native::search`; a join failure maps to `Internal("native search task failed to join: {e}")`.

## 3. QUERY BUILDER

Built once per process in `build_query_parser` (`src/search/native.rs`), mirroring gn3 `search.py:213-273`:

- Stemming: `Stem::for_language("en")` + `StemStrategy::AllZ` (STEM_ALL_Z) — free-text terms are stemmed (`Z…` terms) and case-insensitive.
- Boolean prefixes via custom `LowercaseProcessor` (lowercases the value before forming the term, because the index stores only lowercase boolean terms):
  - `author`→`A`, `species`→`XS`, `group`→`XG`, `tissue`→`XI`, `dataset`→`XDS`, `symbol`→`XY`, `chr`→`XC`, `peakchr`→`XPC` (`NUMERIC...`/registration array in `build_query_parser`).
  - gn3 lowercases only `group` (`search.py:204-209`); GN4 lowercases all values — parity-neutral since graded shapes use lowercase values.
- Probabilistic prefixes (BM25-scored weighted fields): `description`→`XD`, `rif`/`RIF`→`XRF`, `wiki`/`WIKI`→`XWK`. Parsed terms are `ZXD…`, `ZXRF…`, `ZXWK…` under STEM_ALL_Z.
- Numeric range prefixes via `NumberRangeProcessor` with value slots: `mean:`→slot 0, `peak:`→1, `peakmb:`→3, `additive:`→4, `year:`→5 (`NUMERIC_RANGE_PREFIXES`). `position:`/`pos:` use a custom `PositionProcessor` on **slot 2** (megabases; SI suffixes k/m/g; points expand ±50 000 bases clamped at 0 — `HALF_WINDOW_BASES`, `BASES_PER_MB`).
- `Hs:`/`Mm:` location shorthands → `LocationProcessor`: AND of `XShuman`/`XSmouse` + `XC<chr>` + slot-2 range. **No human→mouse liftover** (OPEN-4). Invalid values emit the sentinel term `GN4InvalidLocationLeaf`, detected post-parse and turned into a 422 (see §6).
- App-level `guard_numeric_ranges(q)` re-checks unparseable range bounds (`mean:abc..def` etc.) after the parser's decline→MatchNothing bridge, mapping them to 422.
- **BM25 params / boosting: not found.** No `set_weighting`, no explicit BM25 parameter, and no boosting anywhere in `src/search/` or the route (grep for `bm25|weighting|boost` hits only comments in `src/search/mod.rs:37` and `src/search/native.rs:128`). The `Enquire` default weighting (xapian BM25 defaults) applies implicitly.
- Query execution (`search_blocking` steps 4–5): `Query::combine(Operator::Filter, parsed, Query::term(format!("XT{type_}")))` — type filter is an `XT` term (`XTphenotype` / `XTgene`). Then `Enquire::new(&db).set_query(&filtered, None)` and `enquire.mset(first_item, FULL_MATCH_SET_MAX_ITEMS, None, None)`.

## 4. RESULT SHAPE

Per-hit struct, verbatim from `src/search/mod.rs:66-95`:

```rust
#[derive(serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct SearchHit {
    pub name: Option<String>,
    pub dataset: Option<String>,
    pub dataset_fullname: Option<String>,
    pub group: Option<String>,
    pub species: Option<String>,
    pub authors: Option<Vec<String>>,
    pub year: Option<u16>,
    pub description: Option<String>,
    pub mean: Option<f64>,
    pub additive: Option<f64>,
    pub lrs: Option<f64>,
    /// LOD score, LRS / `db::LRS_PER_LOD` (4.604); the route computes
    /// it from `lrs`.
    pub lod: Option<f64>,
    /// Name of the peak marker (a Geno.Name such as "rsm10000012461"),
    /// looked up in PublishXRef for phenotype hits; null for other
    /// hits, or when the database is unavailable.
    pub locus: Option<String>,
    /// The trait's short name (e.g. "5HT_MB_M"), looked up like `locus`.
    pub abbreviation: Option<String>,
    pub geno_chr: Option<String>,
    pub geno_mb: Option<f64>,
    pub inbredsetcode: Option<String>,
    pub pubmed_id: Option<u32>,
    #[serde(flatten)]
    #[schema(value_type = Option<std::collections::BTreeMap<String, String>>)]
    pub extra: std::collections::BTreeMap<String, serde_json::Value>,
}
```

- Native hits come straight from the doc's stored JSON: `hit_from_doc_data` = `serde_json::from_slice::<SearchHit>(data)`; unknown fields land in `extra` (`#[serde(flatten)]`).
- Envelope: `Envelope<T>` (`src/response.rs:55-70`): `{ data: T, meta: Meta, notes: Vec<String>, next: Vec<NextAction>, actions: Vec<SuggestedCall> (skipped when empty) }`. `NextAction { href, description }`.
- Route enrichments (`src/routes/search.rs`): `lod = lrs.map(db::lod)`; `add_publish_annotations` fills `locus`/`abbreviation` from PublishXRef via `db::get_publish_annotations` (DB failure → nulls + `partial` note, not an error).
- `root_links()` (`src/suggest.rs:287-300`) appended as `with_links(crate::suggest::root_links())`: `/groups`, `/species`, `/mapping-methods`, `/openapi.json`.
- Route also adds: `with_note(db::LOD_NOTE)` ("lod = lrs / 4.604"), optional `partial` notes, and next-page `/search?...&page=N` plus `/traits/{dataset}/{name}` top-hit links.

## 5. PAGINATION + LIMITS

Route (`src/routes/search.rs`):
- `limit` default 20, capped: `params.limit.unwrap_or(20).min(100)`. `page` default 1, must be ≥ 1.
- `limit=0` is **not rejected** — engine fetches the full match set anyway (see below) and `hits.truncate(0)` yields an empty `data` array (no zero-division: `more` computed as `hits.len() == limit` would be true for unsorted → a next link with `page=page+1`; sorted path uses `saturating_mul`, safe).
- Huge page: unsorted → `first_item = (page-1)*limit` clamped via `saturating_mul` and `.min(u32::MAX)` in `search_blocking`; xapian returns an empty mset → 200 with `data: []`. Sorted → `skip((page-1)*limit)` on a ≤1000-hit ranked list → empty.
- `sort` must be `lrs`|`lod`; `order` `asc`|`desc`. With `sort`, the engine is asked for `SORT_FETCH_LIMIT = 1000` hits at `page=1`, ranked in-process by `sort_by_lrs` (missing `lrs` last), then sliced. `fetched >= 1000` adds a `partial` note.
- **Engine deliberately ignores the forwarded limit for fetch size**: `FULL_MATCH_SET_MAX_ITEMS = u32::MAX` — the full match set is retrieved every time (per-page invariant, PORT-DESIGN §5; recorded Q02 set = 57 632 hits). `page` only shifts the mset window. Truncation to `limit` happens only in the route (`hits.truncate(limit)`).

## 6. ERRORS — verbatim statuses and messages

Route mapping `engine_error` (`src/routes/search.rs`); `ApiError::new(status, code, title, detail, operation)`:

| Variant | Status | code | title | detail / message |
|---|---|---|---|---|
| Route validation (empty q) | 400 | `invalid_parameter` | `Invalid parameter` | `"q must not be empty"` |
| bad type | 400 | `invalid_parameter` | `Invalid parameter` | `"type must be \"phenotype\" or \"gene\"; see /openapi.json"` |
| bad sort / order / page<1 | 400 | `invalid_parameter` | `Invalid parameter` | `"sort must be \"lrs\" or \"lod\""` / `"order must be \"asc\" or \"desc\""` / `"page must be at least 1"` |
| `InvalidQuery` | 422 | `invalid_query` | `Query rejected by the search backend` | engine's message + typed body under `error` (type `invalid_query`, `param: "query"`, `detail: {xapian_error_type}`) |
| `Unreachable` | 502 | `backend_unreachable` | `Search backend failure` | e.g. `"xapian index at {path} could not be opened: {e}"` |
| `Misbehaving` | 502 | `backend_misbehaving` | `Search backend failure` | `"xapian doc data is not a search hit: {e}"` (doc JSON decode failure) |
| `Backend` | 502 | `backend_error` | `Search backend failure` | (proxy path) |
| `Internal` | 500 | `internal_error` | `Internal error` | e.g. the unset-`XAPIAN_DB_PATH` message in §2, or `"native search task failed to join: {e}"` |

Native-engine 422 messages seen in code:
- Parser: the xapian diagnostic, e.g. `"Syntax: <expression> AND <expression>"` with `xapian_error_type: "QueryParserError"` (unit test, `src/search/native.rs` tests).
- Invalid Hs/Mm location: `"InvalidOperationError: Query is invalid (invalid Hs/Mm location field)"` with `xapian_error_type: "InvalidOperationError"` (the blessed AMENDMENT-E contract replacing gn3's recorded Q42 500).
- Bad numeric range: `"Unknown range operation"` with `xapian_error_type: "QueryParserError"` (Q52).

Parse happens **before** the index opens (gn3's order preserved), so a malformed query reports 422 even with a broken index path.

## 7. TESTS THAT EXIST

- **`tests/search.rs`** (420 lines, proxy path, hermetic gn3 stub + gn-auth stub + locally signed JWT):
  - happy path: `assert_eq!(status, StatusCode::OK); assert_eq!(hits.as_array().unwrap().len(), 5); assert_eq!(hits[0]["name"], "10199"); assert_eq!(hits[0]["dataset"], "C57BL-6JxC57BL-6NJF2Publish"); assert!(hits[0]["lrs"].is_number());` (lines 205-210); envelope note `assert!(body["notes"]...contains(&json!("lod = lrs / 4.604")))` (215); next links `assert_eq!(&next[..2], ["/search?q=alcohol&limit=5&type=phenotype&page=2", "/traits/C57BL-6JxC57BL-6NJF2Publish/10199"])` (231) + root links present (239).
  - validation: `assert_eq!(status, StatusCode::BAD_REQUEST); assert_eq!(body["code"], "invalid_parameter");` (247-249, 253-255); 422: `assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY); assert_eq!(body["code"], "invalid_query");` + detail contains `"Syntax: <expression> AND <expression>"` (260-263); limit cap OK at 9999 (281); `backend_error` 502 (276-277); sort/order validation (281-287).
  - question-style scenario: `assert_eq!(hits.len(), 100, "119 recorded hits, truncated to limit"); assert!(lods.windows(2).all(|w| w[0] >= w[1]), "descending by lod"); assert!((lod - lrs / 4.604).abs() < 1e-9, "lod is lrs / 4.604");` (302-307); BXD serotonin hit: `assert_eq!(serotonin["name"], "17985"); ... assert!((serotonin["lod"]... - 2.6063)...); assert_eq!(serotonin["geno_chr"], "16");` (326-330); `pubmed_id` 24102824, `locus` `rsm10000012461`, `abbreviation` `5HT_MB_M` (344-348); asc sort ordering (360-365); empty result `assert_eq!(body["data"], json!([]))` (372); sorted paging `assert_eq!(page1...len(), 100); assert_eq!(page2...len(), 19); assert_eq!(page3...len(), 0);` (377-379).
- **`tests/search_native_db.rs`** (236 lines, env-gated: needs `XAPIAN_DB_PATH` or `GN4_FULL_INDEX`, plus SQL_URI and a local gn-auth stub; skips otherwise): sets `SEARCH_ENGINE=native`; asserts `assert_eq!(body["meta"]["state"], "complete", ...)` (162), annotated hits `assert!(annotated > 0, "hits must carry locus and abbreviation from PublishXRef")` (189), case-insensitivity `assert_eq!(upper.len(), lower.len(), "dataset:BXDPublish must match the same documents as dataset:bxdpublish")` (193), and set-equality vs DB: `assert_eq!(index_ids, db_ids, "the full index must hold exactly the database's PublishXRef ids for BXD ...")` (217, full index only; dev index merely `assert!(!index_ids.is_disjoint(&db_ids))`, 225).
- **`tests/search_parity.rs`** (2043 lines): replays 54 recorded query shapes through the real `genenetwork4::app()` and grades against a frozen fixture corpus. Clause (a) set equality graded at the engine seam (calls `genenetwork4::search::search` with `ENGINE_FETCH_LIMIT = 1_000_000`, line ~67) because the route truncates; clause (b) top-10 Jaccard ≥ 0.8 (`const JACCARD_FLOOR: f64 = 0.8`). Its own unit tests assert grader math, e.g. `assert!(g.equal); assert_eq!(g.missing, 0); assert_eq!(g.extra, 0);` (729-731), `assert!(grade_error_class(RecordedStatus::Client4xx, 422, "invalid_query", true).is_pass());` (756), `assert!(grade_error_class(RecordedStatus::Server500, 422, "invalid_query", true).is_pass());` (762), `assert_eq!(jaccard(&a, &b), 1.0); assert_eq!(clause_b_pass(10, jaccard(&a, &b)), Some(true));` (791-792).
- Also present (not in scope but relevant): `tests/search_engine.rs` (engine selection), `tests/search_native.rs`, `tests/search_native_shapes.rs`, `tests/search_native_dev_corpus.rs`, `tests/search_native_route.rs`, `tests/search_selfcorrection.rs`.

## 8. WHAT IS COUPLED TO GN4 (lift-out cost)

- **Config struct**: `crate::config::Config::load()` reads `XAPIAN_DB_PATH` and `SEARCH_ENGINE` (`src/config.rs:34-44,280-281,504-505`); the native module calls it inside `search_blocking` and `mod.rs::search` — not parameterized.
- **Shared seam types**: `SearchHit`, `SearchError`, trait `Engine` all live in `src/search/mod.rs`; the route maps `SearchError` → `ApiError` (`src/routes/search.rs::engine_error`) and returns `Envelope<Vec<SearchHit>>`.
- **Auth/middleware**: handler carries `#[gnauth(token_only)]` (macro from `gnauth_macros`), `Ctx`/`RequestInfo`, `AuthToken` (`src/auth.rs`, JWT/JWKS against `AUTH_SERVER_URL`). Envelope, `ApiError`, `with_typed_invalid_query` live in `src/response.rs`/`src/error.rs`.
- **Database enrichment**: `db::lod` (LOD = lrs / 4.604, `db::LRS_PER_LOD`) and `db::get_publish_annotations` (PublishXRef/Phenotype lookup for `locus`/`abbreviation`) — MySQL-side queries the native engine itself does not perform; any lifted engine must reproduce these or live with `partial` responses.
- **Suggest/links**: `crate::suggest::root_links()`, `path_segment` URL escaping.
- **xapian_rs fork dependency**: `xapian_rs` is a fork (`xapian-rs` with `parse_query_checked`, `NumberRangeProcessor` f64 fix 2673115, decline→MatchNothing bridge, custom FieldProcessor with `Box::leak`) — the whole grammar depends on this fork's API surface.
- **Sort logic in the route** (`SORT_FETCH_LIMIT`, `sort_by_lrs`, `add_publish_annotations`) sits outside the engine; a lifted engine either ports the route layer too or keeps the seam `(q, type, limit, page) -> Vec<SearchHit>` and reimplements ranking/paging/envelope.
- **Index provenance**: the xapian index is built by gn3's python indexer (slot numbers verified literal at `index-genenetwork:293-299` per GN4 comments); the native path assumes exactly that index layout (`XT` type terms, slot 0/1/2/3/4/5, lowercase boolean terms, `iamglass` marker).

## Residual risks / notes

- `Database::open` on a corrupt-but-well-formed glass directory would still abort the process (documented residual in `src/search/native.rs` module doc; no test covers it).
- No explicit BM25 parameterization or boosting exists — ordering depends purely on xapian defaults.
- `limit=0` behavior (empty 200 with a next-link) is inferred from code, not pinned by a test I found.
