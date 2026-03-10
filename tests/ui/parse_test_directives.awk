# parse_test_directives.awk: extract rustc UI test directives from a .rs file.
#
# Scans for //@ directives and decides whether the test should be skipped on
# the current host, and extracts compile-flags/edition/rustc-env for passing
# to the compiler.
#
# Input variables (pass via -v):
#   host_os     - "linux", "macos", "windows", etc.
#   host_arch   - "aarch64", "x86_64", etc.
#   host_bits   - "32" or "64"
#   universal   - if non-empty, only apply platform-independent skips
#                 (needs-sanitizer, needs-subprocess, extern crate libc).
#                 Use this when generating lists that must be correct on
#                 any host; platform-specific filtering happens at runtime.
#
# Output (printed to stdout):
#   If the test should be skipped:  SKIP<TAB>reason
#   Otherwise:                      FLAGS<TAB>flags-string
#
# The FLAGS line may have an empty flags string if no compile-flags were found.

BEGIN {
    skip = ""
    flags = ""

    # Map host_os to the set of OS names this host satisfies.
    # "unix" covers linux, macos, freebsd, etc.  "apple" covers macos.
    is_unix  = (host_os == "linux" || host_os == "macos" || host_os == "freebsd" || host_os == "openbsd" || host_os == "netbsd" || host_os == "dragonfly" || host_os == "solaris" || host_os == "illumos" || host_os == "android")
    is_apple = (host_os == "macos")
}

# Tests that use `extern crate libc` fail with E0464 (multiple candidates)
# because our sysroot ships both .rmeta and .rlib for libc. This is an
# environment limitation of running rustc directly outside cargo.
/extern[[:space:]]+crate[[:space:]]+libc/ { skip = "extern-crate-libc" }

# Only process //@ directive lines.
/^[[:space:]]*\/\/@[[:space:]]/ {
    # Strip leading whitespace and the //@ prefix to get the directive.
    dir = $0
    sub(/^[[:space:]]*\/\/@[[:space:]]*/, "", dir)

    # Detect and strip optional [revision] prefix (e.g., "[x64]only-x86_64").
    has_revision = 0
    if (match(dir, /^\[.*\]/)) {
        has_revision = 1
        sub(/^\[.*\]/, "", dir)
    }

    # --- Skip directives ---
    # Each of these sets skip="reason" if the test shouldn't run here.
    # Platform-specific skips are only evaluated when universal is not set;
    # in universal mode, run_ui_tests.sh handles these at runtime.

    if (!universal) {
        # only-<os>: skip unless we match
        if (match(dir, /^only-linux/))       { if (host_os != "linux")   skip = "only-linux" }
        if (match(dir, /^only-windows/))     { if (host_os != "windows") skip = "only-windows" }
        if (match(dir, /^only-macos/))       { if (host_os != "macos")   skip = "only-macos" }
        if (match(dir, /^only-unix/))        { if (!is_unix)             skip = "only-unix" }
        if (match(dir, /^only-apple/))       { if (!is_apple)            skip = "only-apple" }
        if (match(dir, /^only-msvc/))        { if (host_os != "windows") skip = "only-msvc" }

        # only-<arch>: skip unless we match
        if (match(dir, /^only-x86_64/))      { if (host_arch != "x86_64")  skip = "only-x86_64" }
        if (match(dir, /^only-x86$/) || match(dir, /^only-x86[^_]/)) { if (host_arch != "x86_64" && host_arch != "i686") skip = "only-x86" }
        if (match(dir, /^only-aarch64/))     { if (host_arch != "aarch64") skip = "only-aarch64" }

        # only-<bits>: skip unless we match
        if (match(dir, /^only-32bit/))       { if (host_bits != "32") skip = "only-32bit" }
        if (match(dir, /^only-64bit/))       { if (host_bits != "64") skip = "only-64bit" }

        # ignore-<os>: skip if we match
        if (match(dir, /^ignore-linux/))     { if (host_os == "linux")   skip = "ignore-linux" }
        if (match(dir, /^ignore-windows/))   { if (host_os == "windows") skip = "ignore-windows" }
        if (match(dir, /^ignore-macos/))     { if (host_os == "macos")   skip = "ignore-macos" }
        if (match(dir, /^ignore-apple/))     { if (is_apple)             skip = "ignore-apple" }
        if (match(dir, /^ignore-unix/))      { if (is_unix)              skip = "ignore-unix" }

        # ignore-<arch>: skip if we match
        if (match(dir, /^ignore-x86_64/))    { if (host_arch == "x86_64")  skip = "ignore-x86_64" }
        if (match(dir, /^ignore-aarch64/))   { if (host_arch == "aarch64") skip = "ignore-aarch64" }
    }

    # needs-sanitizer-*: we don't have sanitizer support in our test setup
    if (match(dir, /^needs-sanitizer/))  { skip = "needs-sanitizer" }

    # needs-subprocess: test forks/execs the compiled binary; we run with
    # -Zno-codegen so there is no binary to execute.
    if (match(dir, /^needs-subprocess/)) { skip = "needs-subprocess" }

    # --- Flag extraction directives ---
    # Only extract flags from non-revision-gated directives. Revision-gated
    # flags (e.g., [x32]compile-flags: -Ctarget-feature=+sse2) are for a
    # specific revision; applying all of them at once produces conflicts
    # (e.g., --edition 2018 and --edition 2021 simultaneously).
    if (!has_revision) {
        # compile-flags: append everything after the colon
        if (match(dir, /^compile-flags:[[:space:]]*/)) {
            val = dir
            sub(/^compile-flags:[[:space:]]*/, "", val)
            flags = flags " " val
        }

        # edition: append --edition <value>
        # Range syntax (e.g., "2015..2021") is a compiletest feature that
        # runs the test once per edition in the range. We use the earliest
        # edition, since every test in the range must compile with it and
        # later editions may reject deprecated syntax the test exercises.
        if (match(dir, /^edition:[[:space:]]*/)) {
            val = dir
            sub(/^edition:[[:space:]]*/, "", val)
            # Take first word only
            sub(/[[:space:]].*/, "", val)
            # Range edition: "2015..2021" or "2015..=2021"; extract the start
            if (match(val, /\.\./)) {
                val = substr(val, 1, RSTART - 1)
            }
            # Validate: edition must be a 4-digit year or "future"
            if (val == "" || (!match(val, /^[0-9][0-9][0-9][0-9]$/) && val != "future")) {
                printf "WARNING: unrecognized edition value \"%s\" from directive: %s (in %s)\n", val, dir, FILENAME > "/dev/stderr"
            } else {
                flags = flags " --edition " val
            }
        }

        # rustc-env: append --env-set <KEY=VALUE> -Zunstable-options
        if (match(dir, /^rustc-env:[[:space:]]*/)) {
            val = dir
            sub(/^rustc-env:[[:space:]]*/, "", val)
            sub(/[[:space:]].*/, "", val)
            if (val != "") flags = flags " --env-set " val " -Zunstable-options"
        }
    }
}

END {
    if (skip != "") {
        print "SKIP\t" skip
    } else {
        # Trim leading space from flags
        sub(/^[[:space:]]+/, "", flags)
        print "FLAGS\t" flags
    }
}
