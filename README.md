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

## Index builds

The indexer writes a glass database from the GeneNetwork MySQL dump. It reads
three things from the environment and nothing else:

```sh
export SQL_URI='mysql://user:password@host/db_webqtl'          # where the data is
export XAPIAN_DB_PATH=/srv/gn-index/gen-$(date -u +%Y%m%dT%H%M)   # where the index goes
export GN4_RDF_CACHE=/tmp/gn4-rdf-cache.json                   # RIF + wiki dictionaries (optional)
```

A complete build, then refreshes:

```sh
# full: builds into $XAPIAN_DB_PATH, commits in chunks, swaps atomically when done
gn-indexer full --index "$XAPIAN_DB_PATH"

# afterwards, under a lock, in place
gn-indexer delta        --index "$XAPIAN_DB_PATH"   # system-versioning window: only changed datasets
gn-indexer incremental  --index "$XAPIAN_DB_PATH"   # full crawl, per-document hash diff
```

What a build leaves behind, and how to read it back:

```sh
ls "$XAPIAN_DB_PATH"                      # iamglass · postlist.glass · termlist.glass · position.glass · docdata.glass · flintlock
cat "$XAPIAN_DB_PATH".sidecar.json        # mode · doc_count · rif_hash · wiki_hash · per-shard {file, count, content_sha256}
xapian-delve "$XAPIAN_DB_PATH"            # "number of documents = …" — the coarse check
```

Then point an engine at it and ask a question — the same index serves both:

```sh
XAPIAN_DB_PATH="$XAPIAN_DB_PATH" cargo run --features serve --bin gn-search
curl 'http://127.0.0.1:7641/search?query=alcohol&type=phenotype&limit=3'
```

## The schema, briefly

A document's meaning is a set of term prefixes and numeric value slots, defined
once in `crates/indexer/src/schema.rs` and queried by `crates/engine`:

```
prefix   XS species · XG group · XI tissue · XDS dataset · XY symbol · XC chromosome ·
         XPC peak chromosome · A authors · XRF RIF dictionary · XWK wiki
terms    XT<type>  (XTgene, XTphenotype)      Q<doctype>:<name>:<dataset>  (a document's stable identity)
slots    0 mean · 1 peak/LRS · 2 position (Mb) · 3 peak position (Mb) · 4 additive · 5 year · 6 n_samples
```

Writer and reader must agree on that layout, and today nothing enforces it: the
index carries no version or hash of the schema, so a reader can query an index
written by a different layout and get a plausible, wrong answer. Stamping a
schema hash into the sidecar and checking it on open is the fix, and it is not
built yet.

## Licence

AGPL-3.0-or-later, as GeneNetwork. The engine and indexer are derived from
genenetwork4.
