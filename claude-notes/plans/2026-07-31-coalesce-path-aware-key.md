# Path-aware LocationKey for `coalesce_by_source`

**Strand:** qe-kjherkvf · **GitHub:** [#3](https://github.com/posit-dev/quarto-error-reporting/issues/3)

## Overview

`LocationKey` in `src/coalesce.rs` is `(file_id, start, end)` from
`SourceInfo::resolve_byte_range()`. Raw `file_id` is only globally
meaningful for hash-based ids (e.g. `quarto_yaml::file_id_for_filename`).
Sequential per-context ids are not: in q2 every document's primary file is
`FileId(0)` in its own `SourceContext`, so two diagnostics in *different*
documents at identical byte offsets share a key and falsely coalesce into
one group, with a misleading representative/snippet attributed to the
first file encountered.

Fix (per the issue, no signature change): each input entry already carries
its `Option<SourceContext>`, so derive the key's file component from the
entry's own context when possible:

- If `ctx.get_file(FileId(file_id))` resolves → key on the file **path**
  (+ start/end). `SourceContext::get_file` already handles both the sparse
  `file_id_map` (hash-based ids) and direct indexing (sequential ids).
- Otherwise (no context, or id unregistered in this entry's context) →
  fall back to the raw `file_id` (+ start/end).

## Design

Replace the key's `file_id: usize` with a two-variant discriminant so path
keys and raw-id keys can never collide with each other:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum FileKey {
    /// FileId resolved to a registered file in the entry's own context.
    Path(String),
    /// Unresolvable id — no context, or id not registered in it.
    Raw(usize),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct LocationKey {
    file: FileKey,
    start: usize,
    end: usize,
}
```

`LocationKey::from` grows a `ctx: Option<&SourceContext>` parameter (it's
a private helper; only the loop in `coalesce_by_source` calls it). The
public signature of `coalesce_by_source` is unchanged.

### Behavior notes / accepted edges

- **Path string taken verbatim** (`SourceFile::path`), no canonicalization.
  Two contexts registering the same file under different spellings
  (`./_quarto.yml` vs `_quarto.yml`) would form two groups — split, not
  false-merge, so it's the safe failure direction. Callers that want
  merging must register consistent paths (q2 does).
- **Mixed resolvability of the same hash id:** if the same hash-based id
  is registered in entry A's context (→ `Path`) but entry B has no
  context (→ `Raw`), the two entries split into two groups. Also
  split-not-merge, so acceptable; documenting it in the module docs.
- Entries with `location: None`, `Concat`, `FilterProvenance` are
  untouched — still singleton pass-through before any key is built.

## Checklist

- [x] Update module docs in `src/coalesce.rs`: primary key is now
      *(resolved file path | raw file id, start, end)*; document the
      split-not-merge edges above.
- [x] Introduce `FileKey` enum; rework `LocationKey` and
      `LocationKey::from(info, ctx)` to resolve via
      `SourceContext::get_file`.
- [x] Thread `source_context.as_ref()` into key construction in the
      `coalesce_by_source` loop (key must be built *before* the triple is
      moved into `groups`).
- [x] Tests (issue's suggested set):
  - [x] same hash-based id + span across N entries with per-entry
        contexts registering it under the same path → one group, N
        affected files in encounter order
        (`hash_based_id_with_same_path_collapses_across_contexts`);
  - [x] sequential-id collision: two contexts each registering a
        *different* path under `FileId(0)`, same offsets → **two**
        groups (the regression test for the bug,
        `sequential_id_collision_across_contexts_does_not_collapse`);
  - [x] resolvable-vs-unresolvable same raw id (one entry with context,
        one without) → two groups (documents the accepted edge,
        `resolvable_and_unresolvable_same_raw_id_do_not_collapse`);
  - [x] existing tests stay green. One intentional update:
        `first_encounter_supplies_representative_and_context` used to
        register FileId(1) under *different* paths in its two contexts
        — under the new semantics those are correctly two files, so
        the test now registers the same path (`config.yml`) in both
        and additionally asserts the kept SourceContext is the first
        entry's (by content marker).
- [x] `cargo xtask verify` (fmt, clippy, build, tests across feature sets)
      — all 6 checks green.
- [ ] Branch + PR referencing GH #3 (`Fixes #3`), strand qe-kjherkvf in
      the body. Branch: `fix/coalesce-path-aware-key`.
- [ ] After merge: comment on/close strand qe-kjherkvf; confirm the
      release workflow published 0.2.1.

## Release (decided)

User approved landing on main with a bump (2026-07-31): the fix PR
itself bumps `0.2.0` → `0.2.1` (public API unchanged; grouping-behavior
bugfix), so merging it triggers the release workflow and publishes to
crates.io. q2 then bumps its pin from 0.1.0. No 0.1.x backport.
