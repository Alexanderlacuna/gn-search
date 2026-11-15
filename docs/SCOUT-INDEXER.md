# SCOUT — GN4 Xapian Indexer (read-only, 2026-11-13 snapshot)

Repos: GN4 = `/home/kabui/project5/genenetwork4` (Rust, 154 commits), GN3 = `/tmp/gn3-main` (Python main). No builds, no network.

## 1. WHERE IT LIVES

- Binary: `src/bin/gn-indexer.rs` — the GeneNetwork4 native search-index builder. Module tree: `src/indexer/` (`run.rs`, `build.rs`, `rows.rs`, `schema.rs`, `sidecar.rs`, `delta.rs`, `cache.rs`, `pyjson.rs`, `sha256.rs`, `livemem.rs`).
- Subcommands (`src/bin/gn-indexer.rs:5-9`, dispatch at `:96-100`):
  - `full` — bootstrap; builds a temp index and atomically swaps it in (`run.rs:589`, `run_full_with_reason` `:593`).
  - `incremental` — full crawl, per-document hash diff, in-place write (`run.rs:848`).
  - `delta` — system-versioning window scan, re-diffs only affected datasets (`run.rs:1209`).
- CLI flags (`gn-indexer.rs:40-53`): `--index PATH`, `--rdf-cache PATH`, `--no-publish-filter` (RED/leak-test control only).
- Operational wrappers: `scripts/gn-indexer-run.sh` (cron + flock mutex) and `scripts/gn-indexer-lag.sh` (lag SLA check). Suggested crontab in `gn-indexer-run.sh` lines ~30-32: `*/10 * * * * … delta` and `0 */3 * * * … sweep` (= `incremental`).
- GN3's legacy indexer for comparison: `/tmp/gn3-main/scripts/index-genenetwork` (writes in-place behind an in-process `xapian_lock`, `:185-207`; compaction timing logged at `:513`).

## 2. INPUTS

- SQL URI: `config.sql_uri`, config key `SQL_URI` (`src/config.rs:17-19`, `:230`); layered config, same key gn3 uses. Index path: `--index` else `XAPIAN_DB_PATH` (`gn-indexer.rs:56-58`; `config.rs:34-39`, `:280`, `:450`) — the same key the native search engine reads (`src/search/native.rs:494`).
- RDF caches: `--rdf-cache` else `GN4_RDF_CACHE` else `/tmp/gn4-rdf-cache.json` (`gn-indexer.rs:60-66`); RIF + wiki dictionaries, each one monolithic hashed input (`cache.rs:58-65`).
- Crawl queries (`src/indexer/rows.rs`):
  - Gene (ProbeSet) query `:182-198`: `Species ⋈ InbredSet ⋈ ProbeFreeze ⋈ ProbeSetFreeze(psf) ⋈ ProbeSetXRef(psx) ⋈ ProbeSet LEFT JOIN Geno`.
  - Phenotype (Publish) query `:222-243`: `Species ⋈ InbredSet ⋈ PublishXRef ⋈ Phenotype ⋈ Publication LEFT JOIN Geno`.
  - Both carry `confidentiality < 1 AND public > 0` (commit `cd09579` message; `--no-publish-filter` bypasses, `run.rs:98-100`).
- Prefix mapping (`src/indexer/schema.rs:14-41`), written in `build.rs`:
  - `XS` species (boolean), `XG` group (boolean), `XI` tissue (positioned), `XDS` dataset (positioned), `XY` symbol (boolean), `XC` chromosome (boolean), `XPC` peak chromosome (boolean), plus `A` authors (positioned), `XRF` RIF dictionary, `XWK` wiki dictionary.
  - Value slots (`schema.rs:44-62`): 0 mean, 1 peak/LRS, 2 position-Mb, 3 peak-Mb, 4 additive, 5 year, 6 nsamples.
  - Identity term `Q<doctype>:<name>:<dataset>` lowercased (`schema.rs:69-75`); type term `XTgene` / `XTphenotype` (`schema.rs:65-67`).

## 3. OUTPUT

- Index path: `XAPIAN_DB_PATH` (or `--index`), a glass-backend Xapian directory (marker file `iamglass`; checked at `native.rs:530-536`).
- FULL is atomic: writes to sibling temp dir `.<name>.tmp.<pid>` (`run.rs:133-141`), then `swap_into_place` (`run.rs:145-176`): moves the old index to `.<name>.old.<pid>`, renames temp into place, restores backup on failure. Committed in chunks of `COMMIT_CHUNK_DOCS = 5000` (`run.rs:50`).
- Sidecar (build-id/manifest equivalent): `<index>.sidecar.json` manifest + `<index>.sidecar.d/<label>-<digest16>.json` per-dataset shards (`sidecar.rs:1-30`, `sidecar_path` `:76-83`, `shard_dir` `:86-92`). Manifest fields (`sidecar.rs:47-70`): `version` (3), `mode`, `doc_count`, `rif_hash`, `wiki_hash`, `versioning_marker`, `versioning_flavour`, `shards{label → {file, count, content_sha256}}`.
- Atomicity: shards staged temp + fsync + rename + dir-fsync, **manifest written last** (`run.rs:336-373 finalize`, `sidecar.rs:120-146 atomic_write`, `:150-178 stage_shard`); crash leaves previous self-consistent sidecar; stray temps cleaned by `cleanup_temp` (`sidecar.rs:477-516`). INCREMENTAL/DELTA write the index **in place** (transactional chunks), not by swap; only FULL swaps.

## 4. TIMINGS

- **Not found in the repos.** No recorded full/incremental build durations in GN4 docs, comments, RELEASE_NOTES.md, commit messages, or GN3's `scripts/index-genenetwork`. I have no access to the "October analysis" from this session; nothing quotable about the >15-min complaint exists in the code.
- Closest evidence: `scripts/gn-indexer-run.sh` sets `GN4_INDEXER_WORST_CASE_MINUTES` default **99** (line ~22) as the assumed worst-case hold of the flock — implying a full build was budgeted up to ~1.6 h. `scripts/gn-indexer-lag.sh` sets the lag SLA default **120 minutes** = "worst-case 99 + tick 10 + slack 10". GN3's compaction logs minutes at `scripts/index-genenetwork:513`.
- Commit `96c1979` records *LOCO mapping* timings, not index builds.

## 5. INCREMENTALITY

- DELTA (`src/indexer/delta.rs`): seven system-versioned tables `VERSIONED_TABLES = [PublishXRef, PublishFreeze, ProbeSetXRef, ProbeSetFreeze, ProbeSet, InbredSet, Species]` (`delta.rs:38-46`), probed via `ROW_START` (`versioning_probe` `:507`).
- Watermark/state: `versioning_marker` (DB `NOW(6)`, `timestamp6` flavour, `delta.rs:26`) persisted in the sidecar manifest; advanced each delta run (`run.rs:1241-1283` window from previous marker minus 30 s overlap, `overlap_seconds` `delta.rs:219`, tested `=30` at `:955`). New marker captured **before** candidate queries (`run.rs:1244-1246`).
- Change fan-out: `delta::scan` (`delta.rs:584`) names candidate rows per table → affected dataset labels; a scoped crawl re-diffs only those datasets (`run.rs:1355-1363`). Any doubt (missing/old sidecar, doc-count or shard mismatch, versioning absent, NULL/future/non-monotonic marker, purged history, case-collision labels, unknown flavour) falls back to FULL with a logged reason (`preflight` `run.rs:790-844`; `decide` `delta.rs:251`).
- Blind spots (documented `delta.rs:7-12`): cache text and the unversioned tables (`Geno`, `Phenotype`, `Publication`, `ProbeFreeze`, `Tissue`); the 3-hourly `incremental` sweep (full re-hash) is the correctness bound.

## 6. STALENESS (Pjotr's complaint)

- **Not found.** Nothing in the query path reports index age or build id. `search_blocking` opens the DB and returns hits only (`native.rs:446-519`); `src/routes/search.rs` exposes no sidecar/marker/age field (grep for `sidecar|index_age|marker` in routes: no hits). The only freshness machinery is operator-side (`scripts/gn-indexer-lag.sh` reads `versioning_marker` from the sidecar and alerts past a 120-min SLA) — it never reaches a query response.

## 7. OPERATIONAL RISK (cron building while the service serves)

- SINGLE-INSTANCE IS ENFORCED: `gn-indexer-run.sh` takes an exclusive flock on `<index>.lock` (fd 9, `:90-108`) — two indexers cannot collide. But the service is not coordinated with that lock:
  - FULL swap window: `swap_into_place` (`run.rs:145-176`) first renames the live index **away** (`:154-157`) before renaming the temp in (`:158-166`). Between the two renames the path does not exist; a search request landing there fails at `open_database` (`native.rs:520-540`, metadata/is-dir/iamglass checks → `SearchError::Unreachable`). Mitigated only by readers opening per request (`native.rs:494-503`), so the bad window is the gap between two renames — small but real, and a reader that resolved the path just before the move may open stale files harmlessly.
  - INCREMENTAL/DELTA write in place with committed chunks; xapian readers can tolerate a concurrent writer, but a reader that opens mid-commit between chunks sees a partially applied delta until the next chunk commit — bounded staleness, not corruption.
  - The service has no notion of the lock, the sidecar, or the marker; `XAPIAN_DB_PATH` is read per request (`native.rs:494`), which is what keeps the swap survivable at all.

## 8. WHAT AN ATOMIC SWAP NEEDS

The FULL path already has the mechanics (`temp_index_path`/`swap_into_place`, `run.rs:133-176`). A manifest readable by the **service** would need, at minimum:
- `build_id` (unique per run, e.g. ULID) — not present today;
- `built_at` (UTC timestamp of swap completion) — not present (the existing `versioning_marker` is the DB write-coverage point, not wall-clock build time);
- `source_revision` (git commit of the indexer, e.g. from `env!("CARGO_PKG_…")`/build script) — not present;
- `record_count` — already exists as `doc_count` (`sidecar.rs:54-56`).
- Read point for the service: the natural place is alongside `open_database` (`native.rs:520-540`) — parse `<index>.sidecar.json` next to `XAPIAN_DB_PATH` and expose it in the search envelope/meta (route in `src/routes/search.rs:171`). Since the reader opens per request, it would pick up a new manifest automatically after each swap, giving Pjotr's freshness reporting for free once fields exist.

## Start Here
`src/indexer/run.rs` (orchestration + swap + sidecar finalize), then `src/bin/gn-indexer.rs` and `src/indexer/sidecar.rs`.

## Residual risks
- No measured build timings anywhere in either repo; the 99/120-minute figures are budget constants, not measurements.
- Incremental/delta write in place — the atomic swap guarantee only covers FULL.
- `versioning_flavour`/prefix test seams (`GN4_INDEXER_DELTA_TABLE_PREFIX` + `GN4_INDEXER_DELTA_TEST`) must never leak into production env.

```acceptance-report
{
  "criteriaSatisfied": [
    {
      "id": "criterion-1",
      "status": "satisfied",
      "evidence": "SCOUT-INDEXER.md written with file:line citations for all 8 sections; every 'not found' (timings, staleness reporting, build_id/built_at) explicitly stated rather than guessed"
    }
  ],
  "changedFiles": ["docs/SCOUT-INDEXER.md (new, own output path only)"],
  "testsAddedOrUpdated": [],
  "commandsRun": [
    {"command": "find/grep/read over genenetwork4 and /tmp/gn3-main", "result": "passed", "summary": "located indexer bin, module tree, schema, sidecar, delta, cron wrappers, search read path"},
    {"command": "git log --all -- src/indexer (genenetwork4)", "result": "passed", "summary": "no recorded build timings in history; confirmed indexer commit range"}
  ],
  "validationOutput": ["All sections carry file:line; 'not found' used for timings and staleness reporting"],
  "residualRisks": [
    "No measured index-build durations exist in either repo; 99/120-min constants are budget assumptions, not measurements",
    "October analysis referenced by the task was not available to this session"
  ],
  "noStagedFiles": true,
  "diffSummary": "Single new markdown scout report; no source files touched (read-only task)",
  "reviewFindings": ["no blockers"],
  "manualNotes": "FULL already swaps atomically (temp + backup-rename); incremental/delta write in place. Search engine opens the DB per request, which is what makes the swap survivable today."
}
```
