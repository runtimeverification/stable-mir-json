//! Build script: detect the active rustc nightly's commit-date and emit
//! `cargo:rustc-cfg` flags so that the rest of the crate can gate match
//! arms (and other code) on stable MIR API changes.
//!
//! The approach: run `rustc -vV`, parse `commit-date: YYYY-MM-DD`, and
//! compare against a table of known API breakpoints. ISO date strings
//! sort lexicographically, so `>=` comparison is correct.

use std::process::Command;

/// A single API breakpoint: the date at which the change landed, the
/// cfg flag to emit, and a human-readable description (for the table
/// below; not used at runtime beyond the cargo warning).
struct Breakpoint {
    date: &'static str,
    cfg: &'static str,
    description: &'static str,
}

/// Known stable MIR API breakpoints.
///
/// Naming convention:
///   `smir_has_<thing>` for additions
///   `smir_no_<thing>`  for removals
///
/// Keep this table sorted by date.
const BREAKPOINTS: &[Breakpoint] = &[
    Breakpoint {
        date: "2024-12-14",
        cfg: "smir_has_coroutine_closure",
        description: "AggregateKind::CoroutineClosure added",
    },
    Breakpoint {
        date: "2025-01-24",
        cfg: "smir_has_run_compiler_fn",
        description: "RunCompiler struct replaced by run_compiler() free function",
    },
    Breakpoint {
        date: "2025-01-27",
        cfg: "smir_has_named_mono_item_partitions",
        description: "MonoItemPartitions changed from tuple to named fields",
    },
    Breakpoint {
        date: "2025-01-28",
        cfg: "smir_has_raw_ptr_kind",
        description: "Rvalue::AddressOf changed from Mutability to RawPtrKind",
    },
    Breakpoint {
        date: "2025-07-04",
        cfg: "smir_no_indexed_val",
        description: "IndexedVal trait became pub(crate), to_index()/to_val() unavailable",
    },
    Breakpoint {
        date: "2025-07-07",
        cfg: "smir_rustc_internal_moved",
        description: "rustc_internal::{internal,stable,run} moved from rustc_smir to stable_mir",
    },
    Breakpoint {
        date: "2025-07-10",
        cfg: "smir_has_global_alloc_typeid",
        description: "GlobalAlloc::TypeId { ty } variant added",
    },
    Breakpoint {
        date: "2025-07-14",
        cfg: "smir_crate_renamed",
        description: "stable_mir -> rustc_public, rustc_smir -> rustc_public_bridge",
    },
];

fn main() {
    // Re-run when the toolchain changes.
    println!("cargo:rerun-if-env-changed=RUSTUP_TOOLCHAIN");
    println!("cargo:rerun-if-changed=rust-toolchain.toml");

    let commit_date = detect_commit_date();
    eprintln!("build.rs: rustc commit-date: {commit_date}");

    for bp in BREAKPOINTS {
        // Unconditionally declare the cfg so that rustc doesn't warn about
        // `unexpected_cfgs` on nightlies where the flag isn't active.
        println!("cargo:rustc-check-cfg=cfg({})", bp.cfg);

        if commit_date.as_str() >= bp.date {
            println!("cargo:rustc-cfg={}", bp.cfg);
            eprintln!(
                "build.rs:   enabled cfg `{}` (>= {}): {}",
                bp.cfg, bp.date, bp.description
            );
        }
    }
}

/// Run `rustc -vV` and extract the `commit-date` field.
fn detect_commit_date() -> String {
    let output = Command::new("rustc")
        .args(["-vV"])
        .output()
        .expect("failed to run `rustc -vV`");

    let stdout = String::from_utf8(output.stdout).expect("rustc -vV produced non-UTF-8 output");

    stdout
        .lines()
        .find_map(|line| line.strip_prefix("commit-date: "))
        .expect("rustc -vV output missing `commit-date` field")
        .trim()
        .to_string()
}
