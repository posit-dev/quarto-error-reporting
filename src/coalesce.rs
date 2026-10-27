//! Cross-source diagnostic coalescing.
//!
//! When a single underlying problem produces a diagnostic on many
//! pages — for example, one bad `theme:` key in `_quarto.yml`
//! triggering [`Q-14-1`](../../error_catalog.json) once per rendered
//! page — the renderer should collapse them into a single emission
//! that lists the affected pages, rather than printing the same
//! ariadne block hundreds of times.
//!
//! # The primary key is the source location
//!
//! Two diagnostics whose `location` resolves to the same source
//! span in the same file are presumed to be the same error and are
//! grouped together. We deliberately do **not** include the code or
//! title in the grouping key — the source location alone is the
//! relation's primary key (decision recorded in
//! `claude-notes/plans/2026-05-22-theme-diagnostic-epic.md`).
//!
//! If two unrelated checks ever land at the same span this is a
//! design risk; the v1 cost (one merged emission with a possibly
//! mixed-content representative) is low. We will widen the key to
//! `(location, code)` if it turns out to bite.
//!
//! # File identity is the resolved path, not the raw `FileId`
//!
//! A raw `FileId` is only globally meaningful when it is hash-based
//! (path-derived, e.g. `quarto_yaml::file_id_for_filename`).
//! Sequential per-context ids are not: every document's primary file
//! is `FileId(0)` in its own [`SourceContext`], so keying on the raw
//! id would falsely merge diagnostics from different files that
//! happen to sit at identical byte offsets.
//!
//! Each input entry carries its own `Option<SourceContext>`, so the
//! group key resolves the file component through it: if the
//! location's `FileId` is registered in the entry's context, the key
//! is the registered file **path**; otherwise it falls back to the
//! raw id (hash-based ids that aren't registered in per-document
//! contexts stay stable and collision-safe). The two key flavors
//! never compare equal to each other.
//!
//! Both fallback edges fail toward *splitting* groups, never toward
//! false merges:
//!
//! - paths are compared verbatim (no canonicalization), so two
//!   contexts registering the same file under different spellings
//!   (`./_quarto.yml` vs `_quarto.yml`) form two groups;
//! - the same id resolving in one entry's context but not another's
//!   (e.g. one entry has no context at all) forms two groups.
//!
//! # The key is the location's run list
//!
//! A location is keyed by [`SourceInfo::resolved_runs`]: the ordered
//! list of source runs its content maps onto, each run's file resolved
//! through the entry's context as above. That covers every shape the
//! renderer can draw: an `Original` is one run, a `Substring` composes
//! onto its parent, a [`SourceInfo::Concat`] contributes one run per
//! piece (adjacent same-file runs merged), and a `Substring` over a
//! `Concat` — the shape of a diagnostic re-parsed out of a folded
//! multi-line YAML scalar (GH #10) — is sliced through the pieces it
//! covers. Two locations with equal endpoints but different interiors
//! key differently, so the key never merges falsely; a `Concat` whose
//! pieces tile the source contiguously keys the same as the
//! equivalent `Original`.
//!
//! # What does not coalesce
//!
//! Diagnostics whose `location` is `None`, or whose location has no
//! runs (a `Generated` without an `Invocation` anchor; a range that
//! falls outside every piece of a `Concat`), pass through as
//! singleton groups (one entry each).
//!
//! [`SourceInfo::Concat`]: quarto_source_map::SourceInfo::Concat
//! [`SourceInfo::resolved_runs`]: quarto_source_map::SourceInfo::resolved_runs

use std::collections::HashMap;
use std::path::PathBuf;

use quarto_source_map::{SourceContext, SourceInfo};

use crate::diagnostic::{DiagnosticMessage, TextRenderOptions};

/// One entry from a coalesced render summary.
///
/// `affected_files` is in encounter order — the order in which the
/// caller's iterator produced each (path, diagnostic) pair that
/// contributed to this group. Singleton groups (size 1) carry one
/// path; rendered output for them omits the "Affected files:" tail
/// to match the legacy per-page render.
#[derive(Debug, Clone)]
pub struct CoalescedDiagnostic {
    pub representative: DiagnosticMessage,
    pub source_context: Option<SourceContext>,
    pub affected_files: Vec<PathBuf>,
}

/// Maximum number of file names rendered inline in the "Affected
/// files:" tail before switching to "… (and N others)".
///
/// Tunable; v1 sets it small so the typical "hundreds of pages"
/// case stays one line.
pub const AFFECTED_FILES_CAP: usize = 3;

impl CoalescedDiagnostic {
    /// Render the underlying ariadne diagnostic, followed by an
    /// `Affected files:` tail listing up to [`AFFECTED_FILES_CAP`]
    /// of the affected paths and a `(and N others)` count for the
    /// rest. Single-element groups omit the tail.
    pub fn to_text(&self) -> String {
        self.to_text_with_options(&TextRenderOptions::default())
    }

    /// Like [`Self::to_text`] but with explicit render options
    /// (mostly useful in tests, where hyperlinks are disabled for
    /// path-independent assertions).
    pub fn to_text_with_options(&self, opts: &TextRenderOptions) -> String {
        let body = self
            .representative
            .to_text_with_options(self.source_context.as_ref(), opts);
        if self.affected_files.len() <= 1 {
            return body;
        }
        let tail = render_affected_files_tail(&self.affected_files);
        format!("{}\n{}", body, tail)
    }
}

fn render_affected_files_tail(paths: &[PathBuf]) -> String {
    let shown = paths
        .iter()
        .take(AFFECTED_FILES_CAP)
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let remaining = paths.len().saturating_sub(AFFECTED_FILES_CAP);
    if remaining == 0 {
        format!("Affected files: {}", shown)
    } else {
        format!(
            "Affected files: {} (and {} other{})",
            shown,
            remaining,
            if remaining == 1 { "" } else { "s" },
        )
    }
}

/// File component of a [`LocationKey`].
///
/// The two variants never compare equal to each other, so an entry
/// whose id resolves through its context can never collide with one
/// whose id doesn't.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum FileKey {
    /// The location's `FileId` resolved to a file registered in the
    /// entry's own `SourceContext`; identity is the registered path.
    Path(String),
    /// Unresolvable id — the entry has no context, or the id isn't
    /// registered in it. Raw ids are only collision-safe when they
    /// are hash-based (see module docs).
    Raw(usize),
}

/// Canonical, hashable form of a [`SourceInfo`] for grouping.
///
/// The location's [`SourceInfo::resolved_runs`], in order, with each
/// run's file resolved through the entry's own `SourceContext` (see
/// [`FileKey`]). `None` when the location has no runs.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct LocationKey {
    runs: Vec<(FileKey, usize, usize)>,
}

impl LocationKey {
    fn from(info: &SourceInfo, ctx: Option<&SourceContext>) -> Option<Self> {
        let runs = info
            .resolved_runs()?
            .into_iter()
            .map(|run| {
                let file = match ctx.and_then(|c| c.get_file(run.file_id)) {
                    Some(f) => FileKey::Path(f.path.clone()),
                    None => FileKey::Raw(run.file_id.0),
                };
                (file, run.start, run.end)
            })
            .collect();
        Some(LocationKey { runs })
    }
}

/// Group the input by source location and return one
/// [`CoalescedDiagnostic`] per group, in encounter order.
///
/// Inputs without a coalescable location (no `location`, or a
/// location with no resolved runs — see the module docs) pass through
/// as singleton groups in their original order — they always print
/// exactly once.
///
/// The first `(path, diagnostic, source_context)` triple to introduce
/// a given key becomes the group's representative. Later triples
/// only contribute to `affected_files`. This matches the principle
/// that the user sees the first diagnostic they would have seen
/// before, with extra context appended.
pub fn coalesce_by_source<I>(input: I) -> Vec<CoalescedDiagnostic>
where
    I: IntoIterator<Item = (PathBuf, DiagnosticMessage, Option<SourceContext>)>,
{
    let mut groups: Vec<CoalescedDiagnostic> = Vec::new();
    let mut index: HashMap<LocationKey, usize> = HashMap::new();

    for (path, diagnostic, source_context) in input {
        let key = diagnostic
            .location
            .as_ref()
            .and_then(|loc| LocationKey::from(loc, source_context.as_ref()));
        match key {
            Some(k) => match index.get(&k).copied() {
                Some(idx) => {
                    groups[idx].affected_files.push(path);
                }
                None => {
                    let idx = groups.len();
                    index.insert(k, idx);
                    groups.push(CoalescedDiagnostic {
                        representative: diagnostic,
                        source_context,
                        affected_files: vec![path],
                    });
                }
            },
            None => {
                // Non-coalescable: emit as a singleton group at the
                // tail. Do not register in the index, so subsequent
                // identical-but-uncoalescable entries also emit as
                // singletons.
                groups.push(CoalescedDiagnostic {
                    representative: diagnostic,
                    source_context,
                    affected_files: vec![path],
                });
            }
        }
    }

    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::DiagnosticMessageBuilder;
    use quarto_source_map::{By, FileId};
    use std::sync::Arc;

    fn original(file_id: usize, start: usize, end: usize) -> SourceInfo {
        SourceInfo::Original {
            file_id: FileId(file_id),
            start_offset: start,
            end_offset: end,
        }
    }

    fn diag_at(loc: SourceInfo, title: &str) -> DiagnosticMessage {
        DiagnosticMessageBuilder::error(title)
            .with_code("Q-14-1")
            .with_location(loc)
            .problem("…")
            .build()
    }

    #[test]
    fn two_diagnostics_at_the_same_location_collapse() {
        let loc = original(1, 100, 110);
        let input = vec![
            (PathBuf::from("a.qmd"), diag_at(loc.clone(), "T"), None),
            (PathBuf::from("b.qmd"), diag_at(loc.clone(), "T"), None),
        ];
        let groups = coalesce_by_source(input);
        assert_eq!(groups.len(), 1);
        assert_eq!(
            groups[0].affected_files,
            vec![PathBuf::from("a.qmd"), PathBuf::from("b.qmd"),]
        );
    }

    #[test]
    fn different_locations_do_not_collapse() {
        let input = vec![
            (
                PathBuf::from("a.qmd"),
                diag_at(original(1, 100, 110), "T"),
                None,
            ),
            (
                PathBuf::from("b.qmd"),
                diag_at(original(1, 200, 210), "T"),
                None,
            ),
        ];
        let groups = coalesce_by_source(input);
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn different_file_ids_do_not_collapse() {
        let input = vec![
            (
                PathBuf::from("a.qmd"),
                diag_at(original(1, 100, 110), "T"),
                None,
            ),
            (
                PathBuf::from("b.qmd"),
                diag_at(original(2, 100, 110), "T"),
                None,
            ),
        ];
        let groups = coalesce_by_source(input);
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn substring_resolves_to_root_original_and_groups_with_it() {
        // A Substring whose root Original matches another Original
        // must coalesce into the same group — the canonical key is
        // the resolved root.
        let root = original(1, 100, 200);
        let sub = SourceInfo::Substring {
            parent: Arc::new(root.clone()),
            // Offsets relative to parent's text; resolve_byte_range
            // composes them: (fid, parent_start + sub_start,
            // parent_start + sub_end) = (1, 100, 110).
            start_offset: 0,
            end_offset: 10,
        };
        let input = vec![
            (PathBuf::from("a.qmd"), diag_at(root.clone(), "T"), None),
            (PathBuf::from("b.qmd"), diag_at(sub, "T"), None),
        ];
        let groups = coalesce_by_source(input);
        // root resolves to (1, 100, 200); sub resolves to (1, 100,
        // 110). Different end offsets ⇒ different keys ⇒ separate
        // groups. This documents the v1 contract: Substring uses
        // the *composed* offsets, not the parent's offsets.
        assert_eq!(groups.len(), 2);
    }

    /// The folded-scalar shape from GH #10: a `Concat` whose pieces
    /// tile `_quarto.yml` contiguously — verbatim text, a folded
    /// newline+indent replaced by one space, verbatim text.
    fn folded_scalar(file_id: usize) -> SourceInfo {
        SourceInfo::concat(vec![
            (original(file_id, 40, 56), 16),
            (original(file_id, 56, 63), 1),
            (original(file_id, 63, 80), 17),
        ])
    }

    #[test]
    fn same_concat_location_collapses() {
        let concat = folded_scalar(1);
        let input = vec![
            (PathBuf::from("a.qmd"), diag_at(concat.clone(), "T"), None),
            (PathBuf::from("b.qmd"), diag_at(concat, "T"), None),
        ];
        let groups = coalesce_by_source(input);
        assert_eq!(groups.len(), 1);
        assert_eq!(
            groups[0].affected_files,
            vec![PathBuf::from("a.qmd"), PathBuf::from("b.qmd")]
        );
    }

    #[test]
    fn same_substring_over_concat_collapses() {
        // A tag re-parsed out of the folded scalar's second line: the
        // exact shape q2's Q-2-9 carries (GH #10).
        let loc = SourceInfo::substring(folded_scalar(1), 17, 34);
        assert_eq!(
            loc.resolve_byte_range(),
            None,
            "sanity: the old key refused this shape"
        );
        let input: Vec<_> = ["a", "b", "c"]
            .iter()
            .map(|n| {
                (
                    PathBuf::from(format!("{n}.qmd")),
                    diag_at(loc.clone(), "T"),
                    None,
                )
            })
            .collect();
        let groups = coalesce_by_source(input);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].affected_files.len(), 3);
    }

    #[test]
    fn different_substrings_over_the_same_concat_do_not_collapse() {
        let parent = folded_scalar(1);
        let input = vec![
            (
                PathBuf::from("a.qmd"),
                diag_at(SourceInfo::substring(parent.clone(), 0, 16), "open"),
                None,
            ),
            (
                PathBuf::from("a.qmd"),
                diag_at(SourceInfo::substring(parent, 17, 34), "close"),
                None,
            ),
        ];
        let groups = coalesce_by_source(input);
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn same_concat_resolving_to_different_paths_does_not_collapse() {
        // Same piece list, but each entry's context registers the id
        // under a different path: two files, two groups (GH #3's
        // protection, carried over to the run-list key).
        let concat = folded_scalar(1);
        let mut ctx_a = SourceContext::new();
        ctx_a.add_file_with_id(FileId(1), "a/_quarto.yml".into(), Some(String::new()));
        let mut ctx_b = SourceContext::new();
        ctx_b.add_file_with_id(FileId(1), "b/_quarto.yml".into(), Some(String::new()));
        let input = vec![
            (
                PathBuf::from("a.qmd"),
                diag_at(concat.clone(), "T"),
                Some(ctx_a),
            ),
            (PathBuf::from("b.qmd"), diag_at(concat, "T"), Some(ctx_b)),
        ];
        let groups = coalesce_by_source(input);
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn equal_endpoints_with_different_interiors_do_not_collapse() {
        // Two gapped Concats sharing start and end but not the gap: an
        // endpoint-only key (option 1 in GH #10) would merge them; the
        // run-list key keeps them apart.
        let a = SourceInfo::concat(vec![(original(1, 0, 10), 10), (original(1, 20, 30), 10)]);
        let b = SourceInfo::concat(vec![(original(1, 0, 5), 5), (original(1, 25, 30), 5)]);
        let input = vec![
            (PathBuf::from("a.qmd"), diag_at(a, "T"), None),
            (PathBuf::from("b.qmd"), diag_at(b, "T"), None),
        ];
        let groups = coalesce_by_source(input);
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn tiled_concat_collapses_with_the_equivalent_original() {
        // The folded scalar's pieces tile 40..80, so a diagnostic on the
        // whole scalar keys like one located at Original 40..80.
        let input = vec![
            (PathBuf::from("a.qmd"), diag_at(folded_scalar(1), "T"), None),
            (
                PathBuf::from("b.qmd"),
                diag_at(original(1, 40, 80), "T"),
                None,
            ),
        ];
        let groups = coalesce_by_source(input);
        assert_eq!(groups.len(), 1);
    }

    #[test]
    fn cross_file_concat_collapses_when_both_entries_agree() {
        // Notebook-cell shape: pieces in two files. One file resolves
        // through the context (Path), the other does not (Raw); the key
        // is per-run, so identical locations with identical contexts
        // still group.
        let cells = SourceInfo::concat(vec![(original(1, 0, 10), 10), (original(2, 0, 10), 10)]);
        let mk_ctx = || {
            let mut c = SourceContext::new();
            c.add_file_with_id(FileId(1), "cell-1".into(), Some(String::new()));
            c
        };
        let input = vec![
            (
                PathBuf::from("a.ipynb"),
                diag_at(cells.clone(), "T"),
                Some(mk_ctx()),
            ),
            (
                PathBuf::from("b.ipynb"),
                diag_at(cells, "T"),
                Some(mk_ctx()),
            ),
        ];
        let groups = coalesce_by_source(input);
        assert_eq!(groups.len(), 1);
    }

    #[test]
    fn generated_without_anchor_passes_through_as_singleton() {
        let loc = SourceInfo::generated(By::sectionize());
        let input = vec![
            (PathBuf::from("a.qmd"), diag_at(loc.clone(), "T"), None),
            (PathBuf::from("b.qmd"), diag_at(loc, "T"), None),
        ];
        let groups = coalesce_by_source(input);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].affected_files, vec![PathBuf::from("a.qmd")]);
        assert_eq!(groups[1].affected_files, vec![PathBuf::from("b.qmd")]);
    }

    #[test]
    fn diagnostics_without_location_pass_through_as_singletons() {
        let d = DiagnosticMessageBuilder::error("no location")
            .problem("…")
            .build();
        let input = vec![
            (PathBuf::from("a.qmd"), d.clone(), None),
            (PathBuf::from("b.qmd"), d, None),
        ];
        let groups = coalesce_by_source(input);
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn encounter_order_preserved_across_groups() {
        let loc1 = original(1, 100, 110);
        let loc2 = original(1, 200, 210);
        let input = vec![
            (PathBuf::from("a.qmd"), diag_at(loc1.clone(), "T1"), None),
            (PathBuf::from("b.qmd"), diag_at(loc2.clone(), "T2"), None),
            (PathBuf::from("c.qmd"), diag_at(loc1.clone(), "T1"), None),
        ];
        let groups = coalesce_by_source(input);
        assert_eq!(groups.len(), 2);
        // Group order = order of first occurrence.
        assert_eq!(groups[0].representative.title, "T1");
        assert_eq!(
            groups[0].affected_files,
            vec![PathBuf::from("a.qmd"), PathBuf::from("c.qmd"),]
        );
        assert_eq!(groups[1].representative.title, "T2");
        assert_eq!(groups[1].affected_files, vec![PathBuf::from("b.qmd")]);
    }

    #[test]
    fn first_encounter_supplies_representative_and_context() {
        // The representative is the *first* (path, diagnostic) seen
        // for a given key. Later contributions only add to
        // `affected_files`. The same goes for the SourceContext.
        // Both contexts register the same path for FileId(1), so the
        // entries key identically and merge.
        let loc = original(1, 100, 110);
        let mut ctx_first = SourceContext::new();
        ctx_first.add_file_with_id(FileId(1), "config.yml".into(), Some("first".into()));
        let mut ctx_second = SourceContext::new();
        ctx_second.add_file_with_id(FileId(1), "config.yml".into(), Some("second".into()));

        let input = vec![
            (
                PathBuf::from("a.qmd"),
                diag_at(loc.clone(), "first"),
                Some(ctx_first),
            ),
            (
                PathBuf::from("b.qmd"),
                diag_at(loc.clone(), "second"),
                Some(ctx_second),
            ),
        ];
        let groups = coalesce_by_source(input);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].representative.title, "first");
        let kept = groups[0].source_context.as_ref().expect("context kept");
        assert_eq!(
            kept.get_file(FileId(1)).unwrap().content.as_deref(),
            Some("first"),
            "the group must keep the first entry's SourceContext"
        );
    }

    #[test]
    fn hash_based_id_with_same_path_collapses_across_contexts() {
        // Hash-based ids are registered per-document, but every
        // document's context maps them to the same path — entries
        // must merge into one group in encounter order.
        let hash_id = 0xdeadbeef_usize;
        let loc = original(hash_id, 40, 50);
        let names = ["a", "b", "c"];
        let input: Vec<_> = names
            .iter()
            .map(|n| {
                let mut ctx = SourceContext::new();
                ctx.add_file_with_id(
                    FileId(hash_id),
                    "_quarto.yml".into(),
                    Some("theme: nope".into()),
                );
                (
                    PathBuf::from(format!("{n}.qmd")),
                    diag_at(loc.clone(), "T"),
                    Some(ctx),
                )
            })
            .collect();
        let groups = coalesce_by_source(input);
        assert_eq!(groups.len(), 1);
        assert_eq!(
            groups[0].affected_files,
            vec![
                PathBuf::from("a.qmd"),
                PathBuf::from("b.qmd"),
                PathBuf::from("c.qmd"),
            ]
        );
    }

    #[test]
    fn sequential_id_collision_across_contexts_does_not_collapse() {
        // Regression test for GH #3: every document's primary file is
        // FileId(0) in its own SourceContext. Identical byte offsets
        // in *different* files must not merge.
        let loc = original(0, 10, 20);
        let mut ctx_a = SourceContext::new();
        assert_eq!(
            ctx_a.add_file("a.qmd".into(), Some("contents a".into())),
            FileId(0)
        );
        let mut ctx_b = SourceContext::new();
        assert_eq!(
            ctx_b.add_file("b.qmd".into(), Some("contents b".into())),
            FileId(0)
        );

        let input = vec![
            (
                PathBuf::from("a.qmd"),
                diag_at(loc.clone(), "in a"),
                Some(ctx_a),
            ),
            (
                PathBuf::from("b.qmd"),
                diag_at(loc.clone(), "in b"),
                Some(ctx_b),
            ),
        ];
        let groups = coalesce_by_source(input);
        assert_eq!(groups.len(), 2, "FileId(0) in two contexts is two files");
        assert_eq!(groups[0].representative.title, "in a");
        assert_eq!(groups[0].affected_files, vec![PathBuf::from("a.qmd")]);
        assert_eq!(groups[1].representative.title, "in b");
        assert_eq!(groups[1].affected_files, vec![PathBuf::from("b.qmd")]);
    }

    #[test]
    fn resolvable_and_unresolvable_same_raw_id_do_not_collapse() {
        // Accepted split-not-merge edge (module docs): the same raw
        // id keys as Path in an entry whose context registers it and
        // as Raw in an entry without a context. Path and Raw keys
        // never compare equal, so the entries split.
        let loc = original(7, 10, 20);
        let mut ctx = SourceContext::new();
        ctx.add_file_with_id(FileId(7), "seven.yml".into(), Some("s".into()));

        let input = vec![
            (PathBuf::from("a.qmd"), diag_at(loc.clone(), "T"), Some(ctx)),
            (PathBuf::from("b.qmd"), diag_at(loc.clone(), "T"), None),
        ];
        let groups = coalesce_by_source(input);
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn singleton_group_omits_affected_files_tail() {
        let loc = original(1, 100, 110);
        let input = vec![(PathBuf::from("a.qmd"), diag_at(loc, "T"), None)];
        let groups = coalesce_by_source(input);
        let opts = TextRenderOptions::default().hyperlinks(false);
        let text = groups[0].to_text_with_options(&opts);
        assert!(
            !text.contains("Affected files:"),
            "singleton groups must not emit the affected-files tail:\n{}",
            text
        );
    }

    #[test]
    fn multi_group_below_cap_lists_all_files() {
        let loc = original(1, 100, 110);
        let input = vec![
            (PathBuf::from("a.qmd"), diag_at(loc.clone(), "T"), None),
            (PathBuf::from("b.qmd"), diag_at(loc.clone(), "T"), None),
        ];
        let groups = coalesce_by_source(input);
        let opts = TextRenderOptions::default().hyperlinks(false);
        let text = groups[0].to_text_with_options(&opts);
        assert!(text.contains("Affected files: a.qmd, b.qmd"), "{}", text);
        assert!(
            !text.contains("other"),
            "no '(and N others)' tail expected for ≤ cap:\n{}",
            text
        );
    }

    #[test]
    fn multi_group_above_cap_truncates_and_counts() {
        // AFFECTED_FILES_CAP=3, so 5 files should produce
        // "a.qmd, b.qmd, c.qmd (and 2 others)".
        let loc = original(1, 100, 110);
        let input: Vec<_> = ["a", "b", "c", "d", "e"]
            .iter()
            .map(|n| {
                (
                    PathBuf::from(format!("{n}.qmd")),
                    diag_at(loc.clone(), "T"),
                    None,
                )
            })
            .collect();
        let groups = coalesce_by_source(input);
        let opts = TextRenderOptions::default().hyperlinks(false);
        let text = groups[0].to_text_with_options(&opts);
        assert!(
            text.contains("Affected files: a.qmd, b.qmd, c.qmd (and 2 others)"),
            "{}",
            text,
        );
    }

    #[test]
    fn multi_group_just_above_cap_uses_singular_other() {
        // 4 files at cap=3 ⇒ 1 other (singular).
        let loc = original(1, 100, 110);
        let input: Vec<_> = ["a", "b", "c", "d"]
            .iter()
            .map(|n| {
                (
                    PathBuf::from(format!("{n}.qmd")),
                    diag_at(loc.clone(), "T"),
                    None,
                )
            })
            .collect();
        let groups = coalesce_by_source(input);
        let opts = TextRenderOptions::default().hyperlinks(false);
        let text = groups[0].to_text_with_options(&opts);
        assert!(
            text.contains("(and 1 other)"),
            "expected singular 'other' for exactly 1 over cap:\n{}",
            text,
        );
    }
}
