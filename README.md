# gn-search

The GeneNetwork search engine and indexer, extracted from
[genenetwork4](https://git.genenetwork.org/public/genenetwork4) so that the
search path can be built, tested and deployed on its own.

**Status: working, not finished.** The engine answers real queries against a
real index, GN4 links it as a library and serves it over its existing route,
and the tests below pass. It is not yet a *parity* claim: that needs the corpus
and the index to come from the same snapshot (see "What is not proven").

## What is here

```
crates/schema/    what a search document is: hit, error vocabulary, index manifest
crates/engine/    the reader: query parser, index open, hit decode  (moved verbatim from GN4)
crates/indexer/   the writer: MySQL crawl, the prefix/slot schema, sidecar manifest, delta
```

The engine was **moved, not re-written**. `crates/engine/src/native.rs` is
GN4's `src/search/native.rs` (1,003 lines) with three forced changes: the index
path is a parameter instead of a global, the `Engine` impl stayed in GN4, and
one test calls `search_blocking` directly.

## How GN4 uses it

GN4 does not import a search engine's internals. It links one crate, implements
one trait it already had, and keeps every byte of its public contract.

```toml
# genenetwork4/Cargo.toml — one line, no MySQL, no index writer, no web server
gn-search-engine = { path = "../gn-search/crates/engine", features = ["schema"] }
```

**Path today, a pinned revision before this lands in GN4.** A path dependency is
right while both repositories are being changed together: it needs no
credentials, no vendoring step, and an edit on one side is visible immediately.
It is wrong for anything reproducible, because it needs a sibling checkout at an
*unknown* commit — GN4's CI would build whatever happens to be on disk.

The three options, honestly:

```
path = "../gn-search/crates/engine"        co-development only; unreproducible, invisible to CI
git = "...", rev = "<sha>"                 reproducible and cheap — but GN4 is PUBLIC and this
                                           repository is PRIVATE, so every build needs credentials
vendor into genenetwork4/vendor/gn-search  reproducible, no credentials, reviewable diffs, refreshed
  at a pinned rev by a script              by a commit — the usual answer for a public repo consuming
                                           a private one
```

My recommendation: use the path while both sides move, then either (a) publish
`schema` and `engine` — they are AGPL, derived from GN4, and a published crate
is the least surprising thing to do with code that is already being distributed
inside GN4 — and pin a `rev`, or (b) vendor at a pinned rev if they stay private.
What we should not do is leave a path dependency in a repository other people
build.

`features = ["schema"]` exists for one reason: GN4 wraps hits in
`Envelope<T: Serialize + ToSchema>`, so `ToSchema` is a *type-formation*
requirement there. The derive lives here, behind a feature, instead of an
orphan impl in GN4 that Rust would refuse.

The whole adapter is 77 lines. This is the part that matters:

```rust
// genenetwork4/src/search/gnindex.rs
use gn_search_engine::{search, SearchError, SearchHit};

impl Engine for CrateEngine {
    async fn search(&self, q: &str, type_: &str, limit: u64, page: u64)
        -> Result<Vec<SearchHit>, SearchError>
    {
        let query = q.to_string();
        let search_type = type_.to_string();
        let index_path = self.index_path.clone();      // XAPIAN_DB_PATH, read per request

        // xapian is synchronous C++ work: keep it off the async runtime's workers
        // (the house pattern, cf. every spawn_blocking handler in GN4's routes).
        tokio::task::spawn_blocking(move || {
            search(&index_path, &query, &search_type, limit, page)
        })
        .await
        .map_err(|e| SearchError::Internal(format!("crate search task failed to join: {e}")))?
    }
}
```

Errors cross the seam by name, one arm per variant, so a rejection stays a
rejection:

```rust
fn translate(error: SearchError) -> SearchError {
    match error {
        SearchError::InvalidQuery { message, xapian_error_type }
            => SearchError::InvalidQuery { message, xapian_error_type },
        SearchError::Unreachable(d)  => SearchError::Unreachable(d),   // index missing  -> 502
        SearchError::Misbehaving(d)  => SearchError::Misbehaving(d),   // bad document   -> 502
        SearchError::Backend(d)      => SearchError::Backend(d),       // backend fault  -> 502
        SearchError::Internal(d)     => SearchError::Internal(d),      // misconfigured  -> 500
    }
}
```

Hits cross as JSON rather than by re-declaring the type, which is what keeps
gn3's `pubmed_link` — a field GN4 has no name for — from being dropped:

```rust
fn to_route_hit(hit: SearchHit) -> Result<SearchHit, SearchError> {
    let value = serde_json::to_value(hit)?;          // `extra` carries unknown keys
    serde_json::from_value(value).map_err(...)       // routes' SearchHit, unchanged shape
}
```

### The switch, and why the two names cannot diverge

```
SEARCH_ENGINE = forward | native | crate
                forward   replay through gn3 (dies with gn3)
                native    43 lines that call the same CrateEngine below
                crate     CrateEngine -> gn-search-engine
```

`src/search/native.rs` was 1,003 lines; it is now 43 and contains no engine at
all — it delegates to `CrateEngine`. So `SEARCH_ENGINE=native` and
`SEARCH_ENGINE=crate` run the same code through the same seam. A rollback is one
environment variable, and a rollback cannot silently test a different engine
from the one you rolled back to.

### What GN4 keeps, and will keep

```
the route            GET /search, param validation, `limit` default 20 and cap 100
the envelope         data · meta · notes · next links
derived values       lod = lrs / 4.604, locus / abbreviation enrichment
sort and order       fetch 1,000, rank, truncate  (SORT_FETCH_LIMIT)
the error contract   422 invalid_query with a typed body · 502 backend_* · 500 internal_error
auth                 gnauth macro + AuthToken extractor; the crate neither checks nor carries a token
OpenAPI              utoipa path + IntoParams + ToSchema, all in GN4
```

None of that moved, because the seam is *below* it. That is the point of the
extraction: the engine can be replaced, scaled or restarted without a single
consumer noticing.

### Verified, not asserted

```
gn4 builds with the crate as a path dependency       BUILD_EXIT=0  (8m23s, guix shell)
gn4's seam tests                                     3 passed
   a_malformed_query_is_the_route_422_class
   an_unset_index_path_is_a_named_misconfiguration
   a_crate_hit_deserialises_into_the_route_hit_without_losing_keys
gn4's own engine tests, on a real index               3 passed  (search_native · native_route · native_shapes)
the HTTP route end to end                             GET /search?q=alcohol&type=phenotype&limit=3
   access log: {"path":"/search","scheme":"jwt","status":200,"took_ms":917,"user":"crate-http-test"}
   "route served 3 hit(s), first AXBXAPublish/10022"
```

## Build and test

The xapian bindings need clang and a pinned xapian, so build inside the
GeneNetwork guix shell:

```sh
guix time-machine -C ~/.config/gn4/channels.scm -- \
  shell -L ~/guix-bioinformatics -D -f ~/project5/genenetwork4/guix.scm -- \
  bash -c 'cd gn-search && cargo test --workspace --features gn-search-engine/fixture'
```

```
93 passed, 0 failed   (lib 40 · grammar 26 · corpus runner 9 · manifest 7 · hits 6 · errors 4 · +1)
```

The corpus runner grades 54 recorded query shapes against
`genenetwork4/test/data/parity-fixtures` (set equality on `(dataset, name)`
pairs, Jaccard ≥ 0.8 on the top ten). It reports what it cannot grade as
`SKIP` with a reason, never as a pass.

Build an index:

```sh
gn-indexer full        --index /srv/gn-index/gen-$(date -u +%Y%m%dT%H%M)   # SQL_URI must be set
gn-indexer delta       --index /srv/gn-index/current
gn-indexer incremental --index /srv/gn-index/current
```

`full` builds into a sibling temp directory and swaps atomically; `incremental`
and `delta` update in place under a lock. The sidecar manifest
(`<index>.sidecar.json`) records the mode, the document count and per-shard
hashes.

## What is not proven

Three gaps — parity across two snapshots, no schema stamp, and an index path
where a generation belongs — are written out in
[`docs/LIMITS.md`](docs/LIMITS.md), each as the failure it causes rather than as
a task. Read that before quoting a result from this engine.

## Licence

AGPL-3.0-or-later, as GeneNetwork itself. The engine and indexer are derived
from genenetwork4 and keep its licence and its authors' work.
