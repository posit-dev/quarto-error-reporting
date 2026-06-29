//! Render the same diagnostic with each available source-context renderer.
//!
//! Build with both renderers to compare side by side:
//!
//! ```sh
//! cargo run --example renderer_selection --features annotate-snippets
//! ```
//!
//! `ariadne` is on by default; `annotate-snippets` is opt-in. The
//! structured data (title, code, problem, location) is identical — only
//! the source-excerpt block differs.

use quarto_error_reporting::{DiagnosticMessageBuilder, TextRenderOptions};
#[cfg(any(feature = "ariadne", feature = "annotate-snippets"))]
use quarto_error_reporting::SourceRenderer;
use quarto_source_map::{SourceContext, SourceInfo};

fn main() {
    let mut ctx = SourceContext::new();
    let source = "title: My Document\nformat:\n  html:\n    theme: nosuchtheme\n";
    let file_id = ctx.add_file("_quarto.yml".to_string(), Some(source.to_string()));

    // Point at `nosuchtheme` on line 4 (byte offsets into `source`).
    let start = source.find("nosuchtheme").unwrap();
    let location = SourceInfo::original(file_id, start, start + "nosuchtheme".len());

    let diag = DiagnosticMessageBuilder::error("Unknown theme")
        .with_code("Q-14-1")
        .with_location(location)
        .problem("`nosuchtheme` is not a known Quarto theme")
        .add_hint("Did you mean `cosmo`, `darkly`, or `flatly`?")
        .build();

    // Disable hyperlinks so the output is path-stable in a demo.
    let opts = TextRenderOptions {
        enable_hyperlinks: false,
    };

    // `None` uses the default renderer for the enabled features.
    println!("=== default renderer (None) ===\n");
    println!("{}", diag.to_text_with_renderer(Some(&ctx), &opts, None));

    #[cfg(feature = "ariadne")]
    {
        println!("=== SourceRenderer::Ariadne ===\n");
        println!(
            "{}",
            diag.to_text_with_renderer(Some(&ctx), &opts, Some(SourceRenderer::Ariadne))
        );
    }

    #[cfg(feature = "annotate-snippets")]
    {
        println!("=== SourceRenderer::AnnotateSnippets ===\n");
        println!(
            "{}",
            diag.to_text_with_renderer(
                Some(&ctx),
                &opts,
                Some(SourceRenderer::AnnotateSnippets),
            )
        );
    }
}
