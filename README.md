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

```toml
gn-search-engine = { path = "../gn-search/crates/engine", features = ["schema"] }
```

```rust
use gn_search_engine::{search, SearchError, SearchHit};

let hits = tokio::task::spawn_blocking(move || {
    search(&index_path, q, type_, limit, page)   // synchronous xapian work
}).await??;
```

GN4 keeps everything a consumer sees — the route, the envelope, `lod`,
`locus`, sort, links, auth and OpenAPI — because all of that lives *above* the
seam. `SEARCH_ENGINE=crate` selects this engine; `native` and `crate` are the
same engine through the same code path, so they cannot diverge.

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

- **Parity.** The recording was made against production; the index we have is a
  47,040-document development build from the 2025-02 dump. Comparing the two
  fails identically for the pre-move engine and this one, so the difference is
  the corpus↔index pairing, not the code. Parity waits on the production index.
- **No schema stamp.** Nothing records *what* a document means, so a writer and
  a reader can disagree invisibly. A hashed schema version, written into the
  index and checked on open, is the fix and is not built.
- **No generation pointer.** Readers open a path, not a versioned generation, so
  a rebuild cannot be rolled back by pointing at the previous one.

## Licence

AGPL-3.0-or-later, as GeneNetwork itself. The engine and indexer are derived
from genenetwork4 and keep its licence and its authors' work.
