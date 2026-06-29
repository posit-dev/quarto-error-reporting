# Migrating from `ariadne` to `annotate-snippets`

**Status:** pluggable renderer implemented (both renderers coexist behind features)
**Tracking strand:** `qe-rtjjfff0`
**Date:** 2026-06-29

> **Update (2026-06-29):** Rather than a breaking hard-swap, we shipped a
> **feature-gated, runtime-selectable** renderer seam — see
> "Implemented: pluggable renderer" below. The migration-plan sections that
> follow remain the reference for an eventual default-flip / ariadne removal,
> but nothing is forced now.

## Implemented: pluggable renderer

Both renderers now coexist; the choice is a feature + a runtime arg, with **no
API break**.

- **Cargo features:** `ariadne` (default-on — existing behavior unchanged) and
  `annotate-snippets` (opt-in). Both deps are `optional`. Valid to build with
  neither (source excerpts degrade to the structured text block), either, or
  both.
- **`pub enum SourceRenderer`** (`diagnostic.rs`) with `#[cfg]`-gated variants
  `Ariadne` / `AnnotateSnippets` and `#[non_exhaustive]`; with no renderer
  feature it is uninhabited. `SourceRenderer::default_for_features()` resolves
  the default (prefers ariadne).
- **`DiagnosticMessage::to_text_with_renderer(ctx, &opts, Option<SourceRenderer>)`**
  is the new public seam. `None` = default renderer. `to_text` and
  `to_text_with_options` delegate with `None`, so every existing call site is
  untouched. (We deliberately did *not* add a field to `TextRenderOptions` — its
  public fields are struct-literal-constructed downstream, so that would have
  been the breaking change.)
- **`render_annotate_snippets_source_context`** (private, `annotate-snippets`
  0.12.16) mirrors the ariadne offset-mapping but emits the rust-lang look:
  native `error[CODE]:` header via `Title::id`, `-->` origin, `^` underlines.
  The ariadne renderer and its OSC 8 hyperlink helpers are now gated behind
  `feature = "ariadne"`.
- **`examples/renderer_selection.rs`** renders one diagnostic with each renderer
  for side-by-side comparison.

Verified visual output (hyperlinks off):

```
ariadne:                              annotate-snippets:
Error: [Q-14-1] Unknown theme         error[Q-14-1]: Unknown theme
   ╭─[ _quarto.yml:4:12 ]              --> _quarto.yml:4:12
   │                                    |
 4 │     theme: nosuchtheme            4 |     theme: nosuchtheme
   │            ─────┬─────              |            ^^^^^^^^^^^ `nosuchtheme` is not a known theme
   │                 ╰──── `nosuch…`
```

Behavioral deltas in the annotate-snippets path, **by design** (see the API gaps
section): no OSC 8 hyperlinks; all detail labels render as `Context` (no
per-`DetailKind` color, no `Fixed(249)` faded blend); empty-content "padding"
details are skipped (native folding replaces the elision hack). A trailing
newline is appended to the rendered excerpt so appended hints/details don't glue
onto the last source line.

**Not changed:** the JSON wire shape. `JsonDiagnostic.rendered` still uses the
default renderer via `to_text`, so `schemas/` and the `schema_drift` test are
untouched. Flipping the JSON feed to annotate-snippets (or letting it pick a
renderer) is a separate, later decision.

All four feature combinations build warning-clean (clippy) and pass tests; two
new tests cover the annotate-snippets path and renderer switching.

## Overview

We want to replace the [`ariadne`](https://crates.io/crates/ariadne) source-context
renderer with [`annotate-snippets`](https://github.com/rust-lang/annotate-snippets-rs),
the diagnostic renderer used by the rust-lang toolchain.

**Motivation is purely aesthetic.** There is no specific ariadne feature gap,
bug, or performance problem driving this. We simply prefer the look of the
rust-lang diagnostic style (the `error[E0308]: ...` / `-->` / gutter-bar form)
by a fair margin, and want the crate's rendered output to match it.

This is expected to be a **backwards-incompatible change to the rendered text
output** (and therefore to the `rendered` field on the JSON wire shape). The
structured data model (`DiagnosticMessage`, builder, catalog, JSON fields other
than `rendered`) does **not** need to change. The goal of this document is to
map the blast radius precisely and lay out a migration that downstream
consumers can follow.

## Where ariadne actually lives

Ariadne is a remarkably contained dependency. The entire integration is **one
private method** plus a small amount of supporting glue. The public API does
*not* expose any ariadne type.

### The single rendering site

`src/diagnostic.rs` — `DiagnosticMessage::render_ariadne_source_context()`
(lines ~641–823). This is the **only** place ariadne types are used. It:

1. `use ariadne::{Color, Config, IndexType, Label, Report, ReportKind, Source}`.
2. Resolves the diagnostic's `SourceInfo` location back to a root file +
   original byte offsets via `quarto_source_map` (`root_file_id`, `map_offset`,
   `length`).
3. Reads file content (from the in-memory `SourceContext` or disk).
4. Builds a `Report` with:
   - `ReportKind::{Error,Warning,Advice}` chosen from `DiagnosticKind`.
   - `Config::default().with_index_type(IndexType::Byte)` — **byte-indexed
     spans** (our offsets are byte offsets, not char offsets). This is a key
     requirement for the replacement.
   - `with_message(...)` for the title (optionally `[CODE] title`).
   - A main `Label` over `start..end` with `with_message`, `with_color`, and
     `with_order(end_offset)`.
   - One additional `Label` per detail that has a `location` in the same file,
     each with its own color and `with_order`.
5. Renders to a `Vec<u8>` via `report.write((path, Source::from(content)), buf)`
   and converts to `String`.

### Ariadne-specific behaviors we depend on / work around

These are the load-bearing details a replacement has to reproduce or render
moot:

- **`IndexType::Byte`** — spans are byte offsets. (annotate-snippets must
  index the same way; see API mapping below.)
- **`Color::Fixed(249)`** — `ARIADNE_UNIMPORTANT_COLOR`, a hand-copied mirror of
  ariadne 0.6.0's private `Config::unimportant_color()`. Used so `DetailKind::Faded`
  labels blend into unlabelled text. A comment warns to bump it if ariadne
  upgrades. (`src/diagnostic.rs:649-654`)
- **`with_order(end_offset)` on every label** — a workaround for ariadne's
  label-grouping algorithm, which splits one snippet into duplicated blocks when
  multi-line main labels and per-line "padding" labels land in different groups.
  We sort by end offset to force extend-not-split. (`src/diagnostic.rs:740-753`)
- **Empty-content "padding" detail labels** — details with empty content exist
  *only* to force ariadne to display a source line it would otherwise elide in
  the middle of a long multi-line span. We give them no message so no arrow row
  is drawn. (`src/diagnostic.rs:782-793`) **This whole hack exists because
  ariadne elides middle lines of multi-line spans.** If annotate-snippets does
  not elide (or elides differently), this workaround changes or disappears.
- **OSC 8 hyperlink post-processing** — we wrap the file path in an OSC 8
  terminal hyperlink *before* handing it to ariadne, then post-process the
  output (`extend_hyperlink_to_include_line_column`, ~lines 825-887) to pull the
  `:line:column` suffix that ariadne appends *inside* the hyperlink. This is
  tightly coupled to ariadne's `path:line:column` output format and will need
  rework against annotate-snippets' header/origin format.

### Conditional layout in `to_text_with_options`

`src/diagnostic.rs:351-474`. The renderer decides `has_ariadne` (do we have a
location *and* a `SourceContext`?). The branch matters:

- **With ariadne:** ariadne draws the title, code, problem, and located details;
  the surrounding code then appends only *unlocated* details and *all* hints.
- **Without ariadne:** a tidyverse-style plain-text block prints everything
  (title, `at file:line:col`, problem, details, hints).

So the division of labor between "what the snippet renderer draws" and "what we
append by hand" is baked into this method and will need to be re-checked against
whatever annotate-snippets draws (e.g. does it draw the title line? the code?).

### Coalescing wrapper

`src/coalesce.rs` — `CoalescedDiagnostic::to_text_with_options` calls
`representative.to_text_with_options(...)` and appends an "Affected files:" tail.
No direct ariadne use; it inherits whatever the body renderer produces. Only the
doc comments mention ariadne.

## Downstream / wire-format blast radius

### The `rendered` JSON field (the real downstream contract)

`src/json.rs` (behind the default-off `json` feature):

- `JsonDiagnostic.rendered: Option<String>` (and the same on
  `JsonPass1Failure`) is the **pre-rendered ANSI source-context snippet** —
  literally the ariadne output. Populated whenever a diagnostic has a location
  (`src/json.rs:257-271`).
- Documented as: "Same text the `q2 render` CLI prints to stdout — ANSI-coded;
  strip on the JS side for browser display. Consumers can render this verbatim
  in a `<pre>` block."
- **Known consumers** (per the code comments): `wasm-quarto-hub-client` (WASM
  render bridge) and `quarto-preview` (server-side diagnostics endpoint); the
  `q2-preview` SPA renders it. These consumers display the ANSI text in a
  `<pre>`. Switching renderers changes the *visual* content of this field but
  **not its type or contract** (still `Option<String>`, still ANSI). Consumers
  that render verbatim keep working; only the appearance changes — which is the
  whole point.

### Schema files mention ariadne by name

`schemas/json-diagnostic.json` and `schemas/json-pass1-failure.json` embed the
field doc comment, which says "Pre-rendered ariadne source-context snippet". The
text is generated from the Rust doc comments via `schemars`. Editing the doc
comment to drop "ariadne" will fail `tests/schema_drift.rs` until regenerated
with `QUARTO_REGEN_SCHEMAS=1`. (Cosmetic, but it's a real test gate.)

### Crate metadata / docs that name ariadne

- `Cargo.toml`: `description` ("ariadne-rendered"), `keywords = [..., "ariadne", ...]`,
  and the `ariadne = "0.6"` dependency line.
- `README.md`: links to ariadne, "rendered with ariadne".
- `src/lib.rs`: module docs credit ariadne as inspiration.
- `examples/with_location.rs`: comments reference ariadne.
- Many `src/diagnostic.rs` / `src/coalesce.rs` / `src/json.rs` comments.

### Tests that assert ariadne-specific output

The test surface is **small** — most tests use `to_text(None)` (no location → no
snippet renderer) and are unaffected:

- `src/json.rs:445-447` — asserts `rendered` contains U+256D (`╭`, ariadne's box
  corner). **This will change**: annotate-snippets uses a different glyph set
  (no enclosing box; it uses `-->` and a `|` gutter). This assertion must be
  rewritten to pin an annotate-snippets-stable marker.
- `src/diagnostic.rs` internal tests — exercise the structured/plain path more
  than the snippet glyphs, but any that assert on snippet text need review.

There are **no `insta`/snapshot tests** of the rendered snippet (grep found
none), which makes the swap easier — but also means we have no golden coverage
of the exact visual output today. Adding snapshots *before* the swap is
recommended so we can eyeball the diff.

## annotate-snippets API mapping

**Target version: `annotate-snippets` 0.12.16** (MSRV **1.85**, **edition 2024**).
We are already on edition 2024, so the toolchain requirement is a non-issue.

> ⚠️ The 0.12 line is a **full API redesign**. Older docs / blog posts / SO
> answers describe a different API (`Snippet`/`Slice`/`SourceAnnotation`). Write
> the migration against the current **Group/Report/Element** model below.

### Model

A `Report<'a> = &'a [Group<'a>]`. Each `Group` has a `Title` (a `Level` + message,
optional `id` = error code) and a list of `Element`s (snippets, messages, …). You
render the whole slice with a `Renderer`, which returns a `String`.

### Minimal shape (what our rewrite looks like)

```rust
use annotate_snippets::{AnnotationKind, Level, Renderer, Snippet};

let report = &[
    Level::ERROR
        .primary_title("literal out of range for `u8`")
        .id("Q-2-5")                       // → error[Q-2-5]: ...
        .element(
            Snippet::source(source)
                .line_start(1)
                .path("src/main.rs")
                .annotation(
                    AnnotationKind::Primary
                        .span(28..31)       // BYTE range — matches our offsets
                        .label("value does not fit"),
                ),
        ),
];
let out: String = Renderer::styled().render(report); // ::plain() for tests
```

### ariadne 0.6 → annotate-snippets 0.12 correspondence

| Our current ariadne usage | annotate-snippets equivalent | Notes |
|---|---|---|
| `ReportKind::{Error,Warning,Advice}` from `DiagnosticKind` | `Level::{ERROR,WARNING,INFO/NOTE/HELP}` | Direct. We have Error/Warning/Info/Note. |
| `Report::with_message("[CODE] title")` | `Level::X.primary_title(title).id(code)` | **Better:** `.id()` renders the native `error[CODE]: title` line. We can drop our manual `[CODE]` prefixing. |
| `Config::with_index_type(IndexType::Byte)` | **default** — `span()` is always byte-indexed | The byte-offset requirement is satisfied with no config. |
| main `Label` + per-detail `Label` | `Snippet::annotation(AnnotationKind::Primary…)` + `Context` annotations | Multiple per snippet; primary vs context role. |
| `Label::with_color(Color::Red/…)` | role-based: `AnnotationKind::Primary`/`Context` mapped through the renderer's per-role `anstyle::Style` | **Regression** — color is per-*role*, not per-label (see below). |
| `Color::Fixed(249)` for `DetailKind::Faded` | `AnnotationKind::Context` + globally set `Renderer::styled().context(Ansi256(249))` | **Cannot** color one annotation a fixed value while siblings differ. The faded trick becomes a global context style. |
| `with_order(end_offset)` grouping workaround | **delete** — no ordering knob; renderer derives order | Workaround was ariadne-specific. |
| empty-content "padding" labels to defeat mid-span elision | `Snippet::fold(false)` (show all lines) **or** `AnnotationKind::Visible` on the line to keep | **The whole padding hack is replaceable.** Folding is opt-out per snippet; `Visible` keeps an individual line. |
| `Config::with_char_set(...)` (we don't currently set it) | `Renderer::decor_style(DecorStyle)` (Unicode/ASCII) | Available if we want ASCII output. |
| `report.write((path, Source::from(content)), buf)` → `String` | `Renderer::styled()/plain().render(report)` → `String` | Returns owned `String`; wrap in `anstream` for TTY-aware color stripping. |
| OSC 8 terminal hyperlinks on the path | **NOT supported** | See gap below — this is the one real functional loss. |
| hints rendered by hand after the snippet | same, or as extra `Group`/`Message` elements | No `with_note`/`with_help` sugar; compose manually (we already append hints by hand). |

### Gaps / decisions this forces

1. **OSC 8 hyperlinks are gone.** annotate-snippets has no terminal-hyperlink
   support (open upstream issue #366, PR #386 in progress, *not* shipped in
   0.12.16). Our entire `wrap_path_with_hyperlink` +
   `extend_hyperlink_to_include_line_column` machinery would have **no renderer
   hook to attach to** — annotate-snippets owns the path line and emits it as
   plain text. Options: (a) drop terminal hyperlinks (simplest; aesthetic goal
   dominates); (b) post-process the rendered output to inject OSC 8 around the
   `--> path:line:col` header (brittle, similar in spirit to today's
   post-processing but against a new format); (c) wait for upstream #366. **This
   is the biggest functional regression and needs a product decision.**
2. **Per-label color loss.** `DetailKind::{Error,Info,Note}` currently each get a
   distinct `Color`. Under annotate-snippets every annotation in a snippet is
   either Primary or Context; you can't give three sibling details three
   different fixed colors. We must decide whether detail-kind color
   differentiation matters, or accept Primary/Context only. (rustc itself only
   has primary/secondary, so the rust-style aesthetic we want doesn't use
   per-detail colors anyway — this is arguably *consistent* with the goal.)
3. **Snapshot stability is easier:** `Renderer::plain()` (no ANSI) and
   `anonymized_line_numbers(true)` make golden tests trivial.

## Migration plan (draft)

### Phase 0 — De-risk and pin current behavior
- [ ] Add snapshot tests (e.g. `insta`) capturing today's ariadne output for a
      representative set: single-line span, multi-line span (exercising the
      elision/padding hack), faded detail, multiple details, with/without code,
      each `DiagnosticKind`, hyperlinks on and off. These become the before/after
      reference.
- [ ] Confirm the full downstream consumer list for the `rendered` field
      (`wasm-quarto-hub-client`, `quarto-preview`, `q2-preview`) and whether any
      consumer parses the snippet rather than displaying it verbatim.

### Phase 1 — Swap the renderer
- [ ] Add `annotate-snippets` dependency; remove `ariadne`.
- [ ] Rewrite `render_ariadne_source_context` against the annotate-snippets API
      (byte-indexed spans, level/color mapping, title + code line, main label +
      per-detail labels, footer for hints if desired).
- [ ] Decide the fate of the multi-line **padding/elision hack** and the
      `with_order` workaround — both are ariadne-specific and likely become
      unnecessary or change shape.
- [ ] Re-map `DetailKind::Faded` styling (the `Color::Fixed(249)` mirror) to the
      annotate-snippets equivalent, or drop it if the new renderer handles
      unlabelled/dim text differently.
- [ ] Rework OSC 8 hyperlink wrapping + post-processing against the new origin/
      header format (or drop terminal hyperlinks if annotate-snippets makes them
      impractical — open question).
- [ ] Re-check the `has_ariadne` branch in `to_text_with_options`: what does the
      new renderer draw (title? code? problem?) vs. what we still append by hand.

### Phase 2 — Tests, schema, docs
- [ ] Update the `rendered` glyph assertion in `src/json.rs`.
- [ ] Update/refresh snapshots; review the visual diff deliberately.
- [ ] Update the `rendered` field doc comment (drop "ariadne"); regenerate
      schemas (`QUARTO_REGEN_SCHEMAS=1`).
- [ ] Rename internal helpers (`render_ariadne_source_context`, `has_ariadne`,
      `ARIADNE_UNIMPORTANT_COLOR`) to renderer-neutral names.
- [ ] Update `Cargo.toml` description/keywords, `README.md`, `src/lib.rs` docs,
      example comments.

### Phase 3 — Release & downstream migration
- [ ] Cut a new minor/major version. Because the **rendered text changes**, this
      is at least a visually-breaking change even though the API/type surface is
      stable; decide on semver treatment.
- [ ] Write a downstream migration note: the `rendered` field's *appearance*
      changes (rust-style instead of ariadne-style), its *type and contract are
      unchanged*. Consumers that display it verbatim need no code change; any
      consumer that pattern-matched ariadne glyphs (none known) must adapt. Pin
      the new expected look with a sample.

## Open questions

**Resolved by the API research:**
- ~~Byte vs char indexing~~ → annotate-snippets spans are **byte-indexed by
  default**; direct port, no `IndexType` needed.
- ~~Mid-span elision / padding hack~~ → folding exists (`fold(true)` default) but
  is controllable: `Snippet::fold(false)` or per-line `AnnotationKind::Visible`.
  **The padding-label hack can be deleted.**
- ~~Error code in the title line~~ → yes, `Title::id("Q-2-5")` renders the native
  `error[Q-2-5]: …`. We can drop our manual `[CODE]` prefixing.

**Still open — need a decision:**
- **Terminal hyperlinks (OSC 8):** annotate-snippets does **not** support them
  (upstream issue #366, unshipped). Drop them, post-process the rendered text to
  reinject, or wait upstream? Leaning *drop* given the purely-aesthetic goal, but
  this is a product call — some terminals/editors rely on the clickable paths.
- **Per-detail color:** annotate-snippets is primary/context only; we lose the
  distinct `DetailKind` colors and the `Color::Fixed(249)` faded blend. Accept
  the rust-style two-role model (consistent with the aesthetic goal), or push
  back? Leaning *accept*.
- **Semver:** the `rendered` *appearance* changes while its type/contract do not.
  Treat as a major bump anyway (visual break for downstream UIs), or a minor with
  a loud changelog note? Recommend a **major** bump given the deliberate visual
  break and the metadata churn.
