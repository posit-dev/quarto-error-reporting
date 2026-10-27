# Coalesce diagnostics located in `Concat`-shaped sources

**GitHub:** [#10](https://github.com/posit-dev/quarto-error-reporting/issues/10)
· **Strand:** qe-bju8tz7x (follow-up qe-rkscku9j; source-map qsm-m9xmq12h; q2 bd-btoxaf07)
· **Status:** in progress — PRs open, releases await review

## Overview

`coalesce_by_source` keys each diagnostic by
`SourceInfo::resolve_byte_range()`, which returns `None` for `Concat` and
for `Substring { parent: Concat }`. Since quarto-yaml 0.5.0 every folded
multi-line scalar (plain, quoted, or block) carries a `Concat` content
provenance, and every diagnostic re-parsed out of such a scalar inherits
that shape. Those diagnostics therefore never group: one project-wide
config problem prints once per rendered page.

Reproduced on 2026-10-27 with q2 `0.36.0-nightly.20261024` using the
issue's fixture (three pages, `page-footer.left` single-line,
`page-footer.right` folded over two lines):

| scalar | shape | printed | expected |
|---|---|---|---|
| `left` | `Original` | 2 groups, each "Affected files: a, b, c" | same |
| `right` | `Substring { parent: Concat }` | 12 singletons | 4 groups |

Total: 14 `Q-2-9` emissions where 6 are expected.

The renderer has no trouble with these locations (`map_offset` is
piece-aware); only the grouping key gives up.

## Decision

Implement the issue's **option 3 + option 2**:

1. **quarto-source-map** gains `SourceInfo::resolved_runs()`, the
   canonical "which source bytes does this location's content map onto"
   form: an ordered list of `(FileId, Range<usize>)` runs with
   `Concat` expanded and `Substring`s sliced across the pieces they
   cover. `resolve_byte_range` / `preimage_in` are untouched (their
   callers need a single contiguous range or byte-identity; grouping
   needs neither).
2. **quarto-error-reporting** keys `LocationKey` on the full run list,
   each run's file resolved through the entry's `SourceContext` with the
   existing `FileKey` path-vs-raw rule.
3. **q2** only bumps the two crates and adds an end-to-end test for the
   folded-scalar case. No code change expected on the q2 side.

### Why not the alternatives

- **Option 1 (key on `map_offset` endpoints).** `map_offset` needs a
  `SourceContext` holding the file's content, so entries without a
  context (or whose context doesn't register the file) would lose
  coalescing they have today, and the coalescer would start reading file
  contents just to compute a key. It also discards the interior of the
  location: two different piece lists with equal endpoints would merge.
- **Option 2 alone (all logic in error-reporting).** Avoids a
  quarto-source-map release, but re-implements `SourcePiece` semantics
  (content length vs source length, the exclusive-end rule) outside the
  crate that owns them. Kept as the fallback if we decide a source-map
  release is not worth it; the slicing code is the same either way, only
  its home differs.

## Design

### `SourceInfo::resolved_runs()` (quarto-source-map)

```rust
/// One contiguous byte run in one source file.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceRun {
    pub file_id: FileId,
    pub start: usize,
    pub end: usize,
}

impl SourceInfo {
    /// Ordered source runs that this location's content maps onto.
    pub fn resolved_runs(&self) -> Option<Vec<SourceRun>>;
}
```

Semantics, by variant, for the private worker
`collect_runs(&self, start, end, out)` where `[start, end)` is in
`self`'s content coordinates:

- `Original { file_id, s0, .. }` → one run `(file_id, s0+start .. s0+end)`.
  Affine, unclamped, exactly like `resolve_byte_range` and `map_offset`.
- `Substring { parent, ps, .. }` → `parent.collect_runs(ps+start, ps+end)`.
  Works for any parent, including `Concat` (this is the case that
  `resolve_byte_range` and `preimage_in` refuse).
- `Concat { pieces }` → for each piece whose content span
  `[offset_in_concat, offset_in_concat + length)` overlaps `[start, end)`,
  recurse into `piece.source_info` with the overlap translated into
  piece coordinates:
  - an overlap starting at the piece's content start maps to piece
    offset 0; one ending at the piece's content end maps to
    `piece.source_info.length()` (the piece's *source* length, mirroring
    the exclusive-end rule in `map_offset`). A fully covered
    replacement piece (`''` → `'`, folded newline → space) therefore
    yields its whole source run;
  - an interior offset into a piece composes affinely, clamped to the
    piece's source length. For verbatim pieces this is exact; for a
    partially covered replacement piece it is the same approximation
    `map_offset` already makes.
  - An empty range (`start == end`) yields one zero-length run at the
    position `map_offset` would report (the piece containing `start`,
    or the end of the last piece for `start == total`).
  - A non-empty range overlapping no piece → `None`.
- `Generated` → delegate to `invocation_anchor()` with the same range
  (as `resolve_byte_range` does); `None` without an anchor. The
  top-level `resolved_runs()` on a `Generated` returns the anchor's full
  runs, matching `resolve_byte_range`'s result on the same node.

Post-processing in `resolved_runs()`: merge adjacent runs in the same
file (`prev.end == next.start`). A `Concat` whose pieces tile the source
contiguously therefore keys identically to the equivalent `Original`,
and a one-piece `Concat` equals its piece. An empty result is `None`.

Documented caveat: like `preimage_in`'s hull, this is an **offset
claim, not a byte-identity claim**. It licenses locating, not copying.

### `LocationKey` (quarto-error-reporting)

```rust
struct LocationKey {
    runs: Vec<(FileKey, usize, usize)>,
}
```

`LocationKey::from` calls `resolved_runs()`, then resolves each run's
`FileId` through the entry's own context with the existing
`FileKey::Path` / `FileKey::Raw` rule. Everything else in
`coalesce_by_source` is unchanged: first encounter supplies the
representative and context, encounter order is preserved, `None` keys
pass through as singletons.

The module docs' "What does not coalesce" section shrinks to: no
`location`, or a `Generated` without an `Invocation` anchor (and the
degenerate shapes where `resolved_runs` returns `None`). The stale
`FilterProvenance` reference goes away (issue aside).

### Versions and dependency order

| repo | change | version | depends on |
|---|---|---|---|
| quarto-source-map | add `SourceRun`, `resolved_runs` (additive) | 0.3.0 → **0.3.1** | — |
| quarto-error-reporting | new key; dep `quarto-source-map = "0.3.1"` | 0.4.0 → **0.4.1** | 0.3.1 on crates.io |
| q2 | workspace deps → 0.3.1 / 0.4.1, `cargo update`, new test | nightly | 0.4.1 on crates.io |

quarto-yaml needs no change (its `quarto-source-map = "0.3.0"` unifies to
0.3.1; it does not use the coalescer). q2's lockfile already has a single
copy of each crate, so no duplicate-type risk.

Both crates release by merging a PR that includes the version bump
(precedent: #9 "cut 0.4.0"); CI publishes via trusted publishing. Each
repo's CI builds `--locked`, so `Cargo.lock` is committed with the bump.

### Local development before each publish

- error-reporting is developed against an uncommitted
  `[patch.crates-io] quarto-source-map = { path = "../quarto-source-map" }`
  until 0.3.1 is on crates.io, then the patch is dropped and
  `cargo update -p quarto-source-map` pins 0.3.1.
- q2 is verified the same way before either publish (precedent:
  quarto-yaml's saphyr plan, which ran q2's dependent-crate tests with
  `[patch.crates-io]` pointed at the branch): patch both crates to the
  local checkouts, build `q2`, run the issue fixture, run the
  `coalesced_diagnostics` and `json_errors` integration tests.

## Checklist

### Phase 1: quarto-source-map 0.3.1

- [x] Create strand in quarto-source-map; add
      `claude-notes/plans/2026-10-27-resolved-runs.md` referencing this
      plan.
- [x] Branch `resolved-runs`. Add `SourceRun` and
      `SourceInfo::resolved_runs` in `src/source_info.rs`; re-export
      `SourceRun` from `lib.rs`.
- [x] Unit tests (in `source_info.rs`):
  - [x] `Original` → one run; `Substring` chain composes.
  - [x] `Concat` of two same-file pieces with a gap (folded scalar) →
        two runs; contiguous pieces → one merged run equal to the
        `Original`'s; one-piece `Concat` equals its piece.
  - [x] `Substring { parent: Concat }` inside one piece → one run;
        spanning two pieces → two sliced runs; covering a replacement
        piece wholly → the piece's full source run; partially → clamped.
  - [x] Cross-file `Concat` (notebook cells) → per-file runs, never
        merged across files.
  - [x] Empty range at a piece boundary and at the exclusive end →
        zero-length run where `map_offset` points.
  - [x] `Generated` with anchor → anchor's runs; without → `None`.
  - [x] `ProvenanceBuilder::in_parent` over a `Concat` parent
        (the yaml shape) round-trips to the expected flattened runs.
- [x] Doc comment cross-references on `resolve_byte_range` and
      `preimage_in` pointing at `resolved_runs` for the "I need the
      pieces" case.
- [x] `cargo fmt --all --check`, `cargo clippy --all-targets --locked
      -- -D warnings`, `cargo test --locked`.
- [x] Bump to 0.3.1 as the tip commit; draft PR
      [quarto-source-map#9](https://github.com/posit-dev/quarto-source-map/pull/9).
- [ ] Merge, confirm crates.io publish and `v0.3.1` tag.

### Phase 2: quarto-error-reporting 0.4.1

- [x] Create strand (link GH #10); set this plan's strand field.
- [x] File the follow-up strand "json.rs: emit `resolved_runs` pieces in
      structured provenance" with `discovered-from` this strand; link it
      from PR #10's description. Not implemented in this change.
- [x] Branch `coalesce-concat-runs`. Rewrite `LocationKey` in
      `src/coalesce.rs`; update module docs (drop `FilterProvenance`,
      describe the run-list key and the `Generated` anchor rule).
- [x] Tests in `src/coalesce.rs`:
  - [x] Replace `concat_location_passes_through_as_singleton` with
        "same `Concat` location across two entries → one group, two
        affected files".
  - [x] Same `Substring { parent: Concat }` → one group.
  - [x] Same piece list, contexts resolving the id to different paths →
        two groups (keeps #3's protection).
  - [x] Different piece lists with equal endpoints → two groups (the
        false-merge option 1 would have made).
  - [x] Contiguously tiled `Concat` groups with the equivalent
        `Original`.
  - [x] Cross-file `Concat` where one file resolves and the other does
        not → still one group per identical location (key is
        per-run, `Path` and `Raw` mixed is fine as long as both entries
        agree).
  - [x] Existing `Original`, `None`, encounter-order, representative,
        and raw-id tests unchanged.
- [x] Dep set to `quarto-source-map = "0.3.1"`; a committed
      `[patch.crates-io]` to the source-map PR branch (git, not a local
      path) keeps CI reproducible until 0.3.1 publishes.
- [ ] After 0.3.1 publishes: remove the patch section,
      `cargo update -p quarto-source-map`, commit `Cargo.lock`.
- [x] `cargo xtask verify`.
- [x] Bump to 0.4.1 as the tip commit; draft PR
      [#11](https://github.com/posit-dev/quarto-error-reporting/pull/11)
      ("Fixes #10").
- [ ] Merge, confirm publish and `v0.4.1` tag.

### Phase 3: q2

- [x] Branch `coalesce-concat-runs` in q2 (strand bd-btoxaf07).
      `[workspace.dependencies]`: `quarto-source-map = "0.3.1"`,
      `quarto-error-reporting = "0.4.1"`; committed `[patch.crates-io]`
      git entries to both PR branches (REMOVE BEFORE MERGE); lockfile
      updated.
- [ ] Integration tests in
      `crates/quarto/tests/integration/coalesced_diagnostics.rs`:
  - [ ] Issue fixture (folded single-quoted `page-footer.right`, three
        pages): exactly 6 `Q-2-9` emissions, each tailed by all three
        pages; the `_quarto.yml` snippet still renders for the folded
        ones.
  - [ ] Block scalar (`center: |`) fixture extended to three pages:
        2 emissions, each with the "Affected files:" tail (the
        existing single-page test keeps asserting 2 and no tail).
- [ ] Before the publishes land: verify with `[patch.crates-io]` to the
      local checkouts; after: verify with the released versions. Run
      the issue fixture against the built `q2` (expect 6 / 6 affected
      lines) and, if convenient, the posit-dev/product-doc-theme
      example from the issue (expect 8 instead of 24).
- [ ] Run `coalesced_diagnostics`, `json_errors`,
      `diagnostic_render_panic_boundary` integration tests plus
      `quarto-core` unit tests.
- [ ] PR to q2; record the verification run here.

## Decisions from review (2026-10-27)

1. **Shape: `SourceRun` struct.** `SourceRun { file_id, start, end }` and
   `(FileId, Range<usize>)` are both 24 bytes with identical hashing
   cost, so the choice is API readability only. The real cost in either
   shape is one `Vec` allocation per keyed diagnostic, paid once per
   render in the coalescer (tens to hundreds of entries), which does not
   justify a `SmallVec`.
2. **Zero-length runs are kept**, merged into a neighbour when adjacent.
3. **One PR per repo, including the version bump.** Each PR description
   links the GH issue, this plan, and the sibling PRs (upstream it
   depends on, downstream it unblocks).
4. **`json.rs` follow-up: yes.** Filed as a separate strand at execution
   time (Phase 2 checklist), not in this change: switch the structured
   provenance output to `resolved_runs` so JSON consumers see every
   piece of a `Concat`-shaped location instead of only the mapped
   endpoints.

## Notes

- q2's `attach_config_source` runs *after* `coalesce_by_source`, so the
  key is computed against each page's own context. That works today
  because `MetadataMergeStage` pre-registers `_quarto.yml` into every
  document's context, and `_quarto.yml` uses a hash-based id so the
  `Raw` fallback is stable anyway. No change needed, but it is the
  reason the key must not depend on `attach_config_source`.
- The Q-2-9 location is built in
  `crates/pampa/src/pandoc/treesitter.rs` (`trimmed_source_info`) as a
  `Substring` over the scalar's content provenance; for a folded scalar
  that parent is the `Concat` from `ProvenanceBuilder::finish`, whose
  pieces are `Original`s (file root) or `Substring`s over a further
  parent (front matter inside a `.qmd`). `resolved_runs` handles both
  through the same recursion.
- quarto-yaml-validation depends on quarto-error-reporting 0.3.2; it is
  not in q2's dependency graph and is unaffected.
