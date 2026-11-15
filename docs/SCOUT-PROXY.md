# SCOUT-PROXY — GN4 `/search` proxy path (gn3.rs) and its GN3 endpoint

Read-only scouting. All paths absolute; line numbers from the worktrees as checked out.

---

## 1. THE CALL (GN4 → GN3)

File: `/home/kabui/project5/genenetwork4/src/search/gn3.rs`

- **URL**: built from `Config::load().gn3_url` (env/config key `GN3_URL`, default `http://127.0.0.1:5003/api/`, `src/config.rs:259-264`, normalized to a trailing `/` by `normalize_base_url`, `src/config.rs:713-717`). The forwarder appends `search/` and query params (`gn3.rs:110-125`):

```rust
let gn3_url = crate::config::Config::load().gn3_url;
let url = url::Url::parse_with_params(
    &format!("{}search/", gn3_url),
    [
        ("query", q),
        ("type", type_),
        ("per_page", &limit.to_string()),
        ("page", &page.to_string()),
    ],
)
```

  So the effective endpoint is `GET {GN3_URL}search/?query=…&type=…&per_page=…&page=…` — i.e. `http://127.0.0.1:5003/api/search/`. Note GN4's `limit` is sent as `per_page`.
- **Method**: GET via a shared static reqwest client (`gn3_client()`, `gn3.rs:76-88`), one `OnceLock` reqwest::Client with keepalive pooling.
- **Headers**: none set by the proxy — **no `Authorization` is forwarded to gn3** (contrast the gn-auth path, `src/auth.rs:535,651` which sends `Authorization: Bearer …` to gn-auth). Only reqwest defaults.
- **Timeout**: total client timeout from config key `GN3_TIMEOUT_SECS`, default `30` seconds (`src/config.rs:264,501-503`):

```rust
let timeout = Duration::from_secs(crate::config::Config::load().gn3_timeout_secs);
reqwest::Client::builder().timeout(timeout).build().expect("gn3 client builds")
```

  Comment at `gn3.rs:74-75`: "gn3 sets none of its own, so without this a stalled upstream hangs the handler past any front-proxy timeout."
- **Retry policy**: none. A single `send().await`; a transport error maps straight to `SearchError::Unreachable` (`gn3.rs:128-131`). No grep hits for retry anywhere under `src/search/` or `src/routes/search.rs`.

## 2. THE GN3 SIDE

File: `/tmp/gn3-main/gn3/api/search.py`

- **Route**: blueprint `search` registered with `url_prefix="/api/search"` at `/tmp/gn3-main/gn3/app.py:113` (`app.register_blueprint(search, url_prefix="/api/search")`); the handler is `@search.route("/")` → `search_results()` (`search.py:206-207, 253`). There is no `/api3/search` — not found. So the served URL is exactly `/api/search/` (trailing slash matters: Flask's blueprint root).
- **Params** (`search.py:254-263`):
  - `query` (default `""`)
  - `type` (default `"gene"` — note: GN4's route defaults to `"phenotype"` and always forwards an explicit `type`, so the GN3 default is dead code on the proxy path)
  - `page` (default 1, `type=int`; `page < 1` → `abort(404, description="Requested page does not exist")`)
  - `per_page` (default 100, `type=int`; `per_page > 50000` → `abort(400, description="Requested too many search results")`)
- **Success body**: `jsonify(traits)` — a bare JSON array of trait dicts, each `json.loads(document.get_data())` plus, for `type == "phenotype"`, a computed `pubmed_link` field (`search.py:280-300`). Not enveloped.
- **Error body** (parser rejection, `search.py:267-269`):

```python
except xapian.QueryParserError as err:
    return jsonify({"error_type": str(err.get_type()), "error": err.get_msg()}), 400
```

  `err.get_type()` is the xapian exception class *name as a Python bytes repr string* in production — GN4's fixtures carry `"error_type": "b'QueryParserError'"` (documented at `gn3.rs:7-15`).
- **DB**: `xapian_database(current_app.config["XAPIAN_DB_PATH"])` (`search.py:272`; `gn3/db_utils.py:96`).
- **Ranking**: plain `enquire.set_query(OP_FILTER(query, XT{type}))` — relevance-ordered mset, offset `(page-1)*per_page` (`search.py:273-279`).

## 3. TRANSLATION (GN3 JSON → GN4 envelope)

Two layers:

**Engine layer** (`gn3.rs:166-169`): the body is parsed straight into GN4's `SearchHit` list:

```rust
serde_json::from_str::<Vec<SearchHit>>(&body)
    .map_err(|e| SearchError::Misbehaving(e.to_string()))
```

`SearchHit` (`src/search/mod.rs:120-153`) names 19 fields (name, dataset, dataset_fullname, group, species, authors, year, description, mean, additive, lrs, lod, locus, abbreviation, geno_chr, geno_mb, inbredsetcode, pubmed_id) all `Option<…>`, plus `#[serde(flatten)] extra: BTreeMap<String, Value>` (`mod.rs:150-152`) that catches every other key — a passthrough, not a field-by-field mapping. Because every named field is `Option`, gn3's explicit `null`s deserialize to `None` and then re-serialize as `null` (serde does not skip by default), while gn3-*absent* fields are also `null` in GN4's output — the proxy collapses "absent" and "null" into `null` for the named fields, but passes unknown keys through verbatim inside `extra` (absent keys stay absent there).

**Route layer** (`src/routes/search.rs`):
- `lod` computed from `lrs`: `hit.lod = hit.lrs.map(db::lod);` (`search.rs:247-249`).
- `locus`/`abbreviation` filled from a PublishXRef DB lookup for `*Publish` dataset hits (`add_publish_annotations`, `search.rs:206-232`); on DB failure they are null and the envelope is marked `partial` (`search.rs:262-266`).
- `sort`/`order` handled entirely GN4-side: with `sort`, the engine is asked for `SORT_FETCH_LIMIT = 1000` hits at `page=1` (`search.rs:196,258-263`), sorted by `lrs` (`sort_by_lrs`, `search.rs:183-193`), paged and truncated locally.
- Envelope: `ctx.envelope(hits).with_note(db::LOD_NOTE)`, plus optional `partial` notes and `next` links (`search.rs:260-289`). So the final 200 body is `{data: [...], notes…, links…}` — a shape GN3 never emits.

## 4. DIVERGENCE (proxy vs native, and parity coverage)

Divergences observable today:

1. **Envelope**: proxy/native both return `Envelope<Vec<SearchHit>>` (route is shared, `search.rs:226`), but the *hit contents* differ by engine selection (`SEARCH_ENGINE`, default `"forward"`, `src/config.rs:281`; dispatch `mod.rs:95-99`). Native fills computed fields (`mean`, `additive`, `lrs`, `geno_chr`/`geno_mb`) from its own enrichment path — the module doc (`mod.rs:70-80`) explicitly notes these are NOT in the xapian index and gn3 computes them at query time; any drift in those enrichment queries is proxy/native divergence.
2. **`pubmed_link`**: gn3 adds it for phenotype hits (`search.py:294-299`); it is not a named `SearchHit` field, so on the proxy it survives only via `extra`. Whether native reproduces it is not asserted anywhere in the parity harness.
3. **Error mapping rewrites gn3's statuses**: gn3's designed 400 `{"error_type","error"}` becomes GN4 422 `invalid_query` with a typed body (`gn3.rs:133-146` + route `search.rs:110-133`); gn3's escaping-xapian-exception 500s (e.g. `Hs:foo:1M` → `InvalidOperationError`, `position:abc` → `ValueError`) are re-classified to 422 too (`gn3.rs:39-73`, "AMENDMENT E"), while prose 500s stay 502 `backend_error`. gn3's 404 (bad page) and 400 (per_page > 50000) both collapse into "any 4xx with an `error` field → 422" or, without that field, 502 — a deliberate divergence from gn3's own contract.
4. **Paging semantics**: GN4 caps `limit` at 100 and truncates (`search.rs:243,253`), while gn3 allows `per_page` up to 50000. Sorted mode fetches 1000 and pages locally — no proxy/native difference, but a divergence from raw gn3.
5. **error_type normalization**: GN4 strips the `b'…'` Python repr wrapper (`normalize_error_type`, `gn3.rs:7-22`); gn3 emits the repr verbatim. A consumer comparing raw upstream JSON to GN4's 422 body sees different `error_type` strings.
6. **Empty q / whitespace q**: GN4 rejects with 400 `invalid_parameter` before any upstream call (shapes Q48/Q49, parity `Surface400`); gn3 would happily parse `""`.

**`tests/search_parity.rs` evidence** (2,044 lines):

- Asserts (engine seam, below truncate): full-set equality of `(dataset, name)` identifier pairs against frozen recorded fixtures — `comparator::grade_match` (`search_parity.rs:459-470`), driven at `ENGINE_FETCH_LIMIT = 1_000_000` (`search_parity.rs:70-77`).
- Asserts (route level): clause (b) — top-10 hits in recorded order, graded as **Jaccard ≥ 0.8** (`JACCARD_FLOOR`, `search_parity.rs:66-69`; `clause_b_pass`, `search_parity.rs:531-540`) — i.e. *set* similarity of the first ten, NOT order and NOT any field beyond `dataset`/`name`.
- Asserts error classes: recorded 4xx → 422 `invalid_query`; recorded 500 (Q42) → 422 `invalid_query` (AMENDMENT E re-baseline); surface (Q48/Q49) → 400 `invalid_parameter` with **0 upstream calls** (`grade_error_class`, `search_parity.rs:489-522`).
- Asserts exactly 2 upstream stub calls per match shape (route + engine replay) and loud failure on unmatched forwarded params (`search_parity.rs:1900-1910`, stub no-match path `search_parity.rs:1136-1146`).

**What it does NOT cover**:

- No field-level comparison of hit bodies — only `(dataset, name)` pairs (`hit_key`, `search_parity.rs:420-431`). `pubmed_link`, `lrs`, `mean`, `additive`, `geno_*`, extra keys: unchecked.
- No ordering assertions beyond which ten hits are in the top-10 set (order within the ten is a set).
- `per_page`/`page` forwarded values are logged in unmatched diffs but never asserted for match shapes (`per_page` is read, `search_parity.rs:1121`, only to format the drift message; the stub ignores it).
- No timeout / slow-upstream / empty-body / truncated-JSON failure cases; no test that the 30s timeout fires or that Misbehaving/Unreachable are produced by real socket conditions (the parity stub always answers).
- The engine-seam clause (a) is graded only against the *recorded gn3* corpus — it can never detect divergence between a fixed proxy and gn3 today; it detects native-vs-recording.

## 5. AUTH

- The `/search` route handler is `#[gnauth(token_only)]` (`src/routes/search.rs:227`) and takes `_token: AuthToken` — the caller's Bearer JWT is validated against gn-auth's JWKS (fetched from `AUTH_SERVER_URL`, e.g. `src/auth.rs:535` sends `Authorization: Bearer {token.raw}` to gn-auth).
- **The token does NOT travel to gn3.** The proxy request carries no Authorization header (§1). An extracted service sitting where `Gn3` sits would therefore receive an unauthenticated GET with only `(query, type, per_page, page)`; nothing about the caller's identity, scopes, or the JWT needs reproducing on this hop. (If the extraction instead replaces the *route*, it must reproduce `#[gnauth(token_only)]`: RS256 JWT validation against gn-auth's `/auth/public-jwks`.)

## 6. FAILURE MODES → GN4 responses (verbatim mapping)

Route mapping table `src/search/mod.rs:62-68`; implementation `src/routes/search.rs:88-135`.

| GN3 condition | Engine variant | GN4 response |
|---|---|---|
| Connection refused / DNS fail / transport error (`gn3.rs:128-131`) | `Unreachable(e)` | 502 `backend_unreachable`, title "Search backend failure", detail = reqwest error |
| Timeout (30 s, no response) | `Unreachable` (reqwest timeout error) | 502 `backend_unreachable` |
| Body read fails | `Misbehaving(e)` (`gn3.rs:132-135`) | 502 `backend_misbehaving` |
| 4xx with JSON `error` field | `InvalidQuery{message, xapian_error_type}` (`gn3.rs:137-146`) | 422 `invalid_query`, title "Query rejected by the search backend", typed body under `error` with `param:"query"` and `detail:{"xapian_error_type": …}` |
| 4xx without `error` field (e.g. gn3's per_page>50000 400 has only `description` via Flask abort) | falls through | 502 `backend_error`, detail `"gn3 search returned HTTP 400 Bad Request: …"` (`gn3.rs:160-164`) |
| 5xx whose `error` field looks like an exception class (`looks_like_xapian_exception_class`, `gn3.rs:39-57`) | `InvalidQuery` | 422 `invalid_query` (AMENDMENT E) |
| 5xx with prose body | `Backend` | 502 `backend_error` |
| 200 but body not a JSON array of hits | `Misbehaving` (`gn3.rs:166-169`) | 502 `backend_misbehaving` |
| **200 with empty array `[]`** | Ok | 200, `data: []` in the envelope — *not* an error |
| `GN3_URL` unparsable | `Internal` (`gn3.rs:118-123`) | 500 `internal_error` |

Empty-hit note: parity `match200` shapes with zero hits (Q18, Q24, Q25, Q38, Q39, Q50) are graded as successful 200s.

## 7. WHAT AN EXTRACTED SERVICE MUST REPRODUCE (byte-identical on this path)

1. **Request contract**: GET `{base}search/` with exactly `query`, `type`, `per_page`, `page` as query params, URL-encoded via `url::Url::parse_with_params` form-encoding. `per_page` = the *engine-level* limit (route's `limit` capped at 100; sorted mode sends 1000 at page 1).
2. **Response**: bare JSON array (no envelope) of trait objects, relevance-ranked xapian order, offset `(page-1)*per_page`, phenotype hits augmented with `pubmed_link` built exactly as `search.py:294-299` (`http://www.ncbi.nlm.nih.gov/entrez/query.fcgi?cmd=Retrieve&db=PubMed&list_uids=…&dopt=Abstract`).
3. **Field fidelity**: all documented gn3 trait keys, including ones `SearchHit` only carries in `extra` — an extractor must preserve unknown keys, exact key names, `null` vs absent distinction per key (GN4 collapses it for named fields but `extra` preserves it upstream), number formats (f64 serialization, e.g. `lrs`/`mean` float precision), and array ordering.
4. **Error contract**: 400 with body `{"error_type": "<class>", "error": "<msg>"}` where `error_type` is the *bytes-repr* form (`b'QueryParserError'`) — GN4 normalizes it downstream; emitting the bare class would also work but would diverge from recorded fixtures. The escaping-exception 500 bodies (Q42 `InvalidOperationError`) must keep the `{"error": "<ClassName>…"}` shape so GN4's `looks_like_xapian_exception_class` (`gn3.rs:39-57`) classifies them: first word > 5 chars, ends in `Error`, capitalized, alphanumeric.
5. **Status codes**: 400 for parser rejects, 404 for `page < 1`, 400 for `per_page > 50000` (with only a `description`, no `error` field → GN4 502), 500 otherwise.
6. **Query semantics**: the full xapian queryparser config of `parse_query` (`search.py:213-251`): stemmer en / STEM_ALL_Z, boolean prefixes author/species/group/tissue/dataset/symbol/chr/peakchr/position(+pos alias), value prefixes description/rif/wiki/RIF/WIKI, range prefixes mean/peak/position/peakmb/additive/year with SI suffixes (k/M/G), the ±50kb point-position expansion, `Hs:`/`Mm:` location shorthands with the hg19ToMm10 chain-file liftover, and the `XT{type}` filter.
7. **Ranking**: xapian relevance order (no re-ranking); GN4 sorts client-side only when `sort` is passed, so raw relevance order is observable in the top-10.
8. **What it does NOT need**: any auth (no token is forwarded), no envelope, no `lod`/`locus`/`abbreviation`/`sort`/`order` (all GN4-side), no CORS/front-proxy behavior.

## Start Here

`/home/kabui/project5/genenetwork4/src/search/gn3.rs` (170 lines, whole file) — the entire proxy engine, including all error-mapping rules. Then `src/routes/search.rs` for the envelope, and `/tmp/gn3-main/gn3/api/search.py:253-300` for the served endpoint.

## Residual risks / open questions

- `/api3/search` — not found anywhere in `/tmp/gn3-main`; only `/api/search` exists (`app.py:113`).
- GN3's default `type="gene"` vs GN4's default `"phenotype"`: harmless today (GN4 always sends `type`), but an extracted service must not "fix" it to match GN4's default or untyped direct callers break.
- The 30 s timeout default is config, not code-pinned; a deployment with a different `GN3_TIMEOUT_SECS` changes failure timing, not contract.
- Parity fixtures pin `(dataset, name)` identity only; byte-level field fidelity of the proxy path is currently attested by nothing automated.
