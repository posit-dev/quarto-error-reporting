# Cross-piece renderer fix — diagnostics on `Concat` sources

## Overview

Diagnostics whose location resolves through a multi-piece `Concat`
SourceInfo render against the wrong file today. The renderer takes the
report's file from `root_file_id()` — the first rooted `Concat` piece —
while offsets resolve piece-aware via `map_offset`, so a diagnostic rooted
wholly within piece N > 1 gets piece 1's file label and piece 1's snippet
at piece-N offsets. A span straddling two pieces is silently clamped into
piece 1. And details rooted in a piece other than the report's root are
silently skipped.

Motivated by q2's ipynb content processor (quarto-dev/q2 plan 7c, §
"Phase 1 (upstream)"): per-cell virtual files form a `Concat`, and a parse
error in cell N must render labeled `notebook.ipynb[cell N, markdown]`
with a correct in-cell snippet.

Branch: `fix/concat-renderer-cross-piece` (renamed from the stranded docs
branch `docs/snap-span-char-boundaries-rationale` per Gordon, 2026-09-24;
its docs commit — now `384982e` after rebasing onto origin/main `287d645`
= released 0.3.0 — rides in this PR's history).

## Fix contract (per q2 plan 7c § Implementation seams item 3)

Both renderer copies (`render_ariadne_source_context`,
`render_annotate_snippets_source_context`):

1. **Report file from `start_mapped.file_id`** (which `map_offset` already
   returns) instead of `root_file_id()`.
2. **Explicit straddle detection** (`start_mapped.file_id !=
   end_mapped.file_id`): render a one-line cross-file location block
   (`--> path1:L:C (spans through path2:L:C)`) with **no snippet** —
   never silently clamped into one piece.
3. **Per-detail mapped file, same-file filter corrected.** Each detail
   maps through its own location; details rooted (start and end) in the
   report's file stay inline labels. A detail rooted wholly in another
   piece renders as its own source block (ariadne: multi-source label +
   cache; annotate-snippets: additional `Snippet` element in the group) —
   no longer silently dropped. A detail whose own span straddles pieces is
   not representable inline and stays unrendered (pathological; documented).
4. **Single-file passthrough preserved**: a `Concat` whose pieces all root
   to one file (the percent/spin shape) renders exactly as before.

Foreign-file labels in ariadne sort after all same-file labels
(`with_order` above any realistic span end) so the main file's snippet
group stays intact.

## Work items

- [x] Rename stranded docs branch to `fix/concat-renderer-cross-piece`;
      rebase onto `origin/main` (`287d645`, 0.3.0 = q2's pin)
- [x] TDD: failing tests, both feature gates — single-file passthrough;
      main span in first piece; in a later piece; straddling span; detail
      in another piece
- [x] Fix `render_ariadne_source_context` (file from start_mapped;
      straddle textual block; multi-source cache + foreign detail labels)
- [x] Fix `render_annotate_snippets_source_context` (same contract;
      foreign details as additional snippet elements)
- [ ] `cargo xtask verify` green (fmt, clippy both feature sets, tests
      both feature sets, `--locked`)
- [ ] Push branch, open PR (merge + 0.3.1 release: Gordon)
- [ ] q2 side (separate repo): `cargo update -p quarto-error-reporting`,
      flagship rendering-half assertions

## Notes

- No braid skein is configured in this checkout (no `.braid.toml`); this
  plan file is the tracking artifact. Flag to Gordon if the repo is meant
  to have one.
- Line references in the q2 plan (`diagnostic.rs:819/:925/:1022/:1077`)
  are against 0.2.2; post-rebase the sites are the two
  `render_*_source_context` functions (~920/~1123), confirmed by reading
  the code on 2026-09-24.
