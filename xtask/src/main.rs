//! Local CI runner: `cargo xtask verify`.
//!
//! Runs exactly the checks `.github/workflows/ci.yml` runs, in the same
//! order, so a green `verify` means a green CI (modulo the OS matrix —
//! this runs on your host only). Keep the `STEPS` list below in sync with
//! the workflow.
//!
//! Usage:
//!   cargo xtask verify     # run the full gate
//!   cargo xtask            # same (verify is the default)

use std::process::{Command, exit};

/// One CI step: a human label and the cargo args to run.
struct Step {
    label: &'static str,
    args: &'static [&'static str],
}

/// Mirror of `.github/workflows/ci.yml`. The `lint` job's checks run first
/// (they are the fastest to fail and the easiest to fix), then the `test`
/// job's. `clippy`/`build`/`test` pass `--locked` like CI; `-D warnings`
/// makes clippy warnings fail the build, matching CI.
const STEPS: &[Step] = &[
    // --- lint job ---
    Step {
        label: "rustfmt (cargo fmt --all --check)",
        args: &["fmt", "--all", "--check"],
    },
    Step {
        label: "clippy (default features)",
        args: &[
            "clippy",
            "--all-targets",
            "--locked",
            "--",
            "-D",
            "warnings",
        ],
    },
    Step {
        label: "clippy (--all-features)",
        args: &[
            "clippy",
            "--all-targets",
            "--all-features",
            "--locked",
            "--",
            "-D",
            "warnings",
        ],
    },
    // --- test job ---
    Step {
        label: "build (default features)",
        args: &["build", "--all-targets", "--locked"],
    },
    Step {
        label: "test (default features)",
        args: &["test", "--locked"],
    },
    Step {
        label: "test (--all-features)",
        args: &["test", "--all-features", "--locked"],
    },
];

fn main() {
    let task = std::env::args().nth(1).unwrap_or_else(|| "verify".into());
    if task != "verify" {
        eprintln!("unknown task: {task:?}\n\nusage: cargo xtask [verify]");
        exit(2);
    }

    // Cargo sets $CARGO to the cargo binary that invoked us; fall back to
    // the one on PATH when run directly.
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    // Run from the workspace root (this crate's parent) so the cargo
    // commands resolve the workspace regardless of the invocation dir.
    let workspace_root = {
        let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.pop();
        p
    };

    let total = STEPS.len();
    for (i, step) in STEPS.iter().enumerate() {
        println!("\n\x1b[1m[{}/{}] {}\x1b[0m", i + 1, total, step.label);
        let status = Command::new(&cargo)
            .args(step.args)
            .current_dir(&workspace_root)
            .status()
            .unwrap_or_else(|e| {
                eprintln!("failed to launch `{cargo}`: {e}");
                exit(1);
            });
        if !status.success() {
            let code = status.code().unwrap_or(1);
            eprintln!(
                "\n\x1b[31mxtask verify failed at step {}/{}: {}\x1b[0m",
                i + 1,
                total,
                step.label
            );
            exit(code);
        }
    }

    println!("\n\x1b[32mxtask verify: all {total} checks passed\x1b[0m");
}
