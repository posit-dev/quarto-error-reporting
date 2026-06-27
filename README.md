# quarto-error-reporting

Structured, source-aware diagnostics with a **pluggable error-code catalog**.

This crate produces compiler-quality diagnostics — a title, a problem statement,
detail bullets, hints, and an optional source span rendered with
[ariadne](https://crates.io/crates/ariadne) — and lets the *embedding product*
decide how error codes map to titles and documentation URLs. It originated as
Quarto's diagnostics layer but carries **no Quarto-specific data**: the `Q-*`
catalog lives in a separate crate that installs itself at startup.

## Core types

- [`DiagnosticMessage`] — a diagnostic (kind, title, problem, details, hints,
  optional code and source location). Renders to ANSI text (`to_text`) or JSON.
- [`DiagnosticMessageBuilder`] — a tidyverse-style builder
  (`.problem()`, `.add_detail()`, `.add_hint()`, `.with_code()`, `.with_location()`).
- [`CatalogProvider`] — the seam an embedder implements to resolve a code to
  [`ErrorCodeInfo`] (title, message template, docs URL, subsystem). Install one
  process-wide with [`install_catalog`]; with none installed, lookups return
  `None` ([`EmptyCatalog`]).

## Example

```rust
use quarto_error_reporting::DiagnosticMessageBuilder;

let diag = DiagnosticMessageBuilder::error("Unclosed code block")
    .with_code("E-001")
    .problem("A code block was opened but never closed")
    .add_hint("Did you forget the closing fence?")
    .build();

println!("{}", diag.to_text(None));
```

## The pluggable catalog

Codes are just strings to this crate. To attach titles and docs URLs, implement
`CatalogProvider` and install it once at startup:

```rust
use quarto_error_reporting::{CatalogProvider, ErrorCodeInfo, install_catalog, get_docs_url};

struct MyCatalog;
impl CatalogProvider for MyCatalog {
    fn lookup(&self, code: &str) -> Option<&ErrorCodeInfo> {
        // typically a lookup into a HashMap built from your own catalog data
        None
    }
}

install_catalog(Box::new(MyCatalog));
let _ = get_docs_url("E-001"); // resolved via the installed catalog
```

This is the host side of a cross-package error-code discipline: a *library*
mints namespaced codes it owns; the *product* embedding it remaps those to its
own user-facing codes and supplies the catalog. A consumer that installs nothing
gets code-only diagnostics, which is a valid, leaner mode.

## Features

- `json` *(off by default)* — enables the `JsonDiagnostic` machine-readable wire
  shape and its `schemars`-generated JSON Schema. Leave it off if you only need
  the diagnostic/builder/text-render API; enable it for tooling that consumes
  diagnostics over a wire (editors, language servers, web UIs).

## License

MIT © Posit Software, PBC

[`DiagnosticMessage`]: https://docs.rs/quarto-error-reporting/latest/quarto_error_reporting/struct.DiagnosticMessage.html
[`DiagnosticMessageBuilder`]: https://docs.rs/quarto-error-reporting/latest/quarto_error_reporting/struct.DiagnosticMessageBuilder.html
[`CatalogProvider`]: https://docs.rs/quarto-error-reporting/latest/quarto_error_reporting/catalog/trait.CatalogProvider.html
[`ErrorCodeInfo`]: https://docs.rs/quarto-error-reporting/latest/quarto_error_reporting/catalog/struct.ErrorCodeInfo.html
[`install_catalog`]: https://docs.rs/quarto-error-reporting/latest/quarto_error_reporting/catalog/fn.install_catalog.html
[`EmptyCatalog`]: https://docs.rs/quarto-error-reporting/latest/quarto_error_reporting/catalog/struct.EmptyCatalog.html
