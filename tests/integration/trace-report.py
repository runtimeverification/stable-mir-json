"""Aggregate trace event coverage and report gaps.

Called by trace-report.sh after TRACE=1 runs have been collected.
Usage: trace-report.py <trace_dir> <test_dir>
"""

import json
import glob
import os
import re
import sys
from collections import Counter

trace_dir = sys.argv[1]
test_dir = sys.argv[2] if len(sys.argv) > 2 else None

# ── Collect events ──────────────────────────────────────────────────

counts = {}
by_program = {}      # event -> { program -> count }
sub_values = {}      # event -> field -> Counter(value -> count)
sub_programs = {}    # event -> field -> value -> set(programs)

SUB_FIELDS = ["ty_kind", "sym_kind", "source"]

# Track whether a sub-category value was seen via user code (not just stdlib).
# Key structure mirrors sub_programs: event -> field -> value -> set(programs).
sub_user_programs = {}

files = sorted(glob.glob(trace_dir + "/*.smir.trace.json"))
if not files:
    print("No trace files found. Is TRACE=1 working?")
    sys.exit(1)

for f in files:
    prog = os.path.basename(f).replace(".smir.trace.json", "")
    # Trace events now use readable (demangled) item names, e.g. "main",
    # "test_binop", "main::{closure#0}".  Stdlib items start with "std::",
    # "core::", "alloc::", or "<" (trait impl shims).
    for e in json.load(open(f)):
        ev = e["event"]
        counts[ev] = counts.get(ev, 0) + 1
        by_program.setdefault(ev, {})
        by_program[ev][prog] = by_program[ev].get(prog, 0) + 1
        for key in SUB_FIELDS:
            if key in e:
                sub_values.setdefault(ev, {}).setdefault(key, Counter())
                sub_values[ev][key][e[key]] += 1
                sub_programs.setdefault(ev, {}).setdefault(key, {})
                sub_programs[ev][key].setdefault(e[key], set())
                sub_programs[ev][key][e[key]].add(prog)
                # Check if this event came from user code (not stdlib).
                item = e.get("item", "")
                is_stdlib = (
                    not item
                    or item.startswith("std::")
                    or item.startswith("core::")
                    or item.startswith("alloc::")
                    or item.startswith("<")
                )
                if not is_stdlib:
                    sub_user_programs.setdefault(ev, {}).setdefault(key, {})
                    sub_user_programs[ev][key].setdefault(e[key], set())
                    sub_user_programs[ev][key][e[key]].add(prog)

n = len(files)

# ── Event summary ───────────────────────────────────────────────────

print(f"Trace coverage across {n} test(s):\n")
print(f"  {'Count':>6}  Event")
print(f"  {'-----':>6}  -----")
for k, v in sorted(counts.items(), key=lambda x: -x[1]):
    progs = by_program.get(k, {})
    np = len(progs)
    if np == n:
        detail = "all programs"
    elif np <= 3:
        detail = ", ".join(sorted(progs.keys()))
    else:
        detail = f"{np}/{n} programs"
    print(f"  {v:6}  {k}  ({detail})")

# ── Coverage vs invariant classification ────────────────────────────

coverage_events = [
    "ItemDiscovered", "BodyWalkStarted", "BodyWalkFinished",
    "FunctionCallResolved", "DropGlueResolved", "ReifyFnPointerResolved",
    "AllocationCollected", "FnDefAsValue",
    "TypeCollected", "SpanResolved", "AssemblyStarted",
]

invariant_events = [
    "UnevaluatedConstDiscovered",
]

uncovered = [e for e in coverage_events if e not in counts]
violated = [e for e in invariant_events if counts.get(e, 0) > 0]

if uncovered:
    print(f"\nUncovered ({len(uncovered)}):")
    for e in uncovered:
        print(f"  ** {e}")

if violated:
    print(f"\nInvariant violations ({len(violated)}):")
    for e in violated:
        print(f"  !! {e}: {counts[e]} occurrence(s)")

held = [e for e in invariant_events if counts.get(e, 0) == 0]
if held:
    print(f"\nInvariants holding ({len(held)}):")
    for e in held:
        print(f"  ok {e}")

if not uncovered and not violated:
    print("\nAll coverage events exercised, all invariants holding.")

# ── Sub-category breakdown ──────────────────────────────────────────

# Known universe of values for each (event, field) pair, classified as
# either "actionable" (could be covered with a test) or "unreachable"
# (structurally impossible or requires unstable/exotic features).
# Each entry maps a value to a reason string (None = actionable).
KNOWN_UNIVERSE = {
    ("TypeCollected", "ty_kind"): {
        # actionable
        "Bool": None, "Char": None, "Int": None, "Uint": None, "Float": None,
        "Adt": None, "Str": None, "Array": None, "Slice": None,
        "RawPtr": None, "Ref": None, "Dynamic": None, "Never": None, "Tuple": None,
        "FnDef": None, "FnPtr": None, "Closure": None,
        "Foreign": None,
        # unreachable or exotic
        "Coroutine": "async/generators monomorphize to ADT state machines",
        "CoroutineWitness": "async/generators monomorphize to ADT state machines",
        "Pat": "unstable feature (pattern_types)",
    },
    ("FunctionCallResolved", "sym_kind"): {
        "normal": None, "intrinsic": None,
        "no_op": "no-op shims only arise from drop glue, not call terminators",
    },
    ("DropGlueResolved", "sym_kind"): {
        "no_op": None,
        "normal": "drop glue is always a no-op shim",
        "intrinsic": "drop glue is always a no-op shim",
    },
    ("ReifyFnPointerResolved", "sym_kind"): {
        "normal": None,
        "intrinsic": "intrinsics cannot be coerced to fn pointers",
        "no_op": "no-op shims cannot be coerced to fn pointers",
    },
    ("ItemDiscovered", "source"): {
        "mono_collect": None,
        "unevaluated_const": "compiler eagerly evaluates all consts on current nightly",
    },
}

print("\n" + "=" * 60)
print("Sub-category breakdown:")
print("=" * 60)

for ev in sorted(sub_values):
    for key in sorted(sub_values[ev]):
        counter = sub_values[ev][key]
        universe_key = (ev, key)
        universe = KNOWN_UNIVERSE.get(universe_key)

        print(f"\n  {ev} / {key}:")
        for val, count in counter.most_common():
            progs = sub_programs[ev][key][val]
            np = len(progs)
            if np == n:
                detail = "all"
            elif np <= 3:
                detail = ", ".join(sorted(progs))
            else:
                detail = f"{np}/{n}"
            # Annotate provenance: user code, stdlib, or both.
            user_progs = sub_user_programs.get(ev, {}).get(key, {}).get(val, set())
            if not user_progs:
                detail += ", stdlib only"
            elif len(user_progs) < np:
                detail += ", user + stdlib"
            else:
                detail += ", user code"
            print(f"    {count:6}  {val}  ({detail})")

        if universe:
            actionable = [v for v in universe if v not in counter and universe[v] is None]
            unreachable = [(v, universe[v]) for v in universe if v not in counter and universe[v] is not None]
            if actionable:
                print(f"    gaps (actionable):")
                for v in actionable:
                    print(f"      ** {v}")
            if unreachable:
                print(f"    gaps (unreachable):")
                for v, reason in unreachable:
                    print(f"      -- {v}  ({reason})")

# ── Per-program annotations ─────────────────────────────────────────

covers = {}
if test_dir:
    for rs in sorted(glob.glob(test_dir + "/*.rs")):
        prog = os.path.basename(rs).replace(".rs", "")
        with open(rs) as f:
            for line in f:
                m = re.match(r"//[!/]\s*@covers:\s*(.+)", line)
                if m:
                    covers[prog] = m.group(1).strip()
                    break

if covers:
    print(f"\nTest programs ({len(covers)}/{n} annotated):\n")
    for prog in sorted(covers):
        events = []
        for ev in sorted(by_program):
            if prog in by_program[ev]:
                events.append(ev)
        print(f"  {prog}")
        print(f"    covers: {covers[prog]}")
        unique = [ev for ev in events if len(by_program[ev]) == 1]
        if unique:
            print(f"    unique: {', '.join(unique)}")
    unannotated = sorted(set(
        os.path.basename(f).replace(".smir.trace.json", "")
        for f in files
    ) - set(covers))
    if unannotated:
        print(f"\n  Missing @covers annotation:")
        for p in unannotated:
            print(f"    ** {p}")
