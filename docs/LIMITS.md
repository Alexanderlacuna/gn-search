# Limits — what this engine does not yet prove

What follows is not a TODO list. Each section is a failure mode: what goes
wrong, how it looks from the outside, and what would make it stop.


## 1 · Parity has not been claimed, because the corpus and the index are from different snapshots

A parity claim means *for each of the 54 recorded shapes, this engine returns
what the recording says it returned*. Running that needs two things from the
same moment in time: the recorded answers, and an index those answers could have
come from. We have one of each, and they are not the same moment.

```
the corpus     reduced-v1, 54 shapes, recorded against PRODUCTION genenetwork.org/api3 — 230,031 recorded hits in total
the index      47,040 documents, built 2026-10-06/07 by gn3's indexer from the public 2025-02-18 dump ("~8 months behind the live database")
```

The mismatch is arithmetic, not opinion: shape Q02 (`insulin`, type `gene`)
records **57,632 distinct (dataset, name) pairs** — more documents than the
whole index contains. One query cannot return more documents than exist, so the
recording cannot have come from this index.

What that produces when you grade anyway:

```
39 shapes graded · 15 error-class or nothing-to-grade · 37 disagreed
worst top-10 Jaccard 0.000 (Q02);  the shapes whose documents do overlap score Jaccard 1.000 (Q15, Q19, Q31, Q37)
```

And the decisive control: **the pre-move engine, on the same index, produces the
identical table** — 39 graded, 37 disagreed, same worst shape. A defect introduced
by the move would not reproduce in the code the move came from. So the divergence
measures the corpus↔index pairing, not the extraction.

What would make parity claimable: the production index the recording came from
(the `ask #2` in the project's own notes), or a re-recording of the 54 shapes
against this index — a documented re-baseline event, not a workaround.

```sh
# the run that produced the table above, with a real index and the crate engine
XAPIAN_DB_PATH=/srv/gn-index/current SEARCH_ENGINE=crate   cargo test --offline --test parity_crate -- --nocapture
```

## 2 · Nothing records what a document *means*, so writer and reader can disagree in silence

The index is bytes in a directory. Nothing in it says which prefixes, which value
slots, or which term encoding the documents were written with — so an engine
built today will happily read an index built by a writer from last month and
return *a* result set that no one can check.

That is exactly what tonight looked like from the outside: 37 of 39 shapes
"failed" and neither side was wrong. There was no way to ask the index *what are
you?* and get an answer. The fix is a schema stamp: a version number and a hash
of the prefix/slot table written into the index at build time and compared on
open, so a mismatch is `REFUSED[schema-mismatch]` naming both hashes instead of a
silently different result set.

Not built. `crates/schema` is where it belongs, because both the writer and the
reader already depend on it — that is the one structural advantage of putting
them in one workspace, and it is currently unused.

## 3 · The index is a path, not a generation, so a rebuild cannot be rolled back

Readers open whatever is at `XAPIAN_DB_PATH`. A `full` build writes to a sibling
temp directory and swaps atomically, which protects against a *half-built* index —
but there is no versioned generation behind a pointer, so:

```
· a bad build cannot be rolled back by pointing at the previous one
· two indexes cannot be compared side by side (which is what a re-baseline needs)
· `index_age` is not answerable: the sidecar has a document count and shard hashes, but no built_at,
  so no response can say how old the answers are
```

What it should be:

```
/srv/gn-index/gen-<build_id>/       built by the indexer, complete before it is announced
/srv/gn-index/current -> gen-...    replaced atomically; rollback is pointing it back
manifest inside the generation      schema_hash · build_id · built_at · doc_count · source revision
```

Then readers are stateless and replicable, rollback is one symlink, and
`index_age` becomes a fact in the response rather than folklore.
