# KNOWN-DEBT — lints and limits this crate accepts on purpose

A clean `cargo clippy --offline --all-targets` run is the bar: it prints no
warnings. Where a warning cannot be removed without changing behaviour, or
without a refactor larger than the lint, the item is recorded *here* and
silenced at the one site with a comment naming this file. So an empty git
`grep` for `KNOWN-DEBT` in `src/` and `tests/` means the clippy run is
clean, and every survivor has a row below.

Toolchain the run was measured on: rustup `1.88.0`
(`cargo-clippy 0.1.88`), `--offline`, in a `timeout 900`.

## 1 · `clippy::type_complexity` — `src/grammar.rs::parse_range`

```sh
cargo clippy --offline --all-targets
# warning: very complex type used. Consider factoring parts into `type` definitions
#   --> src/grammar.rs:861:6
#    | ) -> Result<Option<(Option<f64>, Option<f64>)>, RenderError> { ...
```

**Status: open, allowed at the site** (`#[allow(clippy::type_complexity)]`
above `fn parse_range`, with the reason in a comment there).

Why it is not fixed:

* The type is the contract, in three states that must not be collapsed:
  `Ok(None)` is "this value is not a range at all, so the token is free
  text" (`mean:abc`); `Ok(Some((None, None)))` is "a range with both sides
  open, which matches nothing" (`mean:..`); `Err(RenderError::RangeBound)`
  is Q52's `QueryParserError`. Flattening the nesting to make the lint quiet
  would lose one of the three.
* A `type` alias (`type Bounds = (Option<f64>, Option<f64>);`) silences the
  lint without making the signature easier to read — it renames the nesting
  at every use site and adds a second spelling for the same thing, which is
  the *opposite* of the crate's "one spelling per concept" rule.
* The fix that would genuinely earn the removal is a named struct —
  `struct Bounds { begin: Option<f64>, end: Option<f64> }` — but that type
  travels through `parse_range` *and* `parse_position` *and* the
  `TermKind::ValueRange { slot, begin, end }` construction in `render`. That
  is a refactor of the rendering seam with its own test surface
  (`tests/grammar_render.rs`, 26 tests), not a lint cleanup, and it belongs
  with the backend work that will consume `ValueRange`, not with a warning
  sweep.

Cost while it stands: one `#[allow]` on one private function. The signature
is 63 characters and stable; a reader who trips on it is reading the
canonical statement of "a range has two optional bounds", which is what GN4's
`QueryParser` hands back too (`native.rs:389-405`).

## 2 · Everything else: fixed, not recorded

The other findings clippy reported on this tree were mechanical and are gone in
the same pass — three `map_or(false, …)` → `is_some_and(…)`
(`src/index/fixture.rs`), `manual_unwrap_or_default` (`src/grammar.rs`,
the unparseable-position branch), a collapsible `match` (`src/error.rs`,
`Display`), two `&PathBuf` → `&Path` (`tests/manifest.rs`), one
`uninlined_format_args` (`tests/parity_corpus.rs`), and a duplicated
`#![cfg(feature = "xapian")]` (`src/index/xapian.rs`, redundant with the
`#[cfg]` on its `pub mod` declaration). None of them was a behaviour
question, so none of them has a row here.

Counted the way clippy prints them, that is ten distinct sites (nine fixed, one
allowed above): six in the lib target (three `unnecessary_map_or`,
`collapsible_match`, `manual_unwrap_or_default`, `type_complexity`), three in the
test targets (two `ptr_arg`, one `uninlined_format_args`) and one that only the
`xapian` feature compiles (`duplicated_attributes`). The per-target summary lines
report the lib findings twice (lib and lib-test), which is why a raw count of the
warning lines is larger than the number of places to edit.
