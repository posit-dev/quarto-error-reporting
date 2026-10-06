# Plain (escape-free) `rendered` in JSON diagnostics

Strand: **qe-hal9cc7b** (downstream: q2 skein bd-ckbqmupi item 2, via
bd-gnw9asuo / quarto-dev/q2#795).

## Overview

`diagnostic_to_json` (`src/json.rs:176`) fills `JsonDiagnostic.rendered`
with `diag.to_text(Some(ctx))`. That call uses `TextRenderOptions::default()`,
so the result always has SGR color codes and OSC-8 hyperlinks in it. Machine
consumers of `q2 render --json-errors` (jq, agents, CI log processors) can't
read `rendered` without an ANSI stripper, and the escapes are most of its
bytes. No API asks for a plain rendering.

Goal: add an opt-in "plain" rendering (no escape bytes of any kind) for both
renderers, and a JSON entry point that takes render options. The current
default output stays byte-identical, so hub-client and q2's
`ipynb_diagnostic_hyperlinks_real_notebook` test don't change.

## Findings from reading the code

- **ariadne: `Config::with_color(false)` is sufficient** *(corrected during
  implementation)*. At first I thought per-label colors (`Label::with_color`)
  bypassed the config, because `write.rs` draws `label.display_info.color`
  directly. But `Report::add_labels` (`ariadne-0.6.0/src/lib.rs:366`) runs
  each label's color through `config.filter_color` as the label is added. So
  the only requirement is that the config is set **before** any label is
  added, which our builder chain already does; a code comment now records
  this ordering requirement. That covers the faded `Color::Fixed(249)` mirror
  too. A mutation check (forcing `with_color(true)`) fails three of the new
  tests, so a regression here is caught.
- **annotate-snippets** only needs `Renderer::plain()` in place of
  `Renderer::styled()` (~:1445). It never emits OSC-8 (`_enable_hyperlinks`
  is ignored), so hyperlinks don't matter for it.
- **Tidyverse fallback text** (no location or no ctx) and
  `format_cross_piece_location` emit no escapes, so they need no change.
- **OSC-8 is independent of color.** It's applied to the ariadne display
  path by `wrap_path_with_hyperlink` and fixed up by
  `extend_hyperlink_to_include_line_column`, both gated on
  `enable_hyperlinks`. A "plain" preset turns off both color and hyperlinks.
- **Semver: adding a field to `TextRenderOptions` is breaking.** It's a
  plain `pub struct` with one `pub` field and no `#[non_exhaustive]`. q2
  builds it with struct literals in 11 places
  (`TextRenderOptions { enable_hyperlinks: false }`, none using
  `..Default::default()`), so a new field breaks q2's build. The strand
  expected a minor bump, but this makes it **0.4.0**, not 0.3.3.
  `CoalescedDiagnostic::to_text_with_options` already passes options
  through, so it picks up the new field for free.

## Design (proposed; to confirm)

### D1. `TextRenderOptions` gets `enable_color` and becomes `#[non_exhaustive]`

```rust
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct TextRenderOptions {
    pub enable_hyperlinks: bool,
    /// SGR color codes in the source-context snippet. Default true.
    pub enable_color: bool,
}

impl Default for TextRenderOptions { /* both true — unchanged output */ }

impl TextRenderOptions {
    /// No escape sequences of any kind: no color, no OSC-8.
    pub fn plain() -> Self;
    pub fn hyperlinks(self, on: bool) -> Self;
    pub fn color(self, on: bool) -> Self;
}
```

`#[non_exhaustive]` makes this the last breaking change to the struct:
future knobs (charset, width, …) become additive. The fields stay `pub` for
reading. q2 migrates its 11 sites mechanically:
`TextRenderOptions { enable_hyperlinks: false }` →
`TextRenderOptions::default().hyperlinks(false)`.

*Alternative considered:* a non-breaking side channel (e.g. a new
`to_text_plain`, or a separate color parameter). I rejected it because it
spreads the options across signatures, and the next knob would break things
anyway.

### D2. Thread `enable_color` through the renderers

`render_source_context` currently takes `enable_hyperlinks: bool`. Change it
(and both renderer fns) to take `&TextRenderOptions`, so the next knob doesn't
need another bool. In the ariadne path:
`Config::default().with_index_type(Byte).with_color(opts.enable_color)`. (A
per-label helper was planned here too; it turned out to be unnecessary. See
the corrected finding above.) In annotate-snippets: `if opts.enable_color
{ Renderer::styled() } else { Renderer::plain() }`.

### D3. JSON entry point

```rust
pub fn diagnostic_to_json_with_options(
    diag: &DiagnosticMessage,
    ctx: &SourceContext,
    options: &TextRenderOptions,
) -> JsonDiagnostic;
```

`diagnostic_to_json(d, c)` becomes
`diagnostic_to_json_with_options(d, c, &TextRenderOptions::default())`, which
keeps today's behavior. Re-export it from `lib.rs`. The options are used only
for `rendered`. I'm reusing `TextRenderOptions` rather than adding a
`JsonRenderOptions` wrapper, because there are no JSON-specific knobs yet.
Now that the struct is non-exhaustive, a wrapper can be added later without
breaking anything if one turns up.

### D4. Schema text

Rewrite the `rendered` doc comment (`src/json.rs:97`). It currently says
"ariadne" and "ANSI-coded; strip on the JS side". The new text names the
source-context renderer and describes both modes: the default
(`diagnostic_to_json`) has ANSI color and OSC-8 hyperlinks, and the plain
variant (`diagnostic_to_json_with_options` with `TextRenderOptions::plain()`)
has no escape bytes. Then regenerate both schemas with
`QUARTO_REGEN_SCHEMAS=1 cargo test --all-features --test schema_drift`.

## Checklist

- [x] Before any code change: capture golden default-mode outputs (ariadne +
      annotate-snippets; located, multi-detail, faded, cross-piece, real-file
      hyperlinked, notebook-origin cases) into the scratchpad, to check
      byte-identity afterwards. Harness: uncommitted `tests/zz_golden_tmp.rs`
      (GOLDEN_DIR / GOLDEN_MODE=capture|compare), captured for the
      `json`, `json,annotate-snippets` and annotate-snippets-only feature
      sets (36/48/36 outputs). **Delete the harness before committing.**
- [x] D1: `enable_color` field, `#[non_exhaustive]`, `plain()` /
      `hyperlinks()` / `color()`; update rustdoc examples and
      in-crate struct literals (diagnostic.rs and coalesce.rs tests, docs)
      and examples (`custom_rendering.rs` gains a plain-rendering example)
- [x] D2: pass `&TextRenderOptions` through `render_source_context` and both
      renderers; `Config::with_color` (labels filtered by ariadne at `add_label`);
      `Renderer::plain()` when off. Golden compare after D1+D2: 120/120
      byte-identical across the three feature sets
- [x] D3: `diagnostic_to_json_with_options`; `diagnostic_to_json` delegates;
      re-export in `lib.rs`; update module docs' public-surface list
- [x] D4: `rendered` doc comment; regenerate both schema files
- [x] Tests (assert on the raw `String`, `!s.contains('\u{1b}')`, never on
      serialized JSON). Renderer tests are in `diagnostic.rs` ("Plain
      rendering" section), driven by `escape_fixtures()` (every detail kind
      incl. faded, real on-disk file, notebook origin + foreign-piece detail,
      cross-piece span) × `available_renderers()`:
  - [x] plain: no `\x1b` for every fixture × renderer (ariadne and
        annotate-snippets), and the snippet renderer must have run
        (`plain_rendering_contains_no_escape_bytes`)
  - [x] plain == colored output with SGR stripped, byte for byte: same layout
        (`plain_rendering_matches_colored_rendering_stripped`)
  - [x] `color(false)` alone on real-file / origin fixtures: OSC-8 present,
        no SGR (`color_off_keeps_hyperlinks`); annotate-snippets with
        `color(false)` has no escapes (it never emits OSC-8)
  - [x] default mode still has SGR and OSC-8 on a real file
        (`default_rendering_keeps_color_and_hyperlinks`); goldens
        byte-identical
  - [x] mutation checks: forcing ariadne `with_color(true)` or
        annotate-snippets `Renderer::styled()` each fails 3 tests
  - [x] `diagnostic_to_json_with_options(.., &plain())`: `rendered` has no
        `\x1b` (raw) and the wire form has no `\u001b`, for in-memory,
        notebook-cell and real-file fixtures; default drops nothing (SGR, and
        OSC-8 on a real file); all other fields are equal to
        `diagnostic_to_json`'s; `diagnostic_to_json` == default options (json.rs)
- [x] Filed qe-9lmo4y8u: tests already failing on main under feature sets
      CI doesn't run (`--no-default-features`; annotate-snippets-only
      `json` test assumes ariadne)
- [x] README: mention plain rendering / the JSON options entry point if the
      README covers rendering options
- [x] Bump version to 0.4.0 (breaking: `TextRenderOptions` field +
      non_exhaustive); `Cargo.lock`
- [x] `cargo fmt --all`; `cargo xtask verify` (all 6 checks green); final
      golden compare 120/120 byte-identical; temporary harness deleted
- [ ] PR; after the release, comment on qe-hal9cc7b with the q2 migration
      note (11 struct-literal sites → builder) for bd-ckbqmupi

## Decisions (2026-10-06)

1. **0.4.0.** A clean break, as in D1.
2. **Version bump goes in this PR**, as with 0.3.1 and 0.3.2.
3. **Bare builder names: `color(bool)` / `hyperlinks(bool)`.** This follows
   the `OpenOptions::read(true)` / `thread::Builder::name(..)` convention for
   setters that take `self`, and avoids confusion with ariadne's
   `Label::with_color` / `Config::with_color` in the renderer code.
