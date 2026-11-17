# gn-search

The GeneNetwork search engine and indexer, extracted from
[genenetwork4](https://git.genenetwork.org/public/genenetwork4) so the search
path can be built, tested and deployed independently.

Status: the engine answers real queries against a real index, and GN4 consumes it
as a library.

## Crates

```
crates/schema/    document shape: hit, error vocabulary, index manifest
crates/engine/    reader: query parser, index open, hit decode   (moved from GN4)
crates/indexer/   writer: MySQL crawl, prefix/slot schema, sidecar manifest, delta
```

The engine is moved, not re-implemented: `crates/engine/src/native.rs` is GN4's
`src/search/native.rs` with three changes — the index path is a parameter rather
than a global, the `Engine` impl stayed in GN4, and one test calls
`search_blocking` directly.

## Use from GN4

```toml
gn-search-engine = { git = "https://github.com/Alexanderlacuna/gn-search", rev = "<sha>", features = ["schema"] }
```

Pinned by revision, as GN4 already pins its xapian bindings. `features =
["schema"]` supplies the `utoipa::ToSchema` derive that GN4's
`Envelope<T: Serialize + ToSchema>` requires.

```rust
use gn_search_engine::{search, SearchError, SearchHit};

impl Engine for CrateEngine {
    async fn search(&self, q: &str, type_: &str, limit: u64, page: u64)
        -> Result<Vec<SearchHit>, SearchError>
    {
        let (query, search_type, path) =
            (q.to_string(), type_.to_string(), self.index_path.clone());
        tokio::task::spawn_blocking(move || search(&path, &query, &search_type, limit, page))
            .await
            .map_err(|e| SearchError::Internal(format!("search task failed: {e}")))?
    }
}
```

## Engine names

GN4 selects a search engine with one variable. There are three values, and an
unrecognised one is refused by name rather than falling through to another
engine.

```
SEARCH_ENGINE=crate      this crate: in-process xapian, opens XAPIAN_DB_PATH
SEARCH_ENGINE=native     the former name for the same thing (the engine used to
                         live in GN4); kept so existing deployments keep working
SEARCH_ENGINE=forward    GN4's own gn3 proxy, an HTTP client — untouched by this
                         crate, and deleted together with gn3
anything else            500 internal_error naming the value it was given:
                         SEARCH_ENGINE="cratee" is not an engine: expected
                         "crate", "native" or "forward"
```

`forward` is the current default in GN4, so nothing changes for a deployment
until it sets a value. The crate itself never speaks HTTP: it only opens a local
index, which is why the proxy is not here.

GN4 retains the route, envelope, `lod`, `locus`, sort/order, links, error
mapping, authentication and OpenAPI. None of that is below the seam.

Verified against GN4 (guix shell): the library builds, four seam/route tests
pass (including the refusal of an unknown engine name), three of GN4's own
engine tests pass against a 47,040-document index, and `GET /search` returns 200
with real hits.

## Build and test

The xapian bindings require clang and a pinned xapian, so build in the
GeneNetwork guix shell:

```sh
guix time-machine -C ~/.config/gn4/channels.scm -- \
  shell -L ~/guix-bioinformatics -D -f ~/project5/genenetwork4/guix.scm -- \
  bash -c 'cd gn-search && cargo test --workspace --features gn-search-engine/fixture'
```

93 tests pass (lib 40, grammar 26, corpus runner 9, manifest 7, hits 6, errors 4).
The corpus runner grades 54 recorded shapes from
`genenetwork4/test/data/parity-fixtures`; anything it cannot grade is reported
`SKIP` with a reason.

Index builds:

```sh
gn-indexer full        --index /srv/gn-index/gen-$(date -u +%Y%m%dT%H%M)   # SQL_URI required
gn-indexer delta       --index /srv/gn-index/current
gn-indexer incremental --index /srv/gn-index/current
```

`full` builds into a sibling directory and swaps atomically; `incremental` and
`delta` update in place under a lock. `<index>.sidecar.json` records the mode,
document count and per-shard hashes.

## Licence

AGPL-3.0-or-later, as GeneNetwork. The engine and indexer are derived from
genenetwork4.
