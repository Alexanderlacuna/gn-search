# gn-search

The GeneNetwork search seam, as a library: **one function**, the query
grammar as data, the hit shape, and an `Index` trait behind it — enough for
GN4 to link it behind its existing `Engine` trait without touching the
route's contract.

Context documents this crate is built from (read them for provenance; they
are the source of every claim in the code comments):

| Document | What it pins |
|---|---|
| `docs/SCOUT-NATIVE.md` | GN4's request → dispatcher → native engine path, the grammar (§3), the hit shape (§4), paging (§5), the error statuses (§6) |
| `docs/SCOUT-PROXY.md` | the gn3 proxy path, the bare-array response shape (§7.2), and why unknown hit keys must survive (§3, §4.2) |
| `docs/SCOUT-INDEXER.md` | the xapian index layout (§2), the sidecar manifest (§3), and the freshness fields a reader wants (§8) |
| `docs/SYNTHESIS.md` | the seam itself (§3), the index plan (§4) and what not to do (§6) |
| `docs/BASELINE.md` | why nothing here was compiled on this machine (§1, §4) |

## What exists

```
src/lib.rs            pub fn search(index_path, q, type_, limit, page) -> Result<Vec<SearchHit>, SearchError>
src/hit.rs            SearchHit: the 18 named fields of SCOUT-NATIVE §4 + `#[serde(flatten)] extra`
src/error.rs          SearchError: InvalidQuery / Unreachable / Misbehaving / Backend / Internal
src/grammar.rs        the query grammar as DATA + a pure renderer (prefix, term, kind) — no xapian
src/index/mod.rs      trait Index { open(path); search(&ParsedQuery, limit, page) } + the offset
src/index/fixture.rs  in-memory index over the 5 recorded alcohol hits (empty path only)
src/index/xapian.rs   STUB behind `--features xapian`: the three pre-checks, then unimplemented!
src/manifest.rs       reads <index>.sidecar.json (v3, and the v4 build_id/built_at shape)
src/bin/gn-search.rs  dev HTTP server behind `--features serve`: /search, /index, /healthz
tests/                grammar render · hit round-trip · error-to-status table · manifest cases ·
                      parity corpus (54 recorded shapes)
docs/DECISIONS.md     one row per choice, with the alternative that was rejected and why
docs/CORPUS-RUNNER.md what the parity runner grades, what it skips, and the command
docs/KNOWN-DEBT.md    the one lint silenced at its site, and why it is not fixed
```

Dependencies: `serde` 1.0.229 and `serde_json` 1.0.151 — the same pins as
GN4's own `Cargo.toml`. Nothing else, in any feature combination.

What is **not** here, on purpose: the response envelope, `lod = lrs / 4.604`,
the PublishXRef `locus`/`abbreviation` enrichment, `sort`/`order`, the
`limit`/`page`/`q`/`type` 400 validation, and the status mapping. All of
that belongs to GN4's route (SCOUT-NATIVE §8, SYNTHESIS §3 and §6); moving
it into this crate would move the contract, and duplicating the validation
would move the 400/422 boundary.

## What is stubbed

**The xapian backend** (`src/index/xapian.rs`, feature `xapian`). It
implements the pre-checks GN4's engine needs before it may touch the
binding — an empty path (`Internal`), a path that is not a directory or has
no `iamglass` marker (`Unreachable`), each with the message GN4 already
shows — plus one check GN4 does not make and this module does: the directory
must also hold a `postlist.glass` table, because `iamglass` alone is the
orphaned shell an interrupted compact or a half-deleted index leaves behind
and `Database::open` aborts the process on it. Then it stops with an
`unimplemented!` that names the missing thing: no Rust xapian binding is
wired in. GN4's native engine uses
`xapian_rs`, a *fork* pinned by git rev (parser `parse_query_checked`,
the `NumberRangeProcessor` f64 fix, the decline→`MatchNothing` bridge, a
custom `FieldProcessor`), so it is not a crates.io dependency this crate can
just take (SCOUT-NATIVE §8). Until that fork is wired in, the default build
uses the fixture index, which is why every test here runs with no index
directory and no xapian at all.

**The HTTP server** (`src/bin/gn-search.rs`, feature `serve`). It exists and
works — `/search` returns a bare JSON array of hits, `/index` returns the
sidecar manifest plus its age, `/healthz` returns `ok` — but it is a
development shell, not the production surface: plain `std::net`, one
thread per connection, no auth, no envelope, no TLS. GN4 owns the real
route. Without `--features serve` the binary prints why and exits 2.

Also not implemented anywhere in this crate: BM25 parameterisation, and the
`tissue:`/`peakchr:`/`rif:`/`wiki:`/`peakmb:` matching in the *fixture*
(those fields are not on `SearchHit`, so the double cannot answer them — a
real index can). `Hs:`/`Mm:` location shorthands ARE implemented (as
species ∧ chromosome ∧ slot-2 range, with no liftover — GN4's own
behaviour); see `docs/DECISIONS.md` rows 13 and 32/33.

## Running the tests

The gate is `cargo test`, offline, on rustup's `1.88.0` toolchain. The `rustc`
on `PATH` is a broken shim (`the 'rustc' binary ... is not applicable to the
'stable-x86_64-unknown-linux-gnu' toolchain`), so the toolchain's `bin`
directory goes first on `PATH`:

```sh
export PATH=~/.rustup/toolchains/1.88.0-x86_64-unknown-linux-gnu/bin:$PATH
# cargo 1.88.0 / rustc 1.88.0

cargo fmt --check
cargo test --offline
cargo test --offline --features serve
cargo test --offline --features xapian
cargo test --offline --all-features
cargo clippy --offline --all-targets   # 0 warnings; see docs/KNOWN-DEBT.md
```

`serde` 1.0.229 and `serde_json` 1.0.151 are in the local cargo registry cache
(they are GN4's pins), so `--offline` resolves with no network access and
nothing vendored; no run needs an index directory, a database or a network.

### Expected results of that gate

MEASURED on the delivered tree with `cargo 1.88.0` / `rustc 1.88.0`, every run
`--offline`. Counts are `passed / failed` per target; every run exits 0 and
`rustc` emits no warnings:

| target | `default` | `--features serve` | `--features xapian` | `--all-features` |
|---|---|---|---|---|
| `unittests src/lib.rs` | 32 / 0 | 32 / 0 | 38 / 0 | 38 / 0 |
| `unittests src/bin/gn-search.rs` | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 |
| `tests/error_status.rs` | 4 / 0 | 4 / 0 | 4 / 0 | 4 / 0 |
| `tests/grammar_render.rs` | 26 / 0 | 26 / 0 | 26 / 0 | 26 / 0 |
| `tests/hit_roundtrip.rs` | 6 / 0 | 6 / 0 | 6 / 0 | 6 / 0 |
| `tests/manifest.rs` | 7 / 0 | 7 / 0 | 7 / 0 | 7 / 0 |
| `tests/parity_corpus.rs` | 9 / 0 | 9 / 0 | 7 / 0 | 7 / 0 |
| doc-tests | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 |
| **total** | **84 / 0** | **84 / 0** | **88 / 0** | **88 / 0** |

Where the feature columns differ from the default, and why:

* `--features xapian` — `src/lib.rs` compiles the xapian backend (a stub whose
  pre-checks are what runs) instead of the fixture. That adds the six
  `src/index/xapian.rs` pre-check tests (`empty_path_is_a_misconfiguration`,
  `missing_path_is_unreachable`, `a_file_is_not_a_directory`,
  `a_directory_without_iamglass_is_not_a_glass_index`,
  `an_iamglass_marker_without_postlist_is_not_an_index`,
  `a_glass_shaped_directory_reaches_the_stub`) to the 32 lib tests.
* `--features xapian` — `tests/parity_corpus.rs` runs 7, not 9: with no fixture
  in the build there is no inventory to restrict the recorded match sets to, so
  the two inventory-dependent metric tests are gated out
  (`#[cfg(not(feature = "xapian"))]`) and `corpus_report` prints
  `SKIP(no-fixture-backend-under-this-feature)` on every recorded-200 shape,
  still grades the three error classes (the grammar decides those before any
  index is opened), and does not apply the `MIN_GRADED` floor
  (`docs/CORPUS-RUNNER.md`, "Under `--features xapian`").
* `--features xapian` is **not** evidence about a real index: the backend is a
  stub, so no xapian code path runs. See the next subsection for what that
  column does and does not mean.

### The default build is the evidence gate; `--features xapian` is not

Say it plainly, because the green numbers run the wrong way: **under
`--features xapian` the corpus run grades 0 match-set comparisons and the
evidence floor does not apply.** Green there means "it builds, and the three
recorded error classes still hold" — nothing about whether a query selects the
right documents. **The evidence gate is the default build.**

Both corpus reports, measured (`cargo test --offline --test parity_corpus --
--nocapture`, then the same command with `--features xapian`), verbatim:

```
# default build — the evidence gate
54 shapes seen: 5 graded a set-equality/Jaccard comparison, 3 graded an error class, 7 VACUOUS (expected set empty: no evidence), 39 SKIP (the missing xapian backend's, see docs/CORPUS-RUNNER.md); 8 PASS, 0 FAIL
exit rule: any FAIL fails the test, and so does fewer than MIN_GRADED = 5 graded comparisons; PASS, VACUOUS and SKIP alone are not evidence

# --features xapian — a compile-and-error-class gate
54 shapes seen: 0 graded a set-equality/Jaccard comparison, 3 graded an error class, 0 VACUOUS (expected set empty: no evidence), 51 SKIP (the missing xapian backend's, see docs/CORPUS-RUNNER.md); 3 PASS, 0 FAIL
no fixture backend under this feature: 51 shapes SKIP(no-fixture-backend-under-this-feature), 0 graded; the evidence floor MIN_GRADED = 5 is not applied to this run
```

So `88 / 0` under the feature is *less* evidence than the default `84 / 0`: the
five set-equality comparisons (two of them two-sided) and the `MIN_GRADED =
5` floor exist only in the default build. `docs/CORPUS-RUNNER.md` explains the
mechanism under "Under `--features xapian`", and names the one class no run
here can grade — phrases — under "What this runner cannot grade and why".

The dev server is a separate step, not part of `cargo test`:

```sh
cargo run --offline --features serve --bin gn-search &
curl -s 'http://127.0.0.1:8080/healthz'                     # ok
curl -s 'http://127.0.0.1:8080/search?query=alcohol&type=phenotype&limit=2&page=1'
# -> the 5 recorded traits, "10199" first
curl -s 'http://127.0.0.1:8080/index'   # 503 index_unavailable with no XAPIAN_DB_PATH
```

What the gate does *not* prove: the xapian binding (absent — the stub is all
there is, so no real index is ever opened) and anything about a populated
index on this machine (there is none: every `iamglass` directory here has no
`postlist.glass`, which is why the pre-check refuses them). Clippy is not part
of `cargo test`, and it is no longer a gap either: `cargo clippy --offline
--all-targets` prints **0 warnings**, and the single lint silenced at its site
(`clippy::type_complexity` on `parse_range`) is argued in
`docs/KNOWN-DEBT.md`.
