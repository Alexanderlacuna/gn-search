# REVIEW-FIXES — adversarial review of the F1–F7 fix pass

Reviewer: independent, non-GLM lane. Artifact under review was authored by a GLM-family
worker. Scope: the fix pass (`src/grammar.rs`, `src/index/fixture.rs`, `src/index/mod.rs`,
`src/hit.rs`, `src/lib.rs`, `src/bin/gn-search.rs`, `tests/parity_corpus.rs`,
`docs/CORPUS-RUNNER.md`, `docs/DECISIONS.md` rows 9/13/19/30–33) against
`docs/REVIEW-CRATE.md` and the ground truth in `project5/genenetwork4`
(`src/search/native.rs`, `src/routes/search.rs`, `tests/search_parity.rs`,
`test/data/parity-fixtures`). Static review only: `BASELINE.md` §1, no compiler, no index,
no cargo.

**Verdict: PASS-WITH-FIXES.** Six of seven findings are genuinely repaired and I could not
turn any of them back into a defect by reading; the seventh (F6) is repaired in the
direction the finding asked and *over*-repaired in a way that turns a valid xapian query
into a 422. The runner is honest — a SKIP is never printed as a PASS — but it grades far
less than its own summary line suggests, and two of its guarantees are documentary rather
than tested.

---

## a) Per finding

| # | Verdict | Proof line | Note |
|---|---|---|---|
| F1 | **FIXED** | `src/index/fixture.rs:96` `if !path.is_empty() {` → `:97-101` `return Err(SearchError::Internal(…"no search backend is compiled into this build…"))` | Refusal happens before the doc set is built; `""` still opens (tests at `:376`). The fabricated-answer failure mode is gone. Residual: `open_index` still selects by feature, and the dev server maps this to 500 where its doc promises 503 (`bin:22-24` vs `bin:274-281`). |
| F2 | **FIXED** | `src/grammar.rs:708-710` vs GN4 `native.rs:360-363`, `:402`, `:723-724` (quoted below) | The `..` gate now matches GN4's guard exactly, and the no-`..` case is not an error. Residual: the fallback token keeps `field:` (see c). |
| F3 | **FIXED** | `src/grammar.rs:520` `.filter(\|(_, value)\| !value.is_empty())` | Empty value never reaches a processor, so `position:` is text — the Q53 200 class. Same residual as F2 (colon retained). |
| F4 | **FIXED** | `src/grammar.rs:244-253` + `:613-660` + `:401-403` vs GN4 `native.rs:156-169`, `:317-344`, `:296` (quoted below) | Species term, chromosome term and slot-2 range are the same three terms, in the same order, from the same lowercased split; message and exception class are GN4's strings byte-for-byte. Detection point differs (render-time vs engine-time) with no observable difference at the seam. |
| F5 | **FIXED** | `src/index/mod.rs:41-47` + `src/index/fixture.rs:114-124` `Ok(matching.into_iter().skip(to_usize(offset)).collect())` | The full-match-set contract is stated *and* implemented; `limit` only places the offset; the dev server truncates (`bin:189`). Not made structural (`search_all`) — a future backend can still silently page, and no generic test would catch it. |
| F6 | **PARTIALLY** | `src/grammar.rs:679` `Some(previous) => is_operator(previous) && !exempt(previous),` | Q51 is reproduced correctly (`:406` message, `:415` class). But `a AND NOT b` — the canonical xapian exclusion — is rejected as a dangling operator, because the rule exempts `NOT` only in leading position and after itself, not after a binary operator. A false 422, i.e. F6 swapped one error-class drift for its mirror. |
| F7 | **FIXED** | `src/hit.rs:81` `pub extra: BTreeMap<String, serde_json::Value>,` | GN4's own type (`search/mod.rs:150-152`); feature unification can no longer reorder `extra`. Pinned by `tests/hit_roundtrip.rs:125`. |

### F2 — GN4 `native.rs` against the crate line

Crate (`src/grammar.rs:544-560`, then `:708-710`):

```rust
if let Some(range) = self.numeric_range(field) {
    // … `mean:5` is a `mean 5` phrase and never a range …
    if let Some((begin, end)) = parse_range(field, value)? { … continue; }
}
```
```rust
let Some((begin, end)) = value.split_once("..") else {
    return Ok(None);
};
```

GN4 (`src/search/native.rs:360-363` and `:402`):

```rust
/// The scan is deliberately conservative: it only looks at unquoted
/// `<prefix>:` token runs that contain `..` (xapian consults range
/// processors only for range syntax; a quoted range is phrase content
/// and is skipped, matching the parser).
```
```rust
if let Some((begin, end)) = value.split_once("..") {
```

GN4 (`src/search/native.rs:723-724`):

```rust
let mean5 = description(&mut qp, "mean:5");
assert!(!mean5.contains("VALUE"), "mean:5 is text, got {mean5:?}");
```

Verdict **FIXED**: the crate's gate is now GN4's gate. One asymmetry the fix did *not*
copy: GN4's scan is **quote-aware** (`:380-385` toggles `in_quotes`, and `:398-401` stops
the value at `"`), while the crate has no notion of quoted ranges — `mean:"1..2"` renders
`sides = ["\"1", "2\""]`, fails `f64::parse` and produces the Q52 422 that GN4 cannot
produce for phrase content. No recorded shape uses it; it is a real false-422 class
regardless.

### F4 — GN4 `native.rs` against the crate line

Crate (`src/grammar.rs:244-253`):

```rust
location_shorthands: &[
    LocationShorthand { field: "Hs", species_term: "XShuman" },
    LocationShorthand { field: "Mm", species_term: "XSmouse" },
],
```

Crate renderer (`src/grammar.rs:621-634`):

```rust
let value = value.to_lowercase();
let mut parts = value.split(':');
let (chromosome, position_spec) = match (parts.next(), parts.next(), parts.next()) {
    (Some(chromosome), Some(position_spec), None) => (chromosome, position_spec),
    _ => return Err(invalid()),
};
let chrom_number = chromosome.strip_prefix("chr").ok_or_else(invalid)?;
```

Crate message/class (`src/grammar.rs:402`, `:416`):
`"InvalidOperationError: Query is invalid (invalid Hs/Mm location field)"` /
`Self::InvalidLocation { .. } => "InvalidOperationError"`.

GN4 registration (`src/search/native.rs:156-169`):

```rust
qp.add_custom_boolean_prefix::<_, &str>(
    "Hs",
    LocationProcessor { species_term: "XShuman" },
    None,
);
```

GN4 processor (`src/search/native.rs:317-344`):

```rust
let value = term.to_lowercase();
let mut parts = value.split(':');
let (chromosome, position_spec) = match (parts.next(), parts.next(), parts.next()) {
    (Some(chromosome), Some(position_spec), None) => (chromosome, position_spec),
    _ => return Some(invalid_location_leaf()),
};
let Some(chrom_number) = chromosome.strip_prefix("chr") else {
    return Some(invalid_location_leaf());
};
let Some((begin, end)) = parse_position(position_spec) else { … };
Some(Query::combine(
    Operator::And,
    Query::term(self.species_term, None, None),
    Query::combine(
        Operator::And,
        Query::term(format!("XC{chrom_number}"), None, None),
        position_value_query(begin, end),
    ),
))
```

Verdict **FIXED**: identical split rule, identical `chr` requirement, identical
lowercase-before-split, identical three-term AND, identical `XC{chrom_number}` spelling,
identical 422 message and class. The only difference is *where* the invalid case is
detected — GN4 emits `INVALID_LOCATION_SENTINEL` (`:296`) and the engine converts it to
`InvalidQuery` before the mset call (`:472-483`); the crate errors during `render` and
`lib.rs:95-101` converts it. Same variant, same message, same class ⇒ no observable drift
at the seam. Liftover remains out of both, as DECISIONS row 13 now says.

---

## b) The runner

### (i) Can a SKIP ever count as a PASS?

**No — but a run of nothing but SKIPs is still green.** The decision is a three-variant
enum and only one of them is counted as a pass (`tests/parity_corpus.rs:564-567`):

```rust
let passed = rows
    .iter()
    .filter(|row| matches!(row.verdict, Verdict::Pass))
    .count();
```

and the only failing condition is a `Fail` (`:579-587`):

```rust
assert!(
    failed.is_empty(),
    "{} shape(s) diverged: {:?}",
```

`Verdict::Skip` (`:361-365`) is printed as `SKIP(reason)` and the `graded` column is `-`
(`:480`), so a skip is never rendered as evidence. **What is not guarded:** `passed` and
`skipped` are printed (`:573-578`) but never asserted, and `load_shapes` only asserts
*non-empty* (`:534-538`). A corpus directory holding one `Q54.json`, or one where every
shape trips `fixture_blocker`, produces `1 shapes: 0 PASS, 0 FAIL, 1 SKIP` and exit 0 —
a green test with zero graded content. `docs/CORPUS-RUNNER.md`'s "A missing or malformed
corpus is a loud failure, never a smaller run" is therefore only true of a *missing
directory*, not of a *thinned corpus*: a 53-file copy (or a copy with one `raw_query`
edited) runs silently. Smallest fix: assert `shapes.len() == 54`, assert the id set is
exactly `Q01..Q54`, and assert a floor on `passed` (15 today).

Second-order: a SKIP-equivalent hides *inside* PASS. `fixture_blocker` is consulted at
`:479` and the comparison at `:486-500` can still be vacuous — see (ii). And clause (b) is
silently dropped when our side is short (`:508`, `if recorded_top.len() >= 10 && our_top.len() >= 10`),
with `graded` left at `"set-eq"` — a metric component that never fires and never says so in
the row (the doc does say it, in prose, once).

### (ii) Which shapes are HOLLOW on a fixture index

Two kinds of hollow, both invisible to the printed `PASS`:

**Vacuous — expected set is empty after the inventory restriction, so any code path that
returns nothing (or nothing matching) passes.** `tests/parity_corpus.rs:486`
`let expected: BTreeSet<HitKey> = shape.identifiers.intersection(inventory).cloned().collect();`

- **Q16** `group:B6`, **Q17** `group:b6` — recorded **3** hits each, 0 in the fixture.
- **Q19** `dataset:bxdpublish` — recorded **13 757** hits, 0 in the fixture.
- **Q28** `mean:10..20` — recorded **1 206**, 0 in the fixture (its five means are
  3.02/6.60/9.88/3.41/7.78).
- **Q30** `mean:10..` — recorded **8 644**, 0 in the fixture.
- **Q38** `year:2030..2050` — recorded 0.
- **Q39** `mean:99999..100000` — recorded 0.

Only two of those seven are genuinely empty-vs-empty; five PASS while production answered
thousands of documents. `docs/CORPUS-RUNNER.md` calls all seven "both empty", which
understates exactly the five that matter.

**All-five — the fixture holds five documents, so a renderer that matches *everything*
prints the same PASS.** **Q15** `species:mouse`, **Q29** `mean:..15`, **Q37**
`year:2010..2020` (every fixture doc is `species=mouse`, `mean ≤ 15`, `year=2013`). These
catch a *dropped* or *mis-slotted* term (0 hits ≠ 5) but not an *ignored* one.

**Genuinely discriminating on the match set: exactly two — Q31** `peak:5..15` (2 of 5, slot
1) and **Q36** `additive:0.5..1.0` (2 of 5, slot 4). **Error classes: Q42, Q51, Q52** —
these grade the exception class only (the message is checked solely for being non-empty,
`:416`, so a crate that answered Q51 with "Unknown range operation" would still PASS,
contradicting the table in `docs/CORPUS-RUNNER.md`).

So: **10 of the 15 PASS cannot distinguish a correct renderer from one that emits garbage
for the term**, 2 can, 3 grade a class. A grammar bug in `dataset:`/`group:`/`mean:`
rendering (slot 0), or in `species:`, prints PASS.

### (iii) Is the metric the corpus's own, or a new invention?

**Mostly the corpus's own, with one invented rule and one dead rule.**

Same as GN4:
- `const JACCARD_FLOOR: f64 = 0.8;` (`:60`) ↔ GN4 `tests/search_parity.rs:64`.
- `type HitKey = (Option<String>, Option<String>)` with "absent is a distinct key, never
  coerced to `\"\"`" (`:57-61`) ↔ GN4 `comparator::HitKey` (`:459-461`).
- `|A∩B|/|A∪B|`, empty union ⇒ 1.0 (`:602-608`) ↔ GN4 `:587-593` (verbatim, including
  the empty-union convention).
- Clause (b) only when recorded ≥ 10 (`:281-287`) ↔ GN4 `clause_b_applies`.
- Set equality as missing+extra counts (`:488-500`) ↔ GN4 `grade_match`.

New, and load-bearing:
- **The inventory restriction** (`:486`). GN4 has no such rule; it compares the full
  recorded set against the full replayed set. Restricting the *expected* side to documents
  the double happens to hold converts "the engine found the wrong 13 757 documents" into
  "PASS". It is defensible as a way to grade the *grammar* rather than the double, but it
  is a new metric, and it is what makes the seven shapes in (ii) vacuous.
- **`our_top.len() >= 10`** (`:508`). GN4 applies clause (b) on the recorded count alone.
  Net effect here: clause (b) **never executes** on this corpus (our side can never exceed
  5), so `jaccard()` lives only in unit tests and the `graded` column can only ever say
  `set-eq` or `error-class`.

Also not the corpus's: the corpus grades **HTTP status + `invalid_query` code** at the
route; the runner grades the engine's `xapian_error_type`. That is the right projection for
this seam (the crate owns no status), but it is a proxy, and Q48/Q49 — which the corpus
grades as `Surface400` (400 `invalid_parameter`, zero upstream calls) — are SKIPped here
because the validation lives in the route (`docs/DECISIONS.md` row 2). Two of the 54
shapes the real harness *counts* are therefore absent from this runner's total.

---

## c) The single biggest remaining risk to a parity claim

**`tests/parity_corpus.rs:486` — the expected set is intersected with the fixture's
inventory, and the inventory is five documents.** With that restriction, "0 FAIL / 15
PASS" is compatible with the crate returning the wrong documents for every shape whose
recorded answer happens to fall outside those five: Q19 (`dataset:`, 13 757 recorded),
Q16/Q17 (`group:`, 3 each), Q28 (1 206), Q30 (8 644) are graded against the empty set. The
half of the grammar those shapes cover — `dataset:`/`group:` boolean prefixes and
`mean:` slot 0 — has **no falsifying evidence anywhere in the tree**: the fixture's five
documents all share one dataset, one group, one species and one year, and the only
value-ranges with a discriminating subset are slots 1 and 4. Combined with `:508` making
clause (b) dead, the corpus runner's match-set half is, in practice, two assertions about
two documents out of 230 031 recorded hits.

Second (same family, different file): `src/grammar.rs:589-593` emits
`TermKind::FreeText` with the token exactly as written, so the F2/F3 fixes classify
`year:2014` / `position:` correctly but hand a backend `"year:2014"` / `"position:"`
instead of the "`mean 5` phrase" GN4 measures (`native.rs:723-724`). Any real backend that
takes `TermKind::FreeText`'s doc seriously ("the token as written, to be stemmed and
scored", `:272-274`) diverges on Q53's 576-hit class and on every bare-numeric query.

---

## d) Verdict and remaining blockers

**Verdict: PASS-WITH-FIXES** — the fix pass is real work, not rounding-up: F1, F2, F3, F4,
F5, F7 hold up under the ground truth, DECISIONS was updated to match the code, and the
runner never dresses a skip as a pass. It is not yet a parity claim.

Remaining blockers (five):

1. **`src/grammar.rs:679`** — `alcohol AND NOT dependence` (and `a AND (NOT b)`) is a 422:
   `Some(previous) => is_operator(previous) && !exempt(previous)` treats *any* operator
   followed by `NOT` as dangling. xapian accepts the form; F6 replaced a missing 422 with a
   spurious one. Smallest fix: `&& !exempt(term)` added to that arm, plus a test case.
2. **`tests/parity_corpus.rs:486` + `:564-587`** — the runner can be green with no graded
   evidence: the expected set is inventory-restricted (7 shapes vacuous, 3 all-five), and
   neither `passed` nor `shapes.len()` is asserted, so a thinned corpus and an all-SKIP run
   both exit 0.
3. **`src/grammar.rs:589-593`** — the free-text fallback keeps `field:`; `position:` and
   `year:2014` are the two classes the F2/F3 fixes claim, and neither is rendered the way
   GN4's parser renders it (`native.rs:723-724`). Nothing in the corpus can see it.
4. **`src/grammar.rs:549` + `:700-717`** — `mean:"1..2"` is a 422 in the crate and phrase
   content at GN4 (`native.rs:380-385`, `:398-401`): the `..` scan is not quote-aware.
   MINOR in blast radius, but it is a false rejection, the one direction a parity claim
   cannot afford.
5. **`docs/WORKER-CRATE.md:23,26,27` and `src/bin/gn-search.rs:280` vs `:22-24`** — the
   crate's delivered report still documents `serde_json::Map`, `page_bounds` and a fixture
   that "honours `limit`" (three of the seven fixes contradict it), and the dev server
   answers F1's refusal with 500 `internal_error` where it promises 503
   `index_unavailable`. Documentation-level, but it is the file the next reader trusts.

---
