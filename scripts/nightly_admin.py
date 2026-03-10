#!/usr/bin/env python3
"""Manage nightly toolchains for stable-mir-json.

Formalizes the manual workflow for adding, checking, and bumping nightly
toolchains into three composable subcommands. Each step that was previously
a sequence of shell commands and file edits is now a single invocation.

Usage:
    python3 scripts/nightly_admin.py add   nightly-YYYY-MM-DD --rust-dir /path/to/rust
    python3 scripts/nightly_admin.py check nightly-YYYY-MM-DD --rust-dir /path/to/rust
    python3 scripts/nightly_admin.py bump  nightly-YYYY-MM-DD
"""

from __future__ import annotations

import argparse
import dataclasses
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

REPO_ROOT = Path(__file__).resolve().parent.parent
RUST_TOOLCHAIN_TOML = REPO_ROOT / "rust-toolchain.toml"
README_MD = REPO_ROOT / "README.md"
DIFF_SCRIPT = REPO_ROOT / "tests" / "ui" / "diff_test_lists.sh"
GOLDEN_BASE = REPO_ROOT / "tests" / "integration" / "expected"
OVERRIDE_BASE = REPO_ROOT / "tests" / "ui" / "overrides"
MAKEFILE = REPO_ROOT / "Makefile"

NIGHTLY_RE = re.compile(r"^nightly-(\d{4})-(\d{2})-(\d{2})$")
COMPONENTS = ["llvm-tools", "rustc-dev", "rust-src", "rust-analyzer"]


# ---------------------------------------------------------------------------
# Data model
# ---------------------------------------------------------------------------

@dataclasses.dataclass(frozen=True)
class NightlySpec:
    """A validated nightly toolchain identifier with derived paths."""

    channel: str  # e.g. "nightly-2025-08-01"
    date: str     # e.g. "2025-08-01"

    @property
    def golden_dir(self) -> Path:
        return GOLDEN_BASE / self.channel

    @property
    def override_dir(self) -> Path:
        return OVERRIDE_BASE / self.channel

    @property
    def override_tsv(self) -> Path:
        return OVERRIDE_BASE / f"{self.channel}.tsv"


# ---------------------------------------------------------------------------
# Utilities
# ---------------------------------------------------------------------------

def log(msg: str) -> None:
    print(f"  {msg}", flush=True)


def log_step(msg: str) -> None:
    print(f"\n>> {msg}", flush=True)


def log_warn(msg: str) -> None:
    print(f"  WARNING: {msg}", file=sys.stderr, flush=True)


def die(msg: str) -> None:
    print(f"Error: {msg}", file=sys.stderr)
    sys.exit(1)


def validate_nightly(raw: str) -> NightlySpec:
    """Parse and validate a nightly channel string."""
    m = NIGHTLY_RE.match(raw)
    if not m:
        die(f"invalid nightly format: {raw!r} (expected nightly-YYYY-MM-DD)")
    date = f"{m.group(1)}-{m.group(2)}-{m.group(3)}"
    return NightlySpec(channel=raw, date=date)


def run(
    cmd: list[str],
    *,
    check: bool = True,
    capture: bool = False,
    env: dict[str, str] | None = None,
    cwd: Path | None = None,
) -> subprocess.CompletedProcess[str]:
    """Run a subprocess, forwarding output unless capture is requested."""
    merged_env = {**os.environ, **(env or {})}
    kwargs: dict = dict(
        cwd=cwd or REPO_ROOT,
        env=merged_env,
    )
    if capture:
        kwargs["stdout"] = subprocess.PIPE
        kwargs["stderr"] = subprocess.PIPE
        kwargs["text"] = True
    result = subprocess.run(cmd, **kwargs)
    if check and result.returncode != 0:
        die(f"command failed (exit {result.returncode}): {' '.join(cmd)}")
    return result


def toolchain_env(nightly: str) -> dict[str, str]:
    """Return env dict that forces a specific toolchain."""
    return {"RUSTUP_TOOLCHAIN": nightly}


def read_file(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def write_file(path: Path, content: str) -> None:
    path.write_text(content, encoding="utf-8")


def count_lines(path: Path) -> int:
    """Count non-empty, non-comment lines in a TSV file."""
    if not path.exists():
        return 0
    return sum(
        1 for line in path.read_text().splitlines()
        if line.strip() and not line.startswith("#")
    )


def current_pin() -> str:
    """Read the currently pinned nightly from rust-toolchain.toml."""
    content = read_file(RUST_TOOLCHAIN_TOML)
    m = re.search(r'channel\s*=\s*"([^"]+)"', content)
    if not m:
        die("could not parse channel from rust-toolchain.toml")
    return m.group(1)


# ---------------------------------------------------------------------------
# DEFAULT_NIGHTLIES management
# ---------------------------------------------------------------------------

def read_default_nightlies() -> list[str]:
    """Parse DEFAULT_NIGHTLIES from diff_test_lists.sh."""
    content = read_file(DIFF_SCRIPT)
    m = re.search(r'DEFAULT_NIGHTLIES="([^"]*)"', content)
    if not m:
        die("could not parse DEFAULT_NIGHTLIES from diff_test_lists.sh")
    return m.group(1).split()


def write_default_nightlies(nightlies: list[str]) -> None:
    """Rewrite the DEFAULT_NIGHTLIES line, keeping sorted order."""
    content = read_file(DIFF_SCRIPT)
    sorted_list = sorted(nightlies)
    new_line = f'DEFAULT_NIGHTLIES="{" ".join(sorted_list)}"'
    updated = re.sub(r'DEFAULT_NIGHTLIES="[^"]*"', new_line, content)
    write_file(DIFF_SCRIPT, updated)


# ---------------------------------------------------------------------------
# Override TSV management
# ---------------------------------------------------------------------------

def find_most_recent_override_tsv(before: NightlySpec) -> Path | None:
    """Find the most recent override .tsv file with a date before `before`."""
    candidates: list[tuple[str, Path]] = []
    for f in OVERRIDE_BASE.glob("nightly-*.tsv"):
        if f.is_file() and NIGHTLY_RE.match(f.stem):
            m = NIGHTLY_RE.match(f.stem)
            if m:
                fdate = f"{m.group(1)}-{m.group(2)}-{m.group(3)}"
                if fdate < before.date:
                    candidates.append((fdate, f))
    if not candidates:
        return None
    candidates.sort(key=lambda x: x[0])
    return candidates[-1][1]


def create_override_tsv(spec: NightlySpec) -> Path:
    """Create an override .tsv for the given nightly, templated from the most
    recent existing one."""
    if spec.override_tsv.exists():
        log(f"override TSV already exists: {spec.override_tsv.relative_to(REPO_ROOT)}")
        return spec.override_tsv

    template = find_most_recent_override_tsv(spec)
    if template is not None:
        content = read_file(template)
        # Update the header comment's nightly reference
        old_nightly = template.stem
        content = content.replace(old_nightly, spec.channel, 1)
        write_file(spec.override_tsv, content)
        log(f"created override TSV (from {template.name})")
    else:
        header = (
            f"# Manual overrides for {spec.channel}\n"
            f"# Format: action<TAB>path[<TAB>extra]\n"
            f"#\n"
            f"# Actions:\n"
            f"#   -      remove from passing list\n"
            f"#   +      add to passing list\n"
            f"#   skip   remove from passing list (alias for -)\n"
            f"#   fail   move to failing list (extra = expected exit code)\n"
            f"#   pass   move from failing to passing list\n"
        )
        write_file(spec.override_tsv, header)
        log("created skeleton override TSV (no prior template found)")

    return spec.override_tsv


# ---------------------------------------------------------------------------
# Subcommand: add
# ---------------------------------------------------------------------------

def cmd_add(args: argparse.Namespace) -> None:
    """Add support for a new nightly toolchain."""
    spec = validate_nightly(args.nightly)
    rust_dir = Path(args.rust_dir).resolve()
    if not rust_dir.is_dir():
        die(f"RUST_DIR_ROOT is not a directory: {rust_dir}")

    print(f"Adding nightly: {spec.channel}")
    env = toolchain_env(spec.channel)

    # 1. Install toolchain
    log_step("Installing toolchain")
    install_cmd = ["rustup", "toolchain", "install", spec.channel]
    run(install_cmd)
    run(["rustup", "component", "add", *COMPONENTS, "--toolchain", spec.channel])

    # 2. Build
    if not args.skip_build:
        log_step("Building with target toolchain")
        result = run(["cargo", "build"], env=env, check=False)
        if result.returncode != 0:
            die(
                "build failed; you may need to:\n"
                "  - Add compat code in src/printer/ or src/mk_graph/\n"
                "  - Add a new breakpoint in build.rs\n"
                "  - See docs/nightly-compat.md for the full playbook"
            )

    # 3. Generate golden files
    log_step("Generating integration test golden files")
    if spec.golden_dir.exists() and not args.force:
        log(f"golden files already exist: {spec.golden_dir.relative_to(REPO_ROOT)}")
    else:
        spec.golden_dir.mkdir(parents=True, exist_ok=True)
        run(["make", "golden"], env=env)
    n_golden = len(list(spec.golden_dir.glob("*.expected")))
    log(f"{n_golden} golden files in {spec.golden_dir.relative_to(REPO_ROOT)}")

    # 4. Generate UI test effective lists
    if not args.skip_ui:
        log_step("Generating UI test effective lists")
        run(
            ["make", "test-ui-emit"],
            env={**env, "RUST_DIR_ROOT": str(rust_dir), "NIGHTLY": spec.channel},
        )
        n_pass = count_lines(spec.override_dir / "passing.tsv")
        n_fail = count_lines(spec.override_dir / "failing.tsv")
        log(f"effective lists: {n_pass} passing, {n_fail} failing")

    # 5. Create override TSV
    log_step("Creating override TSV")
    create_override_tsv(spec)

    # 6. Add to DEFAULT_NIGHTLIES
    log_step("Updating DEFAULT_NIGHTLIES")
    nightlies = read_default_nightlies()
    if spec.channel in nightlies:
        log("already in DEFAULT_NIGHTLIES")
    else:
        nightlies.append(spec.channel)
        write_default_nightlies(nightlies)
        log(f"added {spec.channel} to DEFAULT_NIGHTLIES")

    # 7. Report
    print(f"\nDone. Next steps:")
    print(f"  - Review {spec.override_tsv.relative_to(REPO_ROOT)} for accuracy")
    print(f"  - Run: python3 scripts/nightly_admin.py check {spec.channel} --rust-dir {rust_dir}")


# ---------------------------------------------------------------------------
# Subcommand: check
# ---------------------------------------------------------------------------

def cmd_check(args: argparse.Namespace) -> None:
    """Run all tests for a nightly toolchain."""
    spec = validate_nightly(args.nightly)
    rust_dir = Path(args.rust_dir).resolve()
    if not rust_dir.is_dir():
        die(f"RUST_DIR_ROOT is not a directory: {rust_dir}")

    print(f"Checking nightly: {spec.channel}")
    env = toolchain_env(spec.channel)

    # Pre-flight
    if not spec.golden_dir.exists():
        die(
            f"no golden files for {spec.channel}; run "
            f"'python3 scripts/nightly_admin.py add {spec.channel} --rust-dir {rust_dir}' first"
        )

    results: list[tuple[str, bool]] = []

    # 1. Build
    if not args.skip_build:
        log_step("Build")
        r = run(["cargo", "build"], env=env, check=False)
        results.append(("Build", r.returncode == 0))
        if r.returncode != 0:
            log("FAIL")
        else:
            log("PASS")

    # 2. Integration tests
    log_step("Integration tests")
    r = run(["make", "integration-test"], env=env, check=False)
    results.append(("Integration tests", r.returncode == 0))
    log("PASS" if r.returncode == 0 else "FAIL")

    # 3. UI tests
    if spec.override_dir.exists():
        log_step("UI tests")
        r = run(
            ["make", "test-ui"],
            env={**env, "RUST_DIR_ROOT": str(rust_dir)},
            check=False,
        )
        results.append(("UI tests", r.returncode == 0))
        log("PASS" if r.returncode == 0 else "FAIL")
    else:
        log_step("UI tests (skipped: no effective lists)")
        results.append(("UI tests", True))

    # 4. Directive parser tests
    log_step("Directive parser tests")
    r = run(["make", "test-directives"], check=False)
    results.append(("Directive tests", r.returncode == 0))
    log("PASS" if r.returncode == 0 else "FAIL")

    # Summary
    print(f"\n{'=' * 40}")
    print(f"Nightly check: {spec.channel}\n")
    all_pass = True
    for name, passed in results:
        status = "PASS" if passed else "FAIL"
        print(f"  {name:<25} {status}")
        if not passed:
            all_pass = False
    print(f"\nOverall: {'PASS' if all_pass else 'FAIL'}")
    sys.exit(0 if all_pass else 1)


# ---------------------------------------------------------------------------
# Subcommand: bump
# ---------------------------------------------------------------------------

def cmd_bump(args: argparse.Namespace) -> None:
    """Bump the pinned nightly in rust-toolchain.toml and README.md."""
    spec = validate_nightly(args.nightly)
    old_pin = current_pin()

    print(f"Bumping pinned nightly: {old_pin} -> {spec.channel}")

    warnings: list[str] = []
    if not spec.golden_dir.exists():
        warnings.append(
            f"no golden files for {spec.channel}; "
            f"run 'python3 scripts/nightly_admin.py add {spec.channel} ...' first"
        )
    if not spec.override_dir.exists():
        warnings.append(
            f"no UI test effective lists for {spec.channel}; "
            f"run 'python3 scripts/nightly_admin.py add {spec.channel} ...' first"
        )
    if spec.channel < old_pin:
        warnings.append(
            f"target nightly {spec.channel} is older than current pin {old_pin}"
        )

    for w in warnings:
        log_warn(w)

    if args.dry_run:
        print("\nDry run; no files modified.")
        return

    # Update rust-toolchain.toml
    content = read_file(RUST_TOOLCHAIN_TOML)
    updated = re.sub(r'channel\s*=\s*"[^"]*"', f'channel = "{spec.channel}"', content)
    write_file(RUST_TOOLCHAIN_TOML, updated)
    log(f"updated rust-toolchain.toml: {old_pin} -> {spec.channel}")

    # Update README.md
    readme = read_file(README_MD)
    new_readme = re.sub(
        r"Pinned nightly: `[^`]*`",
        f"Pinned nightly: `{spec.channel}`",
        readme,
    )
    new_readme = re.sub(
        r"through `[^`]*`",
        f"through `{spec.channel}`",
        new_readme,
    )
    if new_readme == readme:
        log_warn("README.md pinned nightly callout not found; manual update needed")
    else:
        write_file(README_MD, new_readme)
        log(f"updated README.md pinned nightly and supported range")

    # Report
    print(f"\nDone. Next steps:")
    print(f"  - Run: cargo build")
    print(f"  - Run: python3 scripts/nightly_admin.py check {spec.channel} --rust-dir ...")
    print(f"  - Commit the changes")


# ---------------------------------------------------------------------------
# CLI entry point
# ---------------------------------------------------------------------------

def main() -> None:
    parser = argparse.ArgumentParser(
        prog="nightly_admin",
        description="Manage nightly toolchains for stable-mir-json.",
    )
    sub = parser.add_subparsers(dest="command", required=True)

    # --- add ---
    p_add = sub.add_parser("add", help="Add support for a new nightly")
    p_add.add_argument("nightly", help="Toolchain channel (e.g. nightly-2025-08-01)")
    p_add.add_argument("--rust-dir", required=True, help="Path to rust-lang/rust checkout")
    p_add.add_argument("--skip-build", action="store_true", help="Skip build verification")
    p_add.add_argument("--skip-ui", action="store_true", help="Skip UI test list generation")
    p_add.add_argument("--force", action="store_true", help="Regenerate existing artifacts")

    # --- check ---
    p_check = sub.add_parser("check", help="Run all tests for a nightly")
    p_check.add_argument("nightly", help="Toolchain channel")
    p_check.add_argument("--rust-dir", required=True, help="Path to rust-lang/rust checkout")
    p_check.add_argument("--skip-build", action="store_true", help="Skip build step")

    # --- bump ---
    p_bump = sub.add_parser("bump", help="Bump the pinned nightly")
    p_bump.add_argument("nightly", help="Toolchain channel to pin to")
    p_bump.add_argument("--dry-run", action="store_true", help="Show what would change")

    args = parser.parse_args()
    dispatch = {"add": cmd_add, "check": cmd_check, "bump": cmd_bump}
    dispatch[args.command](args)


if __name__ == "__main__":
    main()
