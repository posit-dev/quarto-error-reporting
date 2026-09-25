//! Core diagnostic message types.
//!
//! This module defines the fundamental structures for representing diagnostic messages
//! (errors, warnings, info) following tidyverse-style guidelines.

use serde::{Deserialize, Serialize};

/// The kind of diagnostic message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticKind {
    /// An error that prevents completion
    Error,
    /// A warning that doesn't prevent completion but indicates a problem
    Warning,
    /// Informational message
    Info,
    /// A note providing additional context
    Note,
}

/// How detail items should be presented (tidyverse x/i bullet style).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DetailKind {
    /// Error detail (✖ bullet in tidyverse style)
    Error,
    /// Info detail (i bullet in tidyverse style)
    Info,
    /// Note detail (plain bullet)
    Note,
    /// Faded detail — rendered in Ariadne with the same dim grey colour
    /// Ariadne uses for source characters outside any label. Use it to
    /// attach a high-priority label to a column range you want to
    /// *exclude* from a wider label's highlighting (e.g. a block-quote
    /// prefix inside a multi-line span). Treated the same as `Note` in
    /// tidyverse-style text output.
    Faded,
}

/// Options for rendering diagnostic messages to text.
///
/// This struct controls various aspects of text rendering, such as whether
/// to include terminal hyperlinks for clickable file paths.
#[derive(Debug, Clone)]
pub struct TextRenderOptions {
    /// Enable OSC 8 hyperlinks for clickable file paths in terminals.
    ///
    /// When enabled, file paths in error messages will include terminal
    /// escape codes for clickable links (supported by iTerm2, VS Code, etc.).
    /// Disable for snapshot testing to avoid absolute path differences.
    pub enable_hyperlinks: bool,
}

impl Default for TextRenderOptions {
    fn default() -> Self {
        Self {
            enable_hyperlinks: true,
        }
    }
}

/// Selects which source-context snippet renderer draws the visual code
/// excerpt in [`DiagnosticMessage::to_text_with_renderer`].
///
/// The available variants depend on which renderer features are enabled
/// at compile time, so this enum is `#[non_exhaustive]`: with neither
/// `ariadne` nor `annotate-snippets` enabled it has no variants at all,
/// and downstream `match`es must include a wildcard arm to stay
/// compiling across feature combinations.
///
/// Pass `None` to [`DiagnosticMessage::to_text_with_renderer`] (or use
/// [`DiagnosticMessage::to_text`] / [`DiagnosticMessage::to_text_with_options`])
/// to let the crate pick a default via [`SourceRenderer::default_for_features`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SourceRenderer {
    /// [ariadne](https://crates.io/crates/ariadne)-style rendering: a
    /// boxed source excerpt. Available with the `ariadne` feature (on by
    /// default).
    #[cfg(feature = "ariadne")]
    Ariadne,
    /// [annotate-snippets](https://crates.io/crates/annotate-snippets)-style
    /// rendering: the rust-lang toolchain's `-->` / gutter-bar look.
    /// Available with the `annotate-snippets` feature.
    #[cfg(feature = "annotate-snippets")]
    AnnotateSnippets,
}

impl SourceRenderer {
    /// The renderer used when the caller does not specify one.
    ///
    /// Prefers [`SourceRenderer::Ariadne`] when the `ariadne` feature is
    /// enabled (preserving historical behavior), then falls back to
    /// [`SourceRenderer::AnnotateSnippets`]. Returns `None` when no
    /// renderer feature is enabled, in which case `to_text` drops the
    /// source-context snippet and prints the structured text block.
    pub fn default_for_features() -> Option<Self> {
        // Exactly one of these `#[cfg]` blocks survives in any feature
        // configuration, so the surviving block is the function's tail
        // expression — no `return` and no unreachable code.
        #[cfg(feature = "ariadne")]
        {
            Some(Self::Ariadne)
        }
        #[cfg(all(not(feature = "ariadne"), feature = "annotate-snippets"))]
        {
            Some(Self::AnnotateSnippets)
        }
        #[cfg(all(not(feature = "ariadne"), not(feature = "annotate-snippets")))]
        {
            None
        }
    }
}

/// The content of a message or detail item.
///
/// This will eventually support Pandoc AST for rich formatting, but starts
/// with simpler string-based content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageContent {
    /// Plain text content
    Plain(String),
    /// Markdown content (will be parsed to Pandoc AST in later phases)
    Markdown(String),
    // Future: PandocAst(Box<Inlines>)
}

impl MessageContent {
    /// Get the raw string content for display
    pub fn as_str(&self) -> &str {
        match self {
            MessageContent::Plain(s) => s,
            MessageContent::Markdown(s) => s,
        }
    }

    /// Convert to JSON value with type information
    pub fn to_json(&self) -> serde_json::Value {
        use serde_json::json;
        match self {
            MessageContent::Plain(s) => json!({
                "type": "plain",
                "content": s
            }),
            MessageContent::Markdown(s) => json!({
                "type": "markdown",
                "content": s
            }),
        }
    }
}

impl From<String> for MessageContent {
    fn from(s: String) -> Self {
        MessageContent::Markdown(s)
    }
}

impl From<&str> for MessageContent {
    fn from(s: &str) -> Self {
        MessageContent::Markdown(s.to_string())
    }
}

/// A detail item in a diagnostic message.
///
/// Following tidyverse guidelines, details provide specific information about
/// the error (what went wrong, where, with what values).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DetailItem {
    /// The kind of detail (error, info, note)
    pub kind: DetailKind,
    /// The content of the detail
    pub content: MessageContent,
    /// Optional source location for this detail
    ///
    /// When present, this identifies where in the source code this detail applies.
    /// This allows error messages to highlight multiple related locations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<quarto_source_map::SourceInfo>,
}

/// A diagnostic message following tidyverse-style structure.
///
/// Structure:
/// 1. **Code**: Optional error code (e.g., "Q-1-1") for searchability
/// 2. **Title**: Brief error message
/// 3. **Kind**: Error, Warning, Info
/// 4. **Problem**: What went wrong (the "must" or "can't" statement)
/// 5. **Details**: Specific information (bulleted, max 5 per tidyverse)
/// 6. **Hints**: Optional guidance for fixing (ends with ?)
///
/// # Example
///
/// ```ignore
/// let msg = DiagnosticMessage {
///     code: Some("Q-1-2".to_string()), // quarto-error-code-audit-ignore
///     title: "Incompatible types".to_string(),
///     kind: DiagnosticKind::Error,
///     problem: Some("Cannot combine date and datetime types".into()),
///     details: vec![
///         DetailItem {
///             kind: DetailKind::Error,
///             content: "`x`{.arg} has type `date`{.type}".into(),
///         },
///         DetailItem {
///             kind: DetailKind::Error,
///             content: "`y`{.arg} has type `datetime`{.type}".into(),
///         },
///     ],
///     hints: vec!["Convert both to the same type?".into()],
///     source_spans: vec![],
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiagnosticMessage {
    /// Optional error code (e.g., "Q-1-1")
    ///
    /// Error codes are optional but encouraged. They provide:
    /// - Searchability (users can Google "Q-1-1")
    /// - Stability (codes don't change even if message wording improves)
    /// - Documentation (each code maps to a detailed explanation)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,

    /// Brief title for the error
    pub title: String,

    /// The kind of diagnostic (Error, Warning, Info)
    pub kind: DiagnosticKind,

    /// The problem statement (the "what" - using "must" or "can't")
    pub problem: Option<MessageContent>,

    /// Specific error details (the "where/why" - max 5 per tidyverse)
    pub details: Vec<DetailItem>,

    /// Optional hints for fixing (ends with ?)
    pub hints: Vec<MessageContent>,

    /// Source location for this diagnostic
    ///
    /// When present, this identifies where in the source code the issue occurred.
    /// The location may track transformation history, allowing the error to be
    /// mapped back through multiple processing steps to the original source file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<quarto_source_map::SourceInfo>,
}

/// A minimal multi-source [`ariadne::Cache`] over in-memory files, keyed
/// by display path. The single-file `(id, source)` tuple used previously
/// cannot serve labels rooted in another `Concat` piece, which need one
/// source section per file.
#[cfg(feature = "ariadne")]
struct ContextSourceCache {
    files: Vec<(String, ariadne::Source<String>)>,
}

#[cfg(feature = "ariadne")]
impl ariadne::Cache<String> for ContextSourceCache {
    type Storage = String;

    fn fetch(&mut self, id: &String) -> Result<&ariadne::Source<String>, impl std::fmt::Debug> {
        self.files
            .iter()
            .find(|(path, _)| path == id)
            .map(|(_, source)| source)
            .ok_or(MissingSource)
    }

    fn display<'a>(&self, id: &'a String) -> Option<impl std::fmt::Display + 'a> {
        Some(id)
    }
}

/// Fetch-error type for [`ContextSourceCache`]; ariadne only formats it
/// into an eprintln for sources no label references.
#[cfg(feature = "ariadne")]
struct MissingSource;

#[cfg(feature = "ariadne")]
impl std::fmt::Debug for MissingSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("source not registered in the diagnostic's source context")
    }
}

impl DiagnosticMessage {
    /// Access the diagnostic message builder API.
    ///
    /// This is the recommended way to create diagnostic messages, as the builder API
    /// encodes tidyverse-style guidelines and makes it easy to construct well-structured
    /// error messages.
    ///
    /// # Example
    ///
    /// ```
    /// use quarto_error_reporting::{DiagnosticMessage, DiagnosticMessageBuilder};
    ///
    /// let error = DiagnosticMessageBuilder::error("Incompatible types")
    ///     .with_code("Q-1-2") // quarto-error-code-audit-ignore
    ///     .problem("Cannot combine date and datetime types")
    ///     .add_detail("`x` has type `date`")
    ///     .add_detail("`y` has type `datetime`")
    ///     .add_hint("Convert both to the same type?")
    ///     .build();
    /// ```
    pub fn builder() -> crate::builder::DiagnosticMessageBuilder {
        // This is just a convenience for accessing the builder type
        // Users should call DiagnosticMessageBuilder::error() etc directly
        crate::builder::DiagnosticMessageBuilder::error("")
    }

    /// Create a new diagnostic message with just a title and kind.
    ///
    /// Note: Consider using `DiagnosticMessage::builder()` instead for better structure.
    pub fn new(kind: DiagnosticKind, title: impl Into<String>) -> Self {
        Self {
            code: None,
            title: title.into(),
            kind,
            problem: None,
            details: Vec::new(),
            hints: Vec::new(),
            location: None,
        }
    }

    /// Create an error diagnostic.
    ///
    /// Note: Consider using `DiagnosticMessage::builder().error()` instead for better structure.
    pub fn error(title: impl Into<String>) -> Self {
        Self::new(DiagnosticKind::Error, title)
    }

    /// Create a warning diagnostic.
    ///
    /// Note: Consider using `DiagnosticMessage::builder().warning()` instead for better structure.
    pub fn warning(title: impl Into<String>) -> Self {
        Self::new(DiagnosticKind::Warning, title)
    }

    /// Create an info diagnostic.
    ///
    /// Note: Consider using `DiagnosticMessage::builder().info()` instead for better structure.
    pub fn info(title: impl Into<String>) -> Self {
        Self::new(DiagnosticKind::Info, title)
    }

    /// Set the error code.
    ///
    /// Error codes follow the format `Q-<subsystem>-<number>` (e.g., "Q-1-1").
    ///
    /// # Example
    ///
    /// ```
    /// use quarto_error_reporting::DiagnosticMessage;
    ///
    /// let msg = DiagnosticMessage::error("YAML Syntax Error")
    ///     .with_code("Q-1-1");
    /// ```
    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    /// Get the documentation URL for this error, if it has an error code.
    ///
    /// # Example
    ///
    /// Resolves the code against the installed [`CatalogProvider`]
    /// (`crate::catalog`); returns `None` when no catalog is installed, the
    /// code is unknown, or the entry has no docs URL.
    ///
    /// ```
    /// use quarto_error_reporting::DiagnosticMessage;
    ///
    /// let msg = DiagnosticMessage::error("Internal Error")
    ///     .with_code("Q-0-1");
    ///
    /// // `Some(url)` iff a catalog mapping "Q-0-1" (with a docs URL) is installed.
    /// let _ = msg.docs_url();
    /// ```
    pub fn docs_url(&self) -> Option<&str> {
        self.code
            .as_ref()
            .and_then(|code| crate::catalog::get_docs_url(code))
    }

    /// Render this diagnostic message as text following tidyverse style.
    ///
    /// This is a convenience method that uses default rendering options.
    /// For more control over rendering, use [`Self::to_text_with_options`].
    ///
    /// # Example
    ///
    /// ```
    /// use quarto_error_reporting::DiagnosticMessageBuilder;
    ///
    /// let msg = DiagnosticMessageBuilder::error("Invalid input")
    ///     .problem("Values must be numeric")
    ///     .add_detail("Found text in column 3")
    ///     .add_hint("Convert to numbers first?")
    ///     .build();
    /// let text = msg.to_text(None);
    /// assert!(text.contains("Error: Invalid input"));
    /// assert!(text.contains("Values must be numeric"));
    /// ```
    pub fn to_text(&self, ctx: Option<&quarto_source_map::SourceContext>) -> String {
        self.to_text_with_options(ctx, &TextRenderOptions::default())
    }

    /// Render this diagnostic message as text following tidyverse style with custom options.
    ///
    /// Format:
    /// ```text
    /// Error: title
    /// Problem statement here
    /// ✖ Error detail 1
    /// ✖ Error detail 2
    /// ℹ Info detail
    /// • Note detail
    /// ? Hint 1
    /// ? Hint 2
    /// ```
    ///
    /// # Example
    ///
    /// ```
    /// use quarto_error_reporting::{DiagnosticMessageBuilder, TextRenderOptions};
    ///
    /// let msg = DiagnosticMessageBuilder::error("Invalid input")
    ///     .problem("Values must be numeric")
    ///     .add_detail("Found text in column 3")
    ///     .add_hint("Convert to numbers first?")
    ///     .build();
    ///
    /// // Disable hyperlinks for snapshot testing
    /// let options = TextRenderOptions { enable_hyperlinks: false };
    /// let text = msg.to_text_with_options(None, &options);
    /// assert!(text.contains("Error: Invalid input"));
    /// ```
    pub fn to_text_with_options(
        &self,
        ctx: Option<&quarto_source_map::SourceContext>,
        options: &TextRenderOptions,
    ) -> String {
        self.to_text_with_renderer(ctx, options, None)
    }

    /// Like [`Self::to_text_with_options`], but explicitly selects which
    /// source-context snippet renderer draws the visual code excerpt.
    ///
    /// Pass `Some(SourceRenderer::Ariadne)` or
    /// `Some(SourceRenderer::AnnotateSnippets)` to force a specific
    /// renderer (the corresponding feature must be enabled), or `None`
    /// to use [`SourceRenderer::default_for_features`]. This is the seam
    /// for experimenting with diagnostic rendering styles without
    /// changing the rest of the API: only the source-excerpt block
    /// differs between renderers; the surrounding structured text
    /// (unlocated details, hints) is identical.
    ///
    /// When no renderer feature is enabled — or the diagnostic has no
    /// location / source context — this falls back to the structured
    /// tidyverse-style text block, exactly as [`Self::to_text_with_options`].
    ///
    /// # Example
    ///
    /// ```
    /// use quarto_error_reporting::{DiagnosticMessageBuilder, TextRenderOptions};
    ///
    /// let msg = DiagnosticMessageBuilder::error("Invalid input")
    ///     .problem("Values must be numeric")
    ///     .build();
    ///
    /// // `None` picks the default renderer for the enabled features.
    /// let text = msg.to_text_with_renderer(None, &TextRenderOptions::default(), None);
    /// assert!(text.contains("Invalid input"));
    /// ```
    pub fn to_text_with_renderer(
        &self,
        ctx: Option<&quarto_source_map::SourceContext>,
        options: &TextRenderOptions,
        renderer: Option<SourceRenderer>,
    ) -> String {
        use std::fmt::Write;

        let mut result = String::new();

        // Check if we have any location info that could be displayed in a
        // source excerpt. This includes the main diagnostic location OR
        // any detail with a location.
        let has_any_location =
            self.location.is_some() || self.details.iter().any(|d| d.location.is_some());

        // If we have location info and source context, render the source
        // excerpt with the selected (or default) renderer.
        let has_source_render = if let (true, Some(ctx_val)) = (has_any_location, ctx) {
            // Use main location if available, otherwise use first detail location
            let location = self
                .location
                .as_ref()
                .or_else(|| self.details.iter().find_map(|d| d.location.as_ref()));

            if let Some(loc) = location {
                if let Some(snippet_output) =
                    self.render_source_context(loc, ctx_val, options.enable_hyperlinks, renderer)
                {
                    result.push_str(&snippet_output);
                    true
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };

        // If we don't have a source excerpt, show full tidyverse-style content.
        // If we do, only show details without locations and hints
        // (the renderer already shows: title, code, problem, and located details)
        if !has_source_render {
            // No source excerpt - show everything in tidyverse style

            // Title with kind prefix and error code (e.g., "Error [Q-1-1]: Invalid input")
            let kind_str = match self.kind {
                DiagnosticKind::Error => "Error",
                DiagnosticKind::Warning => "Warning",
                DiagnosticKind::Info => "Info",
                DiagnosticKind::Note => "Note",
            };
            if let Some(code) = &self.code {
                writeln!(result, "{} [{}]: {}", kind_str, code, self.title).unwrap();
            } else {
                writeln!(result, "{}: {}", kind_str, self.title).unwrap();
            }

            // Show location info if available (but no ariadne rendering)
            if let Some(loc) = &self.location {
                // Try to map with context if available
                if let Some(ctx) = ctx {
                    if let Some(mapped) = loc.map_offset(loc.start_offset(), ctx)
                        && let Some(file) = ctx.get_file(mapped.file_id)
                    {
                        writeln!(
                            result,
                            "  at {}:{}:{}",
                            file.path,
                            mapped.location.row + 1,
                            mapped.location.column + 1
                        )
                        .unwrap();
                    }
                } else {
                    // No context: show immediate location (1-indexed for display)
                    // Note: Without context, we can't get row/column from offsets
                    // We could map_offset with ctx to get Location, but ctx is None here
                    writeln!(result, "  at offset {}", loc.start_offset()).unwrap();
                }
            }

            // Problem statement (optional additional context)
            if let Some(problem) = &self.problem {
                writeln!(result, "{}", problem.as_str()).unwrap();
            }

            // All details with appropriate bullets
            for detail in &self.details {
                let bullet = match detail.kind {
                    DetailKind::Error => "✖",
                    DetailKind::Info => "ℹ",
                    DetailKind::Note | DetailKind::Faded => "•",
                };
                writeln!(result, "{} {}", bullet, detail.content.as_str()).unwrap();
            }

            // All hints
            for hint in &self.hints {
                writeln!(result, "ℹ {}", hint.as_str()).unwrap();
            }
        } else {
            // Have a source excerpt - only show details without locations and hints
            // (the renderer shows title, code, problem, and located details)

            // Details without locations (the source excerpt can't show these)
            for detail in &self.details {
                if detail.location.is_none() {
                    let bullet = match detail.kind {
                        DetailKind::Error => "✖",
                        DetailKind::Info => "ℹ",
                        DetailKind::Note | DetailKind::Faded => "•",
                    };
                    writeln!(result, "{} {}", bullet, detail.content.as_str()).unwrap();
                }
            }

            // All hints (ariadne doesn't show hints)
            for hint in &self.hints {
                writeln!(result, "ℹ {}", hint.as_str()).unwrap();
            }
        }

        result
    }

    /// Render this diagnostic message as a JSON value.
    ///
    /// Returns a structured JSON object with all fields:
    /// ```json
    /// {
    ///   "kind": "error",
    ///   "title": "Invalid input",
    ///   "code": "Q-1-2", // quarto-error-code-audit-ignore
    ///   "problem": "Values must be numeric",
    ///   "details": [{"kind": "error", "content": "Found text in column 3"}],
    ///   "hints": ["Convert to numbers first?"]
    /// }
    /// ```
    ///
    /// # Example
    ///
    /// ```
    /// use quarto_error_reporting::DiagnosticMessage;
    ///
    /// let msg = DiagnosticMessage::error("Something went wrong");
    /// let json = msg.to_json();
    /// assert_eq!(json["kind"], "error");
    /// assert_eq!(json["title"], "Something went wrong");
    /// ```
    pub fn to_json(&self) -> serde_json::Value {
        use serde_json::json;

        let kind_str = match self.kind {
            DiagnosticKind::Error => "error",
            DiagnosticKind::Warning => "warning",
            DiagnosticKind::Info => "info",
            DiagnosticKind::Note => "note",
        };

        let mut obj = json!({
            "kind": kind_str,
            "title": self.title,
        });

        // Add optional fields
        if let Some(code) = &self.code {
            obj["code"] = json!(code);
        }

        if let Some(problem) = &self.problem {
            obj["problem"] = problem.to_json();
        }

        if !self.details.is_empty() {
            let details: Vec<_> = self
                .details
                .iter()
                .map(|d| {
                    let detail_kind = match d.kind {
                        DetailKind::Error => "error",
                        DetailKind::Info => "info",
                        DetailKind::Note => "note",
                        DetailKind::Faded => "faded",
                    };
                    let mut detail_obj = json!({
                        "kind": detail_kind,
                        "content": d.content.to_json()
                    });
                    if let Some(location) = &d.location {
                        detail_obj["location"] = json!(location);
                    }
                    detail_obj
                })
                .collect();
            obj["details"] = json!(details);
        }

        if !self.hints.is_empty() {
            let hints: Vec<_> = self.hints.iter().map(|h| h.to_json()).collect();
            obj["hints"] = json!(hints);
        }

        if let Some(location) = &self.location {
            obj["location"] = json!(location); // quarto-source-map::SourceInfo is Serialize
        }

        obj
    }

    /// Snap a mapped byte range onto UTF-8 character boundaries within
    /// `content`, clamping it into the file and keeping `start <= end`.
    ///
    /// # Why the renderers need this
    ///
    /// Both source-context renderers slice the source by byte offset, and
    /// both **panic** — they do not merely mis-render — on an offset that
    /// falls inside a multi-byte character. Measured 2026-08-23 against the
    /// versions this crate's lockfile resolves (ariadne 0.6.0,
    /// annotate-snippets 0.12.16; the manifest declares only `0.6` and
    /// `0.12`), rendering `"text: <span>Ask AI \u{2728}</span>"` in which
    /// `\u{2728}` occupies bytes 19..22 and the offset each row names is
    /// placed at 20 or 21 — both interior to that character:
    ///
    /// | offset placed mid-character | ariadne | annotate-snippets |
    /// |---|---|---|
    /// | label start   | panics, `write.rs:84`  | panics, `renderer/source_map.rs:71` |
    /// | label end     | panics, `write.rs:102` | panics, `renderer/source_map.rs:98` |
    /// | report anchor | panics, `write.rs:267` | n/a — this crate passes no separate anchor |
    ///
    /// The clamping half guards two further aborts that boundary-snapping
    /// alone would not catch, measured the same way:
    ///
    /// | malformed range | ariadne | annotate-snippets |
    /// |---|---|---|
    /// | end past EOF          | tolerates (degraded excerpt) | panics, `renderer/source_map.rs:158` |
    /// | inverted, `end < start` | panics, `lib.rs:145` (a plain `assert!`, so also in release) | `renderer/render.rs:1394` subtracts unchecked: panics only where overflow checks are on (debug/test), wraps silently in a default release build |
    ///
    /// Printing a diagnostic must never be able to kill a render, so we
    /// normalize here rather than trusting the input.
    ///
    /// # What is actually load-bearing, and when
    ///
    /// **How much the snapping half is doing depends on what
    /// `quarto-source-map` resolves to.** Since 0.1.2,
    /// `FileInformation::offset_to_location` returns the *floored* offset
    /// (`src/file_info.rs:116-125`:
    /// `safe_offset` walks left onto a character boundary and is returned as
    /// `Location.offset`; 0.1.0 and 0.1.1 computed `safe_offset` but returned
    /// the raw `offset`). **This crate's declared floor is still
    /// `quarto-source-map = "0.1.0"` (`Cargo.toml:28`)**, so a consumer that
    /// resolves 0.1.0 or 0.1.1 — an existing lock, another graph member
    /// pinning `=0.1.1`, `-Z minimal-versions` — gets no upstream floor at
    /// all, and for that build the snap below is the only guard rather than
    /// a backstop. A published library's lockfile does not constrain its
    /// consumers; downstream, q2 had to raise its own floor, because this
    /// manifest does not force it.
    ///
    /// On a resolution that *does* have the floor, it covers the mapping
    /// path broadly: every value-producing path of `SourceInfo::map_offset`
    /// runs through `offset_to_location` (`src/mapping.rs:38`; `Generated`
    /// yields `None` instead), and all three call sites here are fed
    /// `map_offset` results — the span shared by the report anchor and the
    /// main label, and the detail-label spans, both in
    /// `render_ariadne_source_context`; and the `clamp` closure in
    /// `render_annotate_snippets_source_context`.
    ///
    /// So a mid-character offset can no longer reach a renderer through the
    /// mapping path **when the span resolves within a single file**. The
    /// cross-file `Concat` shape described below is an exception for the
    /// snapping half as well as the clamping half: `map_offset` floors an
    /// offset against the piece's *own* file content (`src/mapping.rs:25-38`),
    /// which says nothing about where character boundaries fall in
    /// `content`.
    ///
    /// The snap is therefore kept deliberately. It is the only guard on a
    /// pre-0.1.2 resolution, a live guard on the cross-file shape, and the
    /// backstop if the upstream floor regresses or a future caller hands us
    /// raw, unmapped offsets.
    ///
    /// **The clamping half is still live.** `end_mapped` is not always the
    /// mapped image of this span's end: when `map_offset(length())` fails,
    /// both renderer paths substitute `map_offset(length() - 1)` and then
    /// fall back to `start_mapped` (the `length() - 1` fallback at
    /// `:842-852` in `render_ariadne_source_context` and `:1038-1047` in
    /// `render_annotate_snippets_source_context`, line numbers as of 0.2.2).
    /// And `content` belongs to `root_file_id()`, which for a `Concat` is
    /// the file of the *first* piece that resolves to one
    /// (`quarto-source-map`'s `src/source_info.rs:549-560`), while
    /// `map_offset` resolves into whichever piece contains the offset.
    /// Structurally, then — a `Concat` spanning two files — the two ends
    /// can resolve into different files, whose offsets are neither ordered
    /// with respect to each other nor bounded by `content.len()`. (That
    /// shape is permitted by the types; unlike the panics tabled above it
    /// has not been exercised here.) `start.min(len)`, `end.min(len)` and
    /// `.max(s)` reduce any of that to an in-range, non-inverted span.
    ///
    /// # Behaviour
    ///
    /// The range is widened, not truncated: `start` floors to the start of
    /// the character containing it and `end` ceils to the end of the
    /// character containing it, so the highlight covers whole characters
    /// and can never invert. This differs from the upstream floor, which
    /// walks an end offset *left* rather than widening it; the two agree
    /// whenever only the start is misaligned.
    ///
    /// # Coverage
    ///
    /// The direct unit coverage is `snap_span_widens_to_whole_characters`.
    /// The two `..._renders_diagnostic_with_originally_mid_character_span`
    /// tests are end-to-end smoke checks only, and do not bind to this
    /// helper: commit `5e48166`, *"Re-anchor the mid-character-span crash
    /// tests after the 0.1.3 floor"*, re-anchored them and records why. (It
    /// is a commit of PR #5, which was squash-merged as `87f1d38`, so it is
    /// reachable through that PR rather than from `main`'s history.)
    ///
    /// Downstream, the quarto-dev/q2 repository is adding an end-to-end pin
    /// for the founding crash, under `crates/quarto/tests/integration/`: it
    /// drives the real `q2` binary over a website project whose
    /// `_quarto.yml` navbar entry embeds `\u{2728}` **mid-string**, as
    /// `text: '<span id="x">Ask AI \u{2728}</span>'`, and asserts a clean
    /// exit alongside the expected caret columns. The trailing `</span>` is
    /// load-bearing — one asserted caret falls past it — so it is the same
    /// shape as this crate's own tests, where `\u{2728}` sits at bytes
    /// 19..22 and `</span>` at 22..29. That pin will redden only if q2's
    /// mapping regresses *and* both this snap and the upstream floor are
    /// gone: it guards the combination, not this helper on its own.
    #[cfg(any(feature = "ariadne", feature = "annotate-snippets"))]
    fn snap_span_to_char_boundaries(
        content: &str,
        start: usize,
        end: usize,
    ) -> std::ops::Range<usize> {
        let len = content.len();
        let mut s = start.min(len);
        let mut e = end.min(len).max(s);
        while s > 0 && !content.is_char_boundary(s) {
            s -= 1;
        }
        while e < len && !content.is_char_boundary(e) {
            e += 1;
        }
        s..e
    }

    /// Dispatch to the selected source-context renderer.
    ///
    /// `renderer` of `None` resolves to [`SourceRenderer::default_for_features`].
    /// Returns `None` when no renderer is available (no renderer feature
    /// enabled) or the chosen renderer could not draw the excerpt (e.g.
    /// the file content is unavailable — common in WASM), in which case
    /// the caller falls back to the structured text block.
    #[cfg_attr(
        not(any(feature = "ariadne", feature = "annotate-snippets")),
        allow(unused_variables)
    )]
    fn render_source_context(
        &self,
        main_location: &quarto_source_map::SourceInfo,
        ctx: &quarto_source_map::SourceContext,
        enable_hyperlinks: bool,
        renderer: Option<SourceRenderer>,
    ) -> Option<String> {
        let renderer = renderer.or_else(SourceRenderer::default_for_features)?;
        match renderer {
            #[cfg(feature = "ariadne")]
            SourceRenderer::Ariadne => {
                self.render_ariadne_source_context(main_location, ctx, enable_hyperlinks)
            }
            #[cfg(feature = "annotate-snippets")]
            SourceRenderer::AnnotateSnippets => {
                self.render_annotate_snippets_source_context(main_location, ctx, enable_hyperlinks)
            }
        }
    }

    /// Wrap a file path with OSC 8 ANSI hyperlink codes for clickable terminal links.
    ///
    /// OSC 8 is a terminal escape sequence that creates clickable hyperlinks:
    /// `\x1b]8;;URI\x1b\\TEXT\x1b\\`
    ///
    /// `path` is always the *displayed* text (for a virtual file, the
    /// cell-qualified label). `link_target` is the disk path the hyperlink
    /// opens — normally `path` itself when it exists on disk, or the owning
    /// notebook for a virtual file that carries a `FileOrigin`. `None`
    /// disables the link.
    ///
    /// A link is emitted only if:
    /// - Hyperlinks are enabled via the `enable_hyperlinks` parameter
    /// - `link_target` is `Some` and can be canonicalized
    ///
    /// The `url` crate handles:
    /// - Platform differences (Windows drive letters vs Unix paths)
    /// - Percent-encoding of special characters
    /// - Proper file:// URL construction
    ///
    /// Line and column numbers are added to the URL as a fragment identifier
    /// (e.g., `file:///path#line:column`), which is supported by iTerm2 3.4+
    /// and other terminal emulators for opening files at specific positions.
    /// The fragment is emitted only when the position belongs to the linked
    /// file itself (`link_target == path`); an origin link opens a different
    /// file whose coordinates we cannot speak to.
    ///
    /// Returns the wrapped path if conditions are met, otherwise returns the original path.
    ///
    /// Only used by the ariadne renderer (annotate-snippets has no OSC 8 support).
    #[cfg(all(feature = "ariadne", not(target_family = "wasm")))]
    fn wrap_path_with_hyperlink(
        path: &str,
        link_target: Option<&str>,
        line: Option<usize>,
        column: Option<usize>,
        enable_hyperlinks: bool,
    ) -> String {
        // Don't add hyperlinks if disabled (e.g., for snapshot testing)
        if !enable_hyperlinks {
            return path.to_string();
        }

        let Some(target) = link_target else {
            return path.to_string();
        };

        // Canonicalize to absolute path
        let abs_path = match std::fs::canonicalize(target) {
            Ok(p) => p,
            Err(_) => return path.to_string(), // Can't canonicalize, skip hyperlink
        };

        // Convert to file:// URL (handles Windows/Unix + percent-encoding)
        let mut file_url = match url::Url::from_file_path(&abs_path) {
            Ok(url) => url.as_str().to_string(),
            Err(_) => return path.to_string(), // Conversion failed, skip hyperlink
        };

        // Add line and column as fragment identifier (e.g., #line:column)
        // This format is supported by iTerm2 3.4+ semantic history — but
        // only when the position belongs to the linked file itself. An
        // origin link opens a *different* file (the owning notebook), and
        // the diagnostic's coordinates are relative to the virtual file.
        if target == path
            && let Some(line_num) = line
        {
            match column {
                Some(col_num) => file_url.push_str(&format!("#{}:{}", line_num, col_num)),
                None => file_url.push_str(&format!("#{}", line_num)),
            }
        }

        // Wrap with OSC 8 codes: \x1b]8;;URI\x1b\\TEXT\x1b]8;;\x1b\\
        format!("\x1b]8;;{}\x1b\\{}\x1b]8;;\x1b\\", file_url, path)
    }

    /// The disk path a file's label should hyperlink to: the owning
    /// notebook for a virtual file with a `FileOrigin`, the file itself
    /// when it exists on disk, `None` otherwise (ephemeral or missing).
    #[cfg(feature = "ariadne")]
    fn hyperlink_target(file: &quarto_source_map::SourceFile) -> Option<&str> {
        match file.metadata.origin.as_ref() {
            Some(quarto_source_map::FileOrigin::NotebookCell { notebook_path, .. }) => {
                Some(notebook_path)
            }
            None => std::path::Path::new(&file.path)
                .exists()
                .then_some(file.path.as_str()),
        }
    }

    /// WASM version: hyperlinks don't make sense in WASM environments (no file system).
    /// Just return the path unmodified.
    #[cfg(all(feature = "ariadne", target_family = "wasm"))]
    fn wrap_path_with_hyperlink(
        path: &str,
        _link_target: Option<&str>,
        _line: Option<usize>,
        _column: Option<usize>,
        _enable_hyperlinks: bool,
    ) -> String {
        path.to_string()
    }

    /// One-line textual location for a span whose two ends resolve into
    /// different `Concat` pieces, rendered in place of a snippet. A
    /// cross-piece span cannot be drawn as one excerpt, and clamping it
    /// into either piece's content would point at the wrong file.
    #[cfg(any(feature = "ariadne", feature = "annotate-snippets"))]
    fn format_cross_piece_location(
        start: &quarto_source_map::MappedLocation,
        end: &quarto_source_map::MappedLocation,
        ctx: &quarto_source_map::SourceContext,
    ) -> String {
        let name = |file_id| match ctx.get_file(file_id) {
            Some(file) => file.path.clone(),
            None => "<unknown file>".to_string(),
        };
        // Line and column numbers are 1-indexed for display (Location uses 0-indexed)
        format!(
            "  --> {}:{}:{} (spans through {}:{}:{})\n",
            name(start.file_id),
            start.location.row + 1,
            start.location.column + 1,
            name(end.file_id),
            end.location.row + 1,
            end.location.column + 1,
        )
    }

    /// Render source context using ariadne (private helper for to_text).
    ///
    /// This produces the visual source code snippet with highlighting.
    /// The tidyverse-style problem/details/hints are added separately by to_text().
    #[cfg(feature = "ariadne")]
    fn render_ariadne_source_context(
        &self,
        main_location: &quarto_source_map::SourceInfo,
        ctx: &quarto_source_map::SourceContext,
        enable_hyperlinks: bool,
    ) -> Option<String> {
        use ariadne::{Color, Config, IndexType, Label, Report, ReportKind, Source};

        // Mirror of ariadne's private `Config::unimportant_color()` from
        // ariadne 0.6.0 (`src/lib.rs:543`). We use this for `DetailKind::Faded`
        // labels so they blend visually with characters that fall outside any
        // label. Bump this constant if the ariadne dependency upgrades and
        // changes the colour.
        const ARIADNE_UNIMPORTANT_COLOR: Color = Color::Fixed(249);

        // The report's file is the one the span *starts* in. For a
        // multi-piece `Concat` (q2's per-cell ipynb virtual files),
        // `root_file_id()` is the first piece's file — wrong for a
        // diagnostic rooted in a later piece — while `map_offset`
        // resolves piece-aware. `start_mapped.file_id` is correct for
        // both shapes (and identical to `root_file_id()` on a
        // single-file source).
        let start_mapped = main_location.map_offset(0, ctx)?;
        let file_id = start_mapped.file_id;

        let file = ctx.get_file(file_id)?;

        // Get file content: use stored content for ephemeral files, or read from disk.
        // In WASM (and any host with no real filesystem) the disk read fails with
        // "operation not supported on this platform"; the only graceful response is
        // to drop the source-context snippet. The diagnostic's code, message, and
        // hints still surface — only the Ariadne visual is unavailable.
        let content = match &file.content {
            Some(c) => c.clone(),
            None => match std::fs::read_to_string(&file.path) {
                Ok(s) => s,
                Err(_) => return None,
            },
        };

        // For end offset, try the full length first. If that fails (e.g., when the span
        // extends past EOF), clamp to the last valid position. This handles edge cases
        // like errors pointing to EOF or diagnostics with off-by-one end offsets.
        let end_mapped = main_location
            .map_offset(main_location.length(), ctx)
            .or_else(|| {
                // Clamp: if length() fails, try length()-1, which should be the last valid byte
                if main_location.length() > 0 {
                    main_location.map_offset(main_location.length() - 1, ctx)
                } else {
                    None
                }
            })
            .unwrap_or_else(|| start_mapped.clone());

        // A span whose two ends resolve into different pieces cannot be
        // drawn as one snippet — rendering it against either piece's
        // content would clamp it silently into the wrong file. Label both
        // ends textually instead.
        if end_mapped.file_id != start_mapped.file_id {
            return Some(Self::format_cross_piece_location(
                &start_mapped,
                &end_mapped,
                ctx,
            ));
        }

        // Create display path with OSC 8 hyperlink for clickable file paths.
        // A virtual cell file (FileOrigin) links its owning notebook; a real
        // file links itself; an ephemeral file with no origin stays unlinked.
        let link_target = Self::hyperlink_target(file);
        // Line and column numbers are 1-indexed for display (start_mapped.location uses 0-indexed)
        let line = Some(start_mapped.location.row + 1);
        let column = Some(start_mapped.location.column + 1);
        let display_path = Self::wrap_path_with_hyperlink(
            &file.path,
            link_target,
            line,
            column,
            enable_hyperlinks,
        );

        // Determine report kind and color
        let (report_kind, main_color) = match self.kind {
            DiagnosticKind::Error => (ReportKind::Error, Color::Red),
            DiagnosticKind::Warning => (ReportKind::Warning, Color::Yellow),
            DiagnosticKind::Info => (ReportKind::Advice, Color::Cyan),
            DiagnosticKind::Note => (ReportKind::Advice, Color::Blue),
        };

        // Snap once, up front: every offset handed to ariadne below (the
        // report anchor and the main label) must be char-boundary safe.
        let main_span = Self::snap_span_to_char_boundaries(
            &content,
            start_mapped.location.offset,
            end_mapped.location.offset,
        );

        // Build the report using the mapped offset for proper line:column display
        // IMPORTANT: Use IndexType::Byte because our offsets are byte offsets, not character offsets
        let mut report = Report::build(
            report_kind,
            (display_path.clone(), main_span.start..main_span.start),
        )
        .with_config(Config::default().with_index_type(IndexType::Byte));

        // Add title with error code
        if let Some(code) = &self.code {
            report = report.with_message(format!("[{}] {}", code, self.title));
        } else {
            report = report.with_message(&self.title);
        }

        // Add main location label using the snapped span computed above.
        let main_message = if let Some(problem) = &self.problem {
            problem.as_str()
        } else {
            &self.title
        };

        // Set `with_order` on every label using its end offset. Ariadne
        // groups labels by source and starts a new group whenever a label's
        // end line is *before* the previous label's end line. Without an
        // explicit order, multi-line main labels and per-line "padding"
        // detail labels (used to defeat Ariadne's middle-line elision) end
        // up in separate groups, producing a duplicated snippet block.
        // Sorting by end offset puts the smaller-line labels first so the
        // grouping algorithm extends rather than splits.
        report = report.with_label(
            Label::new((display_path.clone(), main_span.clone()))
                .with_message(main_message)
                .with_color(main_color)
                .with_order(main_span.end as i32),
        );

        // Add detail locations as additional labels (only those with locations).
        // Details rooted (start and end) in the report's file stay inline
        // labels; details rooted wholly in another piece are collected and
        // appended after the loop as labels in their own file's source
        // section. A detail whose own span straddles pieces is not
        // representable as one inline label and is skipped (the main-span
        // cross-piece label above covers the common cross-cell shape).
        let mut cache_files = vec![(display_path.clone(), Source::from(content.clone()))];
        let mut foreign_labels: Vec<(String, std::ops::Range<usize>, Option<String>, Color)> =
            Vec::new();
        for detail in &self.details {
            if let Some(detail_loc) = &detail.location {
                // Map detail offsets to original file positions
                // map_offset expects relative offsets (0 = start of SourceInfo's range)
                let (Some(detail_start), Some(detail_end)) = (
                    detail_loc.map_offset(0, ctx),
                    detail_loc.map_offset(detail_loc.length(), ctx),
                ) else {
                    continue;
                };
                let detail_color = match detail.kind {
                    DetailKind::Error => Color::Red,
                    DetailKind::Info => Color::Cyan,
                    DetailKind::Note => Color::Blue,
                    // Match Ariadne's unimportant colour so faded
                    // labels visually disappear into the surrounding
                    // unlabelled text.
                    DetailKind::Faded => ARIADNE_UNIMPORTANT_COLOR,
                };

                if detail_start.file_id == detail_end.file_id && detail_start.file_id != file_id {
                    // Wholly inside another piece: render it there as its
                    // own source section instead of silently dropping it.
                    let Some(detail_file) = ctx.get_file(detail_start.file_id) else {
                        continue;
                    };
                    let Some(detail_content) = detail_file
                        .content
                        .clone()
                        .or_else(|| std::fs::read_to_string(&detail_file.path).ok())
                    else {
                        continue;
                    };
                    // File-level id (no per-line hyperlink fragment) so
                    // several details in one foreign file share a source
                    // section; ariadne derives the header line:column from
                    // the label span itself.
                    let foreign_display = Self::wrap_path_with_hyperlink(
                        &detail_file.path,
                        Self::hyperlink_target(detail_file),
                        None,
                        None,
                        enable_hyperlinks,
                    );
                    let detail_span = Self::snap_span_to_char_boundaries(
                        &detail_content,
                        detail_start.location.offset,
                        detail_end.location.offset,
                    );
                    if !cache_files.iter().any(|(path, _)| *path == foreign_display) {
                        cache_files.push((foreign_display.clone(), Source::from(detail_content)));
                    }
                    let message = (!detail.content.as_str().is_empty())
                        .then(|| detail.content.as_str().to_string());
                    foreign_labels.push((foreign_display, detail_span, message, detail_color));
                    continue;
                }

                if detail_start.file_id == file_id && detail_end.file_id == file_id {
                    let detail_span = Self::snap_span_to_char_boundaries(
                        &content,
                        detail_start.location.offset,
                        detail_end.location.offset,
                    );
                    // Empty-content details exist purely to force Ariadne
                    // to display a line that would otherwise be elided
                    // inside a multi-line span. Leaving the label's
                    // message at None makes Ariadne skip drawing the
                    // `╰── ...` arrow row underneath, so the source line
                    // appears clean.
                    let mut label = Label::new((display_path.clone(), detail_span.clone()))
                        .with_color(detail_color)
                        .with_order(detail_span.end as i32);
                    if !detail.content.as_str().is_empty() {
                        label = label.with_message(detail.content.as_str());
                    }
                    report = report.with_label(label);
                }
            }
        }

        // Foreign-piece labels sort after every same-file label so
        // ariadne's order-keyed grouping draws the report's file first,
        // intact; no realistic same-file span end reaches this base.
        for (foreign_order, (path, span, message, color)) in (1_000_000i32..).zip(foreign_labels) {
            let mut label = Label::new((path, span))
                .with_color(color)
                .with_order(foreign_order);
            if let Some(message) = message {
                label = label.with_message(message);
            }
            report = report.with_label(label);
        }

        // Render to string
        let report = report.finish();
        let mut output = Vec::new();
        report
            .write(ContextSourceCache { files: cache_files }, &mut output)
            .ok()?;

        let output_str = String::from_utf8(output).ok()?;

        // Post-process to extend hyperlinks to include line:column numbers
        // Ariadne adds :line:column after our hyperlinked path, so we need to
        // move the hyperlink end marker to include those numbers. Only for
        // self-links: an origin link opens a different file, and the
        // appended coordinates are relative to the virtual file — they
        // must not leak into the notebook URL.
        if enable_hyperlinks && link_target == Some(file.path.as_str()) {
            Some(Self::extend_hyperlink_to_include_line_column(
                &output_str,
                &file.path,
            ))
        } else {
            Some(output_str)
        }
    }

    /// Render source context using [`annotate-snippets`](https://crates.io/crates/annotate-snippets),
    /// the rust-lang toolchain's diagnostic style (private helper for to_text).
    ///
    /// Mirrors [`Self::render_ariadne_source_context`]'s offset-mapping
    /// logic but emits the `error[CODE]: …` / `-->` / gutter-bar look.
    /// Differences from the ariadne path, by design:
    ///
    /// - The error code is rendered natively via `Title::id` (e.g.
    ///   `error[Q-2-5]: …`) rather than prefixed into the message.
    /// - There are **no terminal hyperlinks** — annotate-snippets has no
    ///   OSC 8 support, so `_enable_hyperlinks` is ignored.
    /// - Detail labels are all rendered as `Context` annotations
    ///   (annotate-snippets has no per-label color), so the `DetailKind`
    ///   color distinction and the `Faded` blend are not reproduced.
    /// - Empty-content "padding" details (an ariadne workaround for
    ///   mid-span line elision) are skipped: annotate-snippets folds
    ///   unannotated lines natively, which is the look we want here.
    #[cfg(feature = "annotate-snippets")]
    fn render_annotate_snippets_source_context(
        &self,
        main_location: &quarto_source_map::SourceInfo,
        ctx: &quarto_source_map::SourceContext,
        _enable_hyperlinks: bool,
    ) -> Option<String> {
        use annotate_snippets::{AnnotationKind, Level, Renderer, Snippet};

        // The report's file is the one the span *starts* in — same
        // contract as the ariadne path: `root_file_id()` is the first
        // `Concat` piece's file, wrong for a diagnostic rooted in a
        // later piece, while `map_offset` resolves piece-aware.
        let start_mapped = main_location.map_offset(0, ctx)?;
        let file_id = start_mapped.file_id;

        // A span whose two ends resolve into different pieces cannot be
        // drawn as one snippet; label both ends textually rather than
        // silently clamping into one piece's content.
        let end_mapped = main_location
            .map_offset(main_location.length(), ctx)
            .or_else(|| {
                if main_location.length() > 0 {
                    main_location.map_offset(main_location.length() - 1, ctx)
                } else {
                    None
                }
            })
            .unwrap_or_else(|| start_mapped.clone());
        if end_mapped.file_id != start_mapped.file_id {
            return Some(Self::format_cross_piece_location(
                &start_mapped,
                &end_mapped,
                ctx,
            ));
        }

        let file = ctx.get_file(file_id)?;
        let content = match &file.content {
            Some(c) => c.clone(),
            None => std::fs::read_to_string(&file.path).ok()?,
        };
        // Clamp a mapped byte range into the source, keeping start <= end
        // and both ends on UTF-8 character boundaries (annotate-snippets
        // panics on a mid-character offset just as ariadne does).
        let clamp = |start: usize, end: usize| -> std::ops::Range<usize> {
            Self::snap_span_to_char_boundaries(&content, start, end)
        };

        let main_span = clamp(start_mapped.location.offset, end_mapped.location.offset);

        let level = match self.kind {
            DiagnosticKind::Error => Level::ERROR,
            DiagnosticKind::Warning => Level::WARNING,
            DiagnosticKind::Info => Level::INFO,
            DiagnosticKind::Note => Level::NOTE,
        };

        // Primary label message: the problem statement, else the title.
        let main_message = match &self.problem {
            Some(problem) => problem.as_str(),
            None => self.title.as_str(),
        };

        let mut snippet = Snippet::source(content.as_str())
            .path(file.path.as_str())
            .line_start(1)
            .annotation(AnnotationKind::Primary.span(main_span).label(main_message));

        // Details rooted (start and end) in the report's file become
        // Context annotations; details rooted wholly in another piece
        // render as their own snippet element below the main one instead
        // of being silently dropped. A detail whose own span straddles
        // pieces is not representable as one annotation and is skipped
        // (the main-span cross-piece label covers the common shape).
        let mut foreign: Vec<(String, String, std::ops::Range<usize>, &str)> = Vec::new();
        for detail in &self.details {
            // Skip empty-content padding details (see the doc comment).
            if detail.content.as_str().is_empty() {
                continue;
            }
            let Some(detail_loc) = &detail.location else {
                continue;
            };
            let (Some(detail_start), Some(detail_end)) = (
                detail_loc.map_offset(0, ctx),
                detail_loc.map_offset(detail_loc.length(), ctx),
            ) else {
                continue;
            };
            if detail_start.file_id == detail_end.file_id && detail_start.file_id != file_id {
                let Some(detail_file) = ctx.get_file(detail_start.file_id) else {
                    continue;
                };
                let Some(detail_content) = detail_file
                    .content
                    .clone()
                    .or_else(|| std::fs::read_to_string(&detail_file.path).ok())
                else {
                    continue;
                };
                let detail_span = Self::snap_span_to_char_boundaries(
                    &detail_content,
                    detail_start.location.offset,
                    detail_end.location.offset,
                );
                foreign.push((
                    detail_file.path.clone(),
                    detail_content,
                    detail_span,
                    detail.content.as_str(),
                ));
                continue;
            }
            if detail_start.file_id == file_id && detail_end.file_id == file_id {
                let detail_span = clamp(detail_start.location.offset, detail_end.location.offset);
                snippet = snippet.annotation(
                    AnnotationKind::Context
                        .span(detail_span)
                        .label(detail.content.as_str()),
                );
            }
        }

        // Build the titled group; render the error code natively via `id`,
        // then append each foreign-piece detail as its own snippet element
        // (annotate-snippets draws one excerpt per element, each with its
        // own `--> path` header).
        let mut title = level.primary_title(self.title.as_str());
        if let Some(code) = &self.code {
            title = title.id(code.as_str());
        }
        let mut group = title.element(snippet);
        // Iterate by reference: the built `Snippet`s borrow `path`/`content`,
        // so the owned strings must stay alive in `foreign` until `render`.
        for (path, content, span, message) in &foreign {
            let extra = Snippet::source(content.as_str())
                .path(path)
                .line_start(1)
                .annotation(AnnotationKind::Context.span(span.clone()).label(*message));
            group = group.element(extra);
        }

        // `Renderer::render` returns text with no trailing newline, but
        // `to_text` appends unlocated details and hints directly after the
        // excerpt with `writeln!`. Match the ariadne path (which ends in a
        // newline) so those lines don't glue onto the last source row.
        let mut rendered = Renderer::styled().render(&[group]);
        if !rendered.ends_with('\n') {
            rendered.push('\n');
        }
        Some(rendered)
    }

    /// Extend OSC 8 hyperlinks to include the :line:column suffix that ariadne adds.
    ///
    /// Ariadne formats file references as `path:line:column`, but since we wrap the path
    /// with OSC 8 codes, the structure becomes: `[hyperlink:path]:line:column`
    /// We want: `[hyperlink:path:line:column]`
    ///
    /// This function finds patterns like `path]8;;\:line:column` and moves the hyperlink
    /// end marker to after the line:column part.
    #[cfg(feature = "ariadne")]
    fn extend_hyperlink_to_include_line_column(output: &str, original_path: &str) -> String {
        // Pattern: original_path followed by ]8;;\ then :numbers:numbers
        // We want to move the ]8;;\ to after the :numbers:numbers part
        let end_marker = "\x1b]8;;\x1b\\";
        let search_pattern = format!("{}{}", original_path, end_marker);

        let mut result = output.to_string();
        while let Some(pos) = result.find(&search_pattern) {
            let after_marker = pos + search_pattern.len();
            // Check if what follows is :line:column pattern
            if let Some(rest) = result.get(after_marker..) {
                // Match :digits:digits pattern
                if let Some(colon_end) = Self::find_line_column_end(rest) {
                    // Move the end marker to after the :line:column
                    let before = &result[..pos + original_path.len()];
                    let line_col = &rest[..colon_end];
                    let after = &rest[colon_end..];
                    result = format!("{}{}{}{}", before, line_col, end_marker, after);
                    continue;
                }
            }
            break;
        }
        result
    }

    /// Find the end position of a :line:column pattern at the start of the string.
    /// Returns None if the pattern doesn't match.
    #[cfg(feature = "ariadne")]
    fn find_line_column_end(s: &str) -> Option<usize> {
        let bytes = s.as_bytes();
        if bytes.is_empty() || bytes[0] != b':' {
            return None;
        }

        let mut pos = 1;
        // Read digits for line number
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            pos += 1;
        }
        if pos == 1 || pos >= bytes.len() || bytes[pos] != b':' {
            return None; // No digits or no second colon
        }

        pos += 1; // Skip second colon
        let col_start = pos;
        // Read digits for column number
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            pos += 1;
        }
        if pos == col_start {
            return None; // No digits for column
        }

        Some(pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostic_kind() {
        assert_eq!(DiagnosticKind::Error, DiagnosticKind::Error);
        assert_ne!(DiagnosticKind::Error, DiagnosticKind::Warning);
    }

    #[test]
    fn test_message_content_from_str() {
        let content: MessageContent = "test".into();
        assert_eq!(content.as_str(), "test");
    }

    #[test]
    fn test_diagnostic_message_new() {
        let msg = DiagnosticMessage::new(DiagnosticKind::Error, "Test error");
        assert_eq!(msg.title, "Test error");
        assert_eq!(msg.kind, DiagnosticKind::Error);
        assert!(msg.code.is_none());
        assert!(msg.problem.is_none());
        assert!(msg.details.is_empty());
        assert!(msg.hints.is_empty());
    }

    #[test]
    fn test_diagnostic_message_constructors() {
        let error = DiagnosticMessage::error("Error");
        assert_eq!(error.kind, DiagnosticKind::Error);
        assert!(error.code.is_none());

        let warning = DiagnosticMessage::warning("Warning");
        assert_eq!(warning.kind, DiagnosticKind::Warning);

        let info = DiagnosticMessage::info("Info");
        assert_eq!(info.kind, DiagnosticKind::Info);
    }

    #[test]
    fn test_with_code() {
        let msg = DiagnosticMessage::error("Test error").with_code("Q-1-1");
        assert_eq!(msg.code, Some("Q-1-1".to_string()));
    }

    // The positive case — `docs_url()` for a real code resolves to the
    // quarto.org URL — moved to `quarto-error-catalog`'s integration tests,
    // where the `Q-*` catalog is installed. Here we only cover the
    // catalog-free cases (no code / unknown code → `None`), which hold
    // regardless of whether a catalog is installed.

    #[test]
    fn test_docs_url_without_code() {
        let msg = DiagnosticMessage::error("Test error");
        assert!(msg.docs_url().is_none());
    }

    #[test]
    fn test_docs_url_invalid_code() {
        let msg = DiagnosticMessage::error("Test error").with_code("Q-999-999"); // quarto-error-code-audit-ignore
        assert!(msg.docs_url().is_none());
    }

    #[test]
    fn test_to_text_simple_error() {
        let msg = DiagnosticMessage::error("Something went wrong");
        assert_eq!(msg.to_text(None), "Error: Something went wrong\n");
    }

    #[test]
    fn test_to_text_with_code() {
        let msg = DiagnosticMessage::error("Something went wrong").with_code("Q-1-1");
        assert_eq!(msg.to_text(None), "Error [Q-1-1]: Something went wrong\n");
    }

    #[test]
    fn test_to_text_full_message() {
        use crate::builder::DiagnosticMessageBuilder;

        let msg = DiagnosticMessageBuilder::error("Invalid input")
            .problem("Values must be numeric")
            .add_detail("Found text in column 3")
            .add_info("Columns should contain only numbers")
            .add_hint("Convert to numbers first?")
            .build();

        let text = msg.to_text(None);
        assert!(text.contains("Error: Invalid input"));
        assert!(text.contains("Values must be numeric"));
        assert!(text.contains("✖ Found text in column 3"));
        assert!(text.contains("ℹ Columns should contain only numbers"));
        assert!(text.contains("ℹ Convert to numbers first?"));
    }

    #[test]
    fn test_to_json_simple() {
        let msg = DiagnosticMessage::error("Something went wrong");
        let json = msg.to_json();

        assert_eq!(json["kind"], "error");
        assert_eq!(json["title"], "Something went wrong");
        assert!(json.get("code").is_none());
        assert!(json.get("problem").is_none());
    }

    #[test]
    fn test_to_json_with_code() {
        let msg = DiagnosticMessage::error("Something went wrong").with_code("Q-1-1");
        let json = msg.to_json();

        assert_eq!(json["kind"], "error");
        assert_eq!(json["title"], "Something went wrong");
        assert_eq!(json["code"], "Q-1-1");
    }

    #[test]
    fn test_to_json_full_message() {
        use crate::builder::DiagnosticMessageBuilder;

        let msg = DiagnosticMessageBuilder::error("Invalid input")
            .with_code("Q-1-2") // quarto-error-code-audit-ignore
            .problem("Values must be numeric")
            .add_detail("Found text in column 3")
            .add_info("Expected numbers")
            .add_hint("Convert to numbers first?")
            .build();

        let json = msg.to_json();
        assert_eq!(json["kind"], "error");
        assert_eq!(json["title"], "Invalid input");
        assert_eq!(json["code"], "Q-1-2"); // quarto-error-code-audit-ignore
        assert_eq!(json["problem"]["type"], "markdown");
        assert_eq!(json["problem"]["content"], "Values must be numeric");
        assert_eq!(json["details"][0]["kind"], "error");
        assert_eq!(json["details"][0]["content"]["type"], "markdown");
        assert_eq!(
            json["details"][0]["content"]["content"],
            "Found text in column 3"
        );
        assert_eq!(json["details"][1]["kind"], "info");
        assert_eq!(json["details"][1]["content"]["type"], "markdown");
        assert_eq!(json["details"][1]["content"]["content"], "Expected numbers");
        assert_eq!(json["hints"][0]["type"], "markdown");
        assert_eq!(json["hints"][0]["content"], "Convert to numbers first?");
    }

    #[test]
    fn test_to_json_warning() {
        let msg = DiagnosticMessage::warning("Be careful");
        let json = msg.to_json();

        assert_eq!(json["kind"], "warning");
        assert_eq!(json["title"], "Be careful");
    }

    #[test]
    fn test_location_in_to_text_without_context() {
        use crate::builder::DiagnosticMessageBuilder;

        // Create a location at offsets 100-110
        let location =
            quarto_source_map::SourceInfo::original(quarto_source_map::FileId(0), 100, 110);

        let msg = DiagnosticMessageBuilder::error("Invalid syntax")
            .with_location(location)
            .build();

        let text = msg.to_text(None);

        // Without context, should show offset (we can't get row/column without context)
        assert!(text.contains("Invalid syntax"));
        assert!(text.contains("at offset 100"));
    }

    #[test]
    fn test_location_in_to_text_with_context() {
        use crate::builder::DiagnosticMessageBuilder;

        // Create a source context with a file
        let mut ctx = quarto_source_map::SourceContext::new();
        let file_id = ctx.add_file(
            "test.qmd".to_string(),
            Some("line 1\nline 2\nline 3\nline 4".to_string()),
        );

        // Create a location in that file (offset 7 is start of "line 2")
        let location = quarto_source_map::SourceInfo::original(
            file_id, 7,  // Start of "line 2"
            13, // End of "line 2"
        );

        let msg = DiagnosticMessageBuilder::error("Invalid syntax")
            .with_location(location)
            .build();

        let text = msg.to_text(Some(&ctx));

        // With context, should show file path and 1-indexed location
        assert!(text.contains("Invalid syntax"));
        assert!(text.contains("test.qmd"));
        assert!(text.contains("2:1")); // row 1 + 1, column 0 + 1
    }

    #[test]
    fn test_location_in_to_json() {
        use crate::builder::DiagnosticMessageBuilder;

        let location =
            quarto_source_map::SourceInfo::original(quarto_source_map::FileId(0), 100, 110);

        let msg = DiagnosticMessageBuilder::error("Invalid syntax")
            .with_location(location)
            .build();

        let json = msg.to_json();

        // Should have location field with Original variant
        assert!(json.get("location").is_some());
        let loc = &json["location"];

        // Verify the SourceInfo is serialized correctly (as Original enum variant)
        assert!(loc.get("Original").is_some());
        let original = &loc["Original"];
        assert_eq!(original["file_id"], 0);
        assert_eq!(original["start_offset"], 100);
        assert_eq!(original["end_offset"], 110);
    }

    #[test]
    fn test_location_optional_in_to_json() {
        let msg = DiagnosticMessage::error("No location");
        let json = msg.to_json();

        // Should not have location field when not provided
        assert!(json.get("location").is_none());
    }

    #[test]
    fn test_text_render_options_disable_hyperlinks() {
        use crate::builder::DiagnosticMessageBuilder;

        let mut ctx = quarto_source_map::SourceContext::new();
        let file_id = ctx.add_file("test.qmd".to_string(), Some("test content".to_string()));

        let location = quarto_source_map::SourceInfo::original(file_id, 0, 4);

        let msg = DiagnosticMessageBuilder::error("Test error")
            .with_location(location)
            .build();

        // With hyperlinks enabled (default)
        let with_hyperlinks = msg.to_text(Some(&ctx));

        // With hyperlinks disabled
        let options = TextRenderOptions {
            enable_hyperlinks: false,
        };
        let without_hyperlinks = msg.to_text_with_options(Some(&ctx), &options);

        // When hyperlinks are disabled, output should be different
        // (specifically, no OSC 8 escape sequences)
        if with_hyperlinks.contains("\x1b]8;") {
            assert!(
                !without_hyperlinks.contains("\x1b]8;"),
                "Disabled hyperlinks should not contain OSC 8 codes"
            );
        }
    }

    #[test]
    fn test_text_render_options_default() {
        let options = TextRenderOptions::default();
        assert!(
            options.enable_hyperlinks,
            "Default should enable hyperlinks"
        );
    }

    #[test]
    fn test_render_with_custom_options() {
        use crate::builder::DiagnosticMessageBuilder;

        let msg = DiagnosticMessageBuilder::error("Test")
            .problem("Something went wrong")
            .add_detail("Detail 1")
            .add_hint("Try this")
            .build();

        let options = TextRenderOptions {
            enable_hyperlinks: false,
        };

        let text = msg.to_text_with_options(None, &options);

        // Should still render properly without hyperlinks
        assert!(text.contains("Error: Test"));
        assert!(text.contains("Something went wrong"));
        assert!(text.contains("Detail 1"));
        assert!(text.contains("Try this"));
    }

    /// Strip CSI SGR color sequences (`ESC [ … m`). The annotate-snippets
    /// path emits no OSC 8 hyperlinks, so color is all we need to remove
    /// to make substring assertions robust to styling.
    #[cfg(feature = "annotate-snippets")]
    fn strip_ansi(s: &str) -> String {
        let mut out = String::new();
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\u{1b}' {
                for n in chars.by_ref() {
                    if n == 'm' {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    /// The annotate-snippets renderer emits the rust-lang toolchain look:
    /// an `error[CODE]: …` header, a `-->` origin line, and `^` underlines
    /// — not ariadne's enclosing box.
    #[cfg(feature = "annotate-snippets")]
    #[test]
    fn annotate_snippets_renderer_produces_rust_style_output() {
        use crate::builder::DiagnosticMessageBuilder;

        let mut ctx = quarto_source_map::SourceContext::new();
        let file_id = ctx.add_file(
            "test.qmd".to_string(),
            Some("line 1\nline 2\nline 3".to_string()),
        );
        // Offsets 7..13 cover "line 2" on row 2.
        let location = quarto_source_map::SourceInfo::original(file_id, 7, 13);
        let msg = DiagnosticMessageBuilder::error("Bad thing")
            .with_code("Q-9-9")
            .with_location(location)
            .problem("this is wrong")
            .build();

        let opts = TextRenderOptions {
            enable_hyperlinks: false,
        };
        let raw =
            msg.to_text_with_renderer(Some(&ctx), &opts, Some(SourceRenderer::AnnotateSnippets));
        let text = strip_ansi(&raw);

        assert!(
            text.contains("error[Q-9-9]"),
            "expected rust-style code header; got: {text:?}"
        );
        assert!(
            text.contains("-->"),
            "expected rust-style origin arrow; got: {text:?}"
        );
        assert!(
            text.contains("test.qmd:2:1"),
            "expected mapped location; got: {text:?}"
        );
        assert!(
            !text.contains('\u{256D}'),
            "annotate-snippets must not draw ariadne's box corner; got: {text:?}"
        );
        // No OSC 8 hyperlinks from annotate-snippets.
        assert!(
            !raw.contains("\u{1b}]8;"),
            "annotate-snippets emits no OSC 8 hyperlinks; got: {raw:?}"
        );
    }

    /// Direct coverage of the snapping helper's contract: clamp into the
    /// file, widen to whole characters, never invert.
    #[cfg(any(feature = "ariadne", feature = "annotate-snippets"))]
    #[test]
    fn snap_span_widens_to_whole_characters() {
        // `\u{2728}` occupies bytes 3..6.
        let content = "abc\u{2728}def";
        assert_eq!(content.len(), 9);

        let snap = |s, e| DiagnosticMessage::snap_span_to_char_boundaries(content, s, e);

        // Already aligned: unchanged.
        assert_eq!(snap(0, 3), 0..3);
        assert_eq!(snap(3, 6), 3..6);

        // Start inside the char floors to its first byte; end inside it ceils
        // to its last, so the highlight covers the whole character.
        assert_eq!(snap(4, 9), 3..9);
        assert_eq!(snap(5, 9), 3..9);
        assert_eq!(snap(0, 4), 0..6);
        assert_eq!(snap(0, 5), 0..6);
        assert_eq!(snap(4, 5), 3..6);

        // Past EOF clamps to the file length.
        assert_eq!(snap(3, 999), 3..9);
        assert_eq!(snap(999, 999), 9..9);

        // Inverted input collapses to an empty range rather than inverting.
        assert_eq!(snap(6, 3), 6..6);

        // An empty range inside a character still snaps to a boundary.
        let r = snap(4, 4);
        assert!(content.is_char_boundary(r.start) && content.is_char_boundary(r.end));
        assert!(r.start <= r.end);

        // Second content block: the exact offset pair the two
        // `..._does_not_panic` integration tests below use, now that the
        // `quarto-source-map` 0.1.2+ floor makes 21 unreachable at their
        // level (see those tests' doc comments). This is where that
        // coverage now lives.
        //
        // Layout (byte offsets):
        //   `text: <span>Ask AI ` = 0..19, `\u{2728}` = 19..22, `</span>` = 22..29
        let content2 = "text: <span>Ask AI \u{2728}</span>";
        assert!(!content2.is_char_boundary(21), "test fixture precondition");
        let snap2 = |s, e| DiagnosticMessage::snap_span_to_char_boundaries(content2, s, e);

        // 21 is mid-`\u{2728}` (bytes 19..22) and floors to 19; 28 is
        // already on a boundary (inside the trailing ASCII `</span>`) and
        // is left unchanged.
        assert_eq!(snap2(21, 28), 19..28);
    }

    /// A diagnostic whose `SourceInfo` span originally lands mid-character
    /// still renders end to end under the ariadne renderer.
    ///
    /// This test used to be the integration-level proof that
    /// `snap_span_to_char_boundaries` prevents ariadne's mid-character
    /// panic. It no longer is: `quarto-source-map` 0.1.2+ floors
    /// `offset_to_location`'s returned offset to a UTF-8 character
    /// boundary, so by the time `map_offset` hands this test's span
    /// (21..28, with 21 mid-`\u{2728}`) to the renderer it has already
    /// become 19..28 — the snap in this crate is never exercised against a
    /// mid-character offset at this level, because one can no longer be
    /// constructed here. The snap's actual coverage moved to
    /// `snap_span_widens_to_whole_characters`'s second content block, which
    /// calls it directly with these same offsets.
    ///
    /// **Accepted, not an oversight:** after the upstream floor, no revert
    /// of this crate's own code (the snap helper or its three call sites)
    /// can turn this test red — it is unbound with respect to this crate's
    /// diff. That is known and accepted; the test stays as an end-to-end
    /// smoke check that a span-carrying diagnostic renders successfully
    /// under ariadne, not as a snap regression test.
    ///
    /// Layout of the source below (byte offsets):
    ///   `text: <span>Ask AI ` = 0..19, `\u{2728}` = 19..22, `</span>` = 22..29
    /// so 21 is two bytes into the three-byte char — exactly the observed
    /// off-by-one-left onto a multi-byte boundary.
    #[cfg(feature = "ariadne")]
    #[test]
    fn ariadne_renders_diagnostic_with_originally_mid_character_span() {
        use crate::builder::DiagnosticMessageBuilder;

        let content = "text: <span>Ask AI \u{2728}</span>".to_string();
        assert!(!content.is_char_boundary(21), "test fixture precondition");

        let mut ctx = quarto_source_map::SourceContext::new();
        let file_id = ctx.add_file("_quarto.yml".to_string(), Some(content.clone()));
        // 21..28 — start is mid-`\u{2728}`, mirroring the config-path shift.
        let location = quarto_source_map::SourceInfo::original(file_id, 21, 28);
        let msg = DiagnosticMessageBuilder::warning("HTML element converted to raw HTML")
            .with_code("Q-2-9")
            .with_location(location)
            .build();

        let opts = TextRenderOptions {
            enable_hyperlinks: false,
        };
        let text = msg.to_text_with_renderer(Some(&ctx), &opts, Some(SourceRenderer::Ariadne));

        assert!(
            text.contains("HTML element converted to raw HTML"),
            "diagnostic must still render; got: {text:?}"
        );
        assert!(
            text.contains("_quarto.yml"),
            "source context must still render; got: {text:?}"
        );
    }

    /// The same end-to-end smoke check as
    /// `ariadne_renders_diagnostic_with_originally_mid_character_span`, for
    /// the annotate-snippets renderer.
    ///
    /// It no longer exercises the mid-character path either, for the same
    /// reason: `quarto-source-map` 0.1.2+'s floor in `offset_to_location`
    /// means `map_offset` has already snapped this test's 21..28 span to
    /// 19..28 before it reaches annotate-snippets' `clamp` closure, so the
    /// closure never sees a mid-character offset from this call path. The
    /// snap's real coverage lives in `snap_span_widens_to_whole_characters`'s
    /// second content block (same 21..28 offsets, exercised directly).
    ///
    /// **Accepted, not an oversight:** as with the ariadne test above, no
    /// revert of this crate's own snap logic can turn this test red after
    /// the upstream floor — that unbinding is known and accepted.
    #[cfg(feature = "annotate-snippets")]
    #[test]
    fn annotate_snippets_renders_diagnostic_with_originally_mid_character_span() {
        use crate::builder::DiagnosticMessageBuilder;

        let content = "text: <span>Ask AI \u{2728}</span>".to_string();
        assert!(!content.is_char_boundary(21), "test fixture precondition");

        let mut ctx = quarto_source_map::SourceContext::new();
        let file_id = ctx.add_file("_quarto.yml".to_string(), Some(content.clone()));
        let location = quarto_source_map::SourceInfo::original(file_id, 21, 28);
        let msg = DiagnosticMessageBuilder::warning("HTML element converted to raw HTML")
            .with_code("Q-2-9")
            .with_location(location)
            .build();

        let opts = TextRenderOptions {
            enable_hyperlinks: false,
        };
        let text =
            msg.to_text_with_renderer(Some(&ctx), &opts, Some(SourceRenderer::AnnotateSnippets));

        assert!(
            text.contains("HTML element converted to raw HTML"),
            "diagnostic must still render; got: {text:?}"
        );
    }

    /// Strip CSI SGR color sequences, for tests gated under a single
    /// renderer feature that can't rely on `strip_ansi` above (which is
    /// gated on `annotate-snippets` only — ariadne colorizes its source
    /// line and marker row too, so an ariadne-only test needs the same
    /// stripping without pulling in that feature). Same logic as
    /// `strip_ansi`, duplicated rather than re-gated so as not to touch
    /// the existing helper.
    #[cfg(any(feature = "ariadne", feature = "annotate-snippets"))]
    fn strip_ansi_colors(s: &str) -> String {
        let mut out = String::new();
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\u{1b}' {
                for n in chars.by_ref() {
                    if n == 'm' {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    /// Measures the *rendered* width of a label whose mapped span is
    /// genuinely zero-width, under the ariadne renderer.
    ///
    /// `\u{2728}` occupies bytes 6..9 of the content below; the input span
    /// `SourceInfo::original(fid, 7, 8)` has **both ends** strictly inside
    /// that character (unlike the `..._does_not_panic` tests above, whose
    /// span only *starts* mid-character). Before the `quarto-source-map`
    /// 0.1.2+ floor, `map_offset` passed the raw offsets 7 and 8 through
    /// unchanged and this crate's own snap widened them to the whole
    /// character (6..9). After the floor, `offset_to_location` already
    /// floors both 7 and 8 down to 6 before this crate ever sees them, so
    /// both mapped offsets are 6 — the snap runs on `6..6`, which is
    /// already boundary-aligned, and has nothing left to widen. The
    /// highlight that reaches the renderer is therefore zero-width, not
    /// the whole character.
    ///
    /// ariadne's `Report::build` anchor is already `start..start`, so "a
    /// zero-width label probably renders fine" was a reasonable guess
    /// before this test — turning that guess into a measurement is the
    /// point here. The assertion is keyed on the renderer's own
    /// zero-width-vs-one-character marker *shape* (a bare `│` vs. `┬─`),
    /// not merely on the message text appearing, so it fails if the
    /// highlight ever widens back to covering the whole character.
    #[cfg(feature = "ariadne")]
    #[test]
    fn ariadne_zero_width_label_renders_a_bare_marker() {
        use crate::builder::DiagnosticMessageBuilder;

        let content = "x = 'A\u{2728}B'".to_string();
        let mut ctx = quarto_source_map::SourceContext::new();
        let file_id = ctx.add_file("scratch.qmd".to_string(), Some(content.clone()));
        let location = quarto_source_map::SourceInfo::original(file_id, 7, 8);
        let msg = DiagnosticMessageBuilder::warning("scratch")
            .with_code("Q-2-9")
            .with_location(location)
            .build();
        let opts = TextRenderOptions {
            enable_hyperlinks: false,
        };
        let text = msg.to_text_with_renderer(Some(&ctx), &opts, Some(SourceRenderer::Ariadne));
        let stripped = strip_ansi_colors(&text);

        let lines: Vec<&str> = stripped.lines().collect();
        let source_idx = lines
            .iter()
            .position(|l| l.contains("x = 'A\u{2728}B'"))
            .unwrap_or_else(|| panic!("source line must render; got: {stripped:?}"));
        let marker_line = lines[source_idx + 1];
        // Skip past the gutter's own `│` (present on every row, e.g.
        // `   │       │  `) to isolate the marker glyphs themselves.
        let gutter_end = marker_line
            .find('│')
            .map(|i| i + '│'.len_utf8())
            .unwrap_or_else(|| panic!("marker row must have a gutter `│`; got: {marker_line:?}"));
        let marker = marker_line[gutter_end..].trim();

        assert_eq!(
            marker, "│",
            "expected the zero-width `│` marker (a whole-character label \
             would instead draw `┬─`); got {marker:?} in:\n{stripped}"
        );
    }

    /// The same measurement as `ariadne_zero_width_label_renders_a_bare_marker`,
    /// for the annotate-snippets renderer. See that test's doc comment for
    /// why both ends of `SourceInfo::original(fid, 7, 8)` land on the same
    /// mapped offset (6) after the `quarto-source-map` 0.1.2+ floor.
    ///
    /// annotate-snippets underlines a span with one `^` per byte of width,
    /// so the discriminating measurement here is even more direct than
    /// ariadne's marker shape: a zero-width label draws exactly one `^`,
    /// a whole-character label draws two (`^^`).
    #[cfg(feature = "annotate-snippets")]
    #[test]
    fn annotate_snippets_zero_width_label_renders_a_single_caret() {
        use crate::builder::DiagnosticMessageBuilder;

        let content = "x = 'A\u{2728}B'".to_string();
        let mut ctx = quarto_source_map::SourceContext::new();
        let file_id = ctx.add_file("scratch.qmd".to_string(), Some(content.clone()));
        let location = quarto_source_map::SourceInfo::original(file_id, 7, 8);
        let msg = DiagnosticMessageBuilder::warning("scratch")
            .with_code("Q-2-9")
            .with_location(location)
            .build();
        let opts = TextRenderOptions {
            enable_hyperlinks: false,
        };
        let text =
            msg.to_text_with_renderer(Some(&ctx), &opts, Some(SourceRenderer::AnnotateSnippets));
        let stripped = strip_ansi_colors(&text);

        let lines: Vec<&str> = stripped.lines().collect();
        let source_idx = lines
            .iter()
            .position(|l| l.contains("x = 'A\u{2728}B'"))
            .unwrap_or_else(|| panic!("source line must render; got: {stripped:?}"));
        let marker_line = lines[source_idx + 1];
        let caret_run: String = marker_line.chars().filter(|&c| c == '^').collect();

        assert_eq!(
            caret_run, "^",
            "expected a single `^` caret marking a zero-width label (a \
             whole-character label would instead draw `^^`); got \
             {caret_run:?} in line: {marker_line:?}"
        );
    }

    /// Forcing a specific renderer is honored: ariadne draws its boxed
    /// excerpt (the U+256D corner) while annotate-snippets does not.
    #[cfg(all(feature = "ariadne", feature = "annotate-snippets"))]
    #[test]
    fn renderer_selection_switches_styles() {
        use crate::builder::DiagnosticMessageBuilder;

        let mut ctx = quarto_source_map::SourceContext::new();
        let file_id = ctx.add_file("a.qmd".to_string(), Some("alpha\nbeta\ngamma".to_string()));
        let location = quarto_source_map::SourceInfo::original(file_id, 6, 10); // "beta"
        let msg = DiagnosticMessageBuilder::error("Pick a style")
            .with_location(location)
            .build();
        let opts = TextRenderOptions {
            enable_hyperlinks: false,
        };

        let ariadne = msg.to_text_with_renderer(Some(&ctx), &opts, Some(SourceRenderer::Ariadne));
        let snippets =
            msg.to_text_with_renderer(Some(&ctx), &opts, Some(SourceRenderer::AnnotateSnippets));

        assert!(ariadne.contains('\u{256D}'), "ariadne draws a box corner");
        assert!(
            !strip_ansi(&snippets).contains('\u{256D}'),
            "annotate-snippets does not"
        );
        assert!(strip_ansi(&snippets).contains("-->"));
    }

    // ==================== Concat / cross-piece rendering ====================
    //
    // A diagnostic whose location resolves through a multi-piece `Concat`
    // (the shape q2's ipynb processor produces: one virtual file per
    // notebook cell) must render against the piece the span actually
    // resolves into — not `root_file_id()`, which is the *first* rooted
    // piece. These tests pin the plan-7c contract: report file from
    // `start_mapped.file_id`; a span straddling pieces renders a
    // cross-file location label with no snippet; a detail rooted in
    // another piece renders in its own file's source block.

    /// Two per-cell virtual files joined by a `Concat`, exactly as the
    /// ipynb converter registers them. Returns the context, the concat,
    /// and piece 1's length (the concat offset where piece 2 begins).
    #[cfg(any(feature = "ariadne", feature = "annotate-snippets"))]
    fn concat_fixture() -> (
        quarto_source_map::SourceContext,
        quarto_source_map::SourceInfo,
        usize,
    ) {
        let piece1 = "first cell text\n";
        let piece2 = "second cell text\n";
        let mut ctx = quarto_source_map::SourceContext::new();
        let f1 = ctx.add_file(
            "notebook.ipynb[cell 1, markdown]".to_string(),
            Some(piece1.to_string()),
        );
        let f2 = ctx.add_file(
            "notebook.ipynb[cell 2, markdown]".to_string(),
            Some(piece2.to_string()),
        );
        let concat = quarto_source_map::SourceInfo::concat(vec![
            (
                quarto_source_map::SourceInfo::original(f1, 0, piece1.len()),
                piece1.len(),
            ),
            (
                quarto_source_map::SourceInfo::original(f2, 0, piece2.len()),
                piece2.len(),
            ),
        ]);
        (ctx, concat, piece1.len())
    }

    /// The percent/spin shape: a `Concat` whose pieces all root to ONE
    /// file. Must render exactly as a plain single-file diagnostic — the
    /// fix must not disturb it.
    #[cfg(feature = "ariadne")]
    #[test]
    fn ariadne_concat_single_file_pieces_passthrough() {
        use crate::builder::DiagnosticMessageBuilder;

        let content = "alpha\nbeta\ngamma\n";
        let mut ctx = quarto_source_map::SourceContext::new();
        let f1 = ctx.add_file("script.qmd".to_string(), Some(content.to_string()));
        let concat = quarto_source_map::SourceInfo::concat(vec![
            (quarto_source_map::SourceInfo::original(f1, 0, 6), 6),
            (
                quarto_source_map::SourceInfo::original(f1, 6, content.len()),
                content.len() - 6,
            ),
        ]);
        let location = quarto_source_map::SourceInfo::substring(concat, 6, 10); // "beta"
        let msg = DiagnosticMessageBuilder::error("Bad chunk")
            .with_location(location)
            .build();
        let opts = TextRenderOptions {
            enable_hyperlinks: false,
        };
        let text = strip_ansi_colors(&msg.to_text_with_renderer(
            Some(&ctx),
            &opts,
            Some(SourceRenderer::Ariadne),
        ));

        assert!(
            text.contains("script.qmd"),
            "single-file passthrough must keep the file label; got:\n{text}"
        );
        assert!(
            text.contains("beta"),
            "single-file passthrough must render the snippet; got:\n{text}"
        );
    }

    /// A main span rooted wholly in the *first* piece already renders
    /// correctly (`root_file_id()` happens to agree with
    /// `start_mapped.file_id`); must stay correct after the fix.
    #[cfg(feature = "ariadne")]
    #[test]
    fn ariadne_concat_main_span_in_first_piece_renders_own_file() {
        use crate::builder::DiagnosticMessageBuilder;

        let (ctx, concat, _) = concat_fixture();
        let location = quarto_source_map::SourceInfo::substring(concat, 0, 5); // "first"
        let msg = DiagnosticMessageBuilder::error("Bad cell")
            .with_location(location)
            .build();
        let opts = TextRenderOptions {
            enable_hyperlinks: false,
        };
        let text = strip_ansi_colors(&msg.to_text_with_renderer(
            Some(&ctx),
            &opts,
            Some(SourceRenderer::Ariadne),
        ));

        assert!(
            text.contains("notebook.ipynb[cell 1, markdown]"),
            "first-piece span must label piece 1; got:\n{text}"
        );
        assert!(
            text.contains("first cell text"),
            "first-piece span must show piece 1's snippet; got:\n{text}"
        );
        assert!(
            !text.contains("notebook.ipynb[cell 2, markdown]"),
            "piece 2 must not appear; got:\n{text}"
        );
    }

    /// THE cross-piece case: a diagnostic rooted wholly in a later piece
    /// must be labeled and snippeted from *that* piece — today it gets
    /// piece 1's label and piece 1's content at piece-2 offsets.
    #[cfg(feature = "ariadne")]
    #[test]
    fn ariadne_concat_main_span_in_later_piece_renders_own_file() {
        use crate::builder::DiagnosticMessageBuilder;

        let (ctx, concat, l1) = concat_fixture();
        let location = quarto_source_map::SourceInfo::substring(concat, l1, l1 + 6); // "second"
        let msg = DiagnosticMessageBuilder::error("Bad cell")
            .with_location(location)
            .build();
        let opts = TextRenderOptions {
            enable_hyperlinks: false,
        };
        let text = strip_ansi_colors(&msg.to_text_with_renderer(
            Some(&ctx),
            &opts,
            Some(SourceRenderer::Ariadne),
        ));

        assert!(
            text.contains("notebook.ipynb[cell 2, markdown]"),
            "later-piece span must label the owning piece; got:\n{text}"
        );
        assert!(
            text.contains("second cell text"),
            "later-piece span must snippet the owning piece; got:\n{text}"
        );
        assert!(
            !text.contains("first cell text"),
            "the wrong piece must not render; got:\n{text}"
        );
    }

    /// A span straddling two pieces cannot be one snippet; it must render
    /// an explicit cross-file label naming both pieces and no snippet —
    /// today it is silently clamped into piece 1.
    #[cfg(feature = "ariadne")]
    #[test]
    fn ariadne_concat_straddling_span_labels_both_pieces_without_snippet() {
        use crate::builder::DiagnosticMessageBuilder;

        let (ctx, concat, l1) = concat_fixture();
        // Starts mid-piece-1 ("cell text…"), ends mid-piece-2 ("second").
        let location = quarto_source_map::SourceInfo::substring(concat, 6, l1 + 6);
        let msg = DiagnosticMessageBuilder::error("Cross-cell markdown")
            .with_location(location)
            .build();
        let opts = TextRenderOptions {
            enable_hyperlinks: false,
        };
        let text = strip_ansi_colors(&msg.to_text_with_renderer(
            Some(&ctx),
            &opts,
            Some(SourceRenderer::Ariadne),
        ));

        assert!(
            text.contains("--> notebook.ipynb[cell 1, markdown]:1:7"),
            "straddling span must name its start piece and position; got:\n{text}"
        );
        assert!(
            text.contains("(spans through notebook.ipynb[cell 2, markdown]:1:7)"),
            "straddling span must name its end piece and position; got:\n{text}"
        );
        assert!(
            !text.contains("first cell text") && !text.contains("second cell text"),
            "a cross-piece span must render no snippet; got:\n{text}"
        );
    }

    /// A detail rooted wholly in another piece must render — in its own
    /// file's source block — not be silently dropped.
    #[cfg(feature = "ariadne")]
    #[test]
    fn ariadne_concat_detail_in_another_piece_renders_own_file() {
        use crate::builder::DiagnosticMessageBuilder;

        let (ctx, concat, l1) = concat_fixture();
        let main = quarto_source_map::SourceInfo::substring(concat.clone(), 0, 5);
        let detail = quarto_source_map::SourceInfo::substring(concat, l1, l1 + 6);
        let msg = DiagnosticMessageBuilder::error("Mismatch")
            .with_location(main)
            .add_detail_at("related token in cell 2", detail)
            .build();
        let opts = TextRenderOptions {
            enable_hyperlinks: false,
        };
        let text = strip_ansi_colors(&msg.to_text_with_renderer(
            Some(&ctx),
            &opts,
            Some(SourceRenderer::Ariadne),
        ));

        assert!(
            text.contains("notebook.ipynb[cell 1, markdown]") && text.contains("first cell text"),
            "main span must still render; got:\n{text}"
        );
        assert!(
            text.contains("notebook.ipynb[cell 2, markdown]"),
            "foreign-piece detail must render its own block; got:\n{text}"
        );
        assert!(
            text.contains("second cell text"),
            "foreign-piece detail must snippet its own file; got:\n{text}"
        );
        assert!(
            text.contains("related token in cell 2"),
            "foreign-piece detail must keep its message; got:\n{text}"
        );
    }

    /// Passthrough for the annotate-snippets renderer (see the ariadne
    /// twin for the contract).
    #[cfg(feature = "annotate-snippets")]
    #[test]
    fn annotate_snippets_concat_single_file_pieces_passthrough() {
        use crate::builder::DiagnosticMessageBuilder;

        let content = "alpha\nbeta\ngamma\n";
        let mut ctx = quarto_source_map::SourceContext::new();
        let f1 = ctx.add_file("script.qmd".to_string(), Some(content.to_string()));
        let concat = quarto_source_map::SourceInfo::concat(vec![
            (quarto_source_map::SourceInfo::original(f1, 0, 6), 6),
            (
                quarto_source_map::SourceInfo::original(f1, 6, content.len()),
                content.len() - 6,
            ),
        ]);
        let location = quarto_source_map::SourceInfo::substring(concat, 6, 10); // "beta"
        let msg = DiagnosticMessageBuilder::error("Bad chunk")
            .with_location(location)
            .build();
        let opts = TextRenderOptions {
            enable_hyperlinks: false,
        };
        let text = strip_ansi(&msg.to_text_with_renderer(
            Some(&ctx),
            &opts,
            Some(SourceRenderer::AnnotateSnippets),
        ));

        assert!(
            text.contains("script.qmd") && text.contains("beta"),
            "single-file passthrough must render unchanged; got:\n{text}"
        );
    }

    /// First-piece span under annotate-snippets (see the ariadne twin).
    #[cfg(feature = "annotate-snippets")]
    #[test]
    fn annotate_snippets_concat_main_span_in_first_piece_renders_own_file() {
        use crate::builder::DiagnosticMessageBuilder;

        let (ctx, concat, _) = concat_fixture();
        let location = quarto_source_map::SourceInfo::substring(concat, 0, 5); // "first"
        let msg = DiagnosticMessageBuilder::error("Bad cell")
            .with_location(location)
            .build();
        let opts = TextRenderOptions {
            enable_hyperlinks: false,
        };
        let text = strip_ansi(&msg.to_text_with_renderer(
            Some(&ctx),
            &opts,
            Some(SourceRenderer::AnnotateSnippets),
        ));

        assert!(
            text.contains("notebook.ipynb[cell 1, markdown]") && text.contains("first cell text"),
            "first-piece span must render piece 1; got:\n{text}"
        );
        assert!(
            !text.contains("notebook.ipynb[cell 2, markdown]"),
            "piece 2 must not appear; got:\n{text}"
        );
    }

    /// Later-piece span under annotate-snippets (see the ariadne twin).
    #[cfg(feature = "annotate-snippets")]
    #[test]
    fn annotate_snippets_concat_main_span_in_later_piece_renders_own_file() {
        use crate::builder::DiagnosticMessageBuilder;

        let (ctx, concat, l1) = concat_fixture();
        let location = quarto_source_map::SourceInfo::substring(concat, l1, l1 + 6); // "second"
        let msg = DiagnosticMessageBuilder::error("Bad cell")
            .with_location(location)
            .build();
        let opts = TextRenderOptions {
            enable_hyperlinks: false,
        };
        let text = strip_ansi(&msg.to_text_with_renderer(
            Some(&ctx),
            &opts,
            Some(SourceRenderer::AnnotateSnippets),
        ));

        assert!(
            text.contains("notebook.ipynb[cell 2, markdown]"),
            "later-piece span must label the owning piece; got:\n{text}"
        );
        assert!(
            text.contains("second cell text"),
            "later-piece span must snippet the owning piece; got:\n{text}"
        );
        assert!(
            !text.contains("first cell text"),
            "the wrong piece must not render; got:\n{text}"
        );
    }

    /// Straddling span under annotate-snippets (see the ariadne twin).
    #[cfg(feature = "annotate-snippets")]
    #[test]
    fn annotate_snippets_concat_straddling_span_labels_both_pieces_without_snippet() {
        use crate::builder::DiagnosticMessageBuilder;

        let (ctx, concat, l1) = concat_fixture();
        let location = quarto_source_map::SourceInfo::substring(concat, 6, l1 + 6);
        let msg = DiagnosticMessageBuilder::error("Cross-cell markdown")
            .with_location(location)
            .build();
        let opts = TextRenderOptions {
            enable_hyperlinks: false,
        };
        let text = strip_ansi(&msg.to_text_with_renderer(
            Some(&ctx),
            &opts,
            Some(SourceRenderer::AnnotateSnippets),
        ));

        assert!(
            text.contains("--> notebook.ipynb[cell 1, markdown]:1:7"),
            "straddling span must name its start piece and position; got:\n{text}"
        );
        assert!(
            text.contains("(spans through notebook.ipynb[cell 2, markdown]:1:7)"),
            "straddling span must name its end piece and position; got:\n{text}"
        );
        assert!(
            !text.contains("first cell text") && !text.contains("second cell text"),
            "a cross-piece span must render no snippet; got:\n{text}"
        );
    }

    /// Foreign-piece detail under annotate-snippets (see the ariadne
    /// twin).
    #[cfg(feature = "annotate-snippets")]
    #[test]
    fn annotate_snippets_concat_detail_in_another_piece_renders_own_file() {
        use crate::builder::DiagnosticMessageBuilder;

        let (ctx, concat, l1) = concat_fixture();
        let main = quarto_source_map::SourceInfo::substring(concat.clone(), 0, 5);
        let detail = quarto_source_map::SourceInfo::substring(concat, l1, l1 + 6);
        let msg = DiagnosticMessageBuilder::error("Mismatch")
            .with_location(main)
            .add_detail_at("related token in cell 2", detail)
            .build();
        let opts = TextRenderOptions {
            enable_hyperlinks: false,
        };
        let text = strip_ansi(&msg.to_text_with_renderer(
            Some(&ctx),
            &opts,
            Some(SourceRenderer::AnnotateSnippets),
        ));

        assert!(
            text.contains("notebook.ipynb[cell 1, markdown]") && text.contains("first cell text"),
            "main span must still render; got:\n{text}"
        );
        assert!(
            text.contains("notebook.ipynb[cell 2, markdown]")
                && text.contains("second cell text")
                && text.contains("related token in cell 2"),
            "foreign-piece detail must render its own block with its message; got:\n{text}"
        );
    }

    // ==================== Origin-aware hyperlinks (plan 7c) ====================
    //
    // A virtual file carrying `FileMetadata::origin` (a notebook cell) is
    // hyperlinked to the *owning notebook on disk*, and the `#line:column`
    // fragment must be suppressed: origin coordinates are cell-relative and
    // would be wrong in the notebook's URL. Without origin, a virtual file
    // gets no hyperlink at all (its pseudo-path does not exist on disk).

    /// Extract every OSC 8 link URL from raw terminal output.
    #[cfg(feature = "ariadne")]
    fn osc8_urls(text: &str) -> Vec<String> {
        let mut urls = Vec::new();
        let mut rest = text;
        while let Some(start) = rest.find("\x1b]8;;") {
            let after = &rest[start + 5..];
            match after.find("\x1b\\") {
                Some(end) => {
                    urls.push(after[..end].to_string());
                    rest = &after[end..];
                }
                None => break,
            }
        }
        urls
    }

    /// A real notebook on disk plus two per-cell virtual files joined by a
    /// `Concat`, with `origin` attached to both cells. `notebook_path` is the
    /// absolute disk path — what q2 registers after resolving the document.
    #[cfg(feature = "ariadne")]
    fn origin_fixture() -> (
        quarto_source_map::SourceContext,
        quarto_source_map::SourceInfo,
        tempfile::TempDir,
        std::path::PathBuf,
        usize,
    ) {
        let piece1 = "first cell text\n";
        let piece2 = "second cell text\n";
        let dir = tempfile::TempDir::new().unwrap();
        let notebook = dir.path().join("notebook.ipynb");
        std::fs::write(&notebook, "{}").unwrap();
        let origin_for = |index: usize| {
            Some(quarto_source_map::FileOrigin::NotebookCell {
                notebook_path: notebook.display().to_string(),
                cell_index: index,
                cell_id: None,
                cell_type: "markdown".to_string(),
            })
        };
        let mut ctx = quarto_source_map::SourceContext::new();
        let f1 = ctx.add_file(
            "notebook.ipynb[cell 1, markdown]".to_string(),
            Some(piece1.to_string()),
        );
        let f2 = ctx.add_file(
            "notebook.ipynb[cell 2, markdown]".to_string(),
            Some(piece2.to_string()),
        );
        ctx.get_file_mut(f1).unwrap().metadata.origin = origin_for(1);
        ctx.get_file_mut(f2).unwrap().metadata.origin = origin_for(2);
        let concat = quarto_source_map::SourceInfo::concat(vec![
            (
                quarto_source_map::SourceInfo::original(f1, 0, piece1.len()),
                piece1.len(),
            ),
            (
                quarto_source_map::SourceInfo::original(f2, 0, piece2.len()),
                piece2.len(),
            ),
        ]);
        (ctx, concat, dir, notebook, piece1.len())
    }

    #[cfg(feature = "ariadne")]
    #[test]
    fn ariadne_origin_cell_hyperlinks_the_owning_notebook() {
        use crate::builder::DiagnosticMessageBuilder;

        let (ctx, concat, _dir, notebook, l1) = origin_fixture();
        let location = quarto_source_map::SourceInfo::substring(concat, l1, l1 + 6); // "second"
        let msg = DiagnosticMessageBuilder::error("Bad cell")
            .with_location(location)
            .build();
        let opts = TextRenderOptions {
            enable_hyperlinks: true,
        };
        let text = msg.to_text_with_renderer(Some(&ctx), &opts, Some(SourceRenderer::Ariadne));

        let canonical = std::fs::canonicalize(&notebook).unwrap();
        let expected_prefix = format!("file://{}", canonical.display());
        let urls = osc8_urls(&text);
        let cell_link = urls
            .iter()
            .find(|u| u.starts_with(&expected_prefix))
            .expect("cell label must hyperlink to the owning notebook");

        // The fragment must be absent: origin coordinates are cell-relative
        // and would be wrong as a notebook line/column.
        assert!(
            !cell_link.contains('#'),
            "cell hyperlink must not carry a #line:col fragment; got {cell_link:?}"
        );
        // The human-readable label is still the cell-qualified pseudo-path.
        assert!(
            text.contains("notebook.ipynb[cell 2, markdown]"),
            "label must stay the pseudo-path; got:\n{text}"
        );
    }

    #[cfg(feature = "ariadne")]
    #[test]
    fn ariadne_virtual_file_without_origin_gets_no_hyperlink() {
        use crate::builder::DiagnosticMessageBuilder;

        let (ctx, concat, _l1) = concat_fixture();
        let location = quarto_source_map::SourceInfo::substring(concat, 0, 5);
        let msg = DiagnosticMessageBuilder::error("Bad cell")
            .with_location(location)
            .build();
        let opts = TextRenderOptions {
            enable_hyperlinks: true,
        };
        let text = msg.to_text_with_renderer(Some(&ctx), &opts, Some(SourceRenderer::Ariadne));

        assert!(
            osc8_urls(&text).is_empty(),
            "a virtual file whose path does not exist on disk must not be hyperlinked; got:\n{text}"
        );
    }
}
