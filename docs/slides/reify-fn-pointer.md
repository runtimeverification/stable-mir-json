---
title: "From Rust Source to SMIR JSON"
author: stable-mir-json
---

# From Rust Source to SMIR JSON

Tracing how stable-mir-json reads a Rust program
and builds its JSON output, step by step.

<!-- end_slide -->

# Quick glossary

A few terms that show up repeatedly in the trace:

<!-- pause -->

**MIR** (Mid-level IR): the compiler's intermediate representation.
Think of it as typed assembly with structured control flow.
Each function compiles down to a MIR "body."

<!-- pause -->

**Mono item**: a concrete function after all generics have been
filled in. `Vec::<i32>::push` is a mono item; `Vec::<T>::push`
is not. Even non-generic functions like `main` are mono items
(they just have zero type parameters to fill in).

<!-- pause -->

**Allocation**: a chunk of compile-time-known memory: a string
literal, a static variable, a constant value, a vtable.

**Span**: a region of source code (a file + start/end positions).

<!-- end_slide -->

# A few more terms

**Terminator**: MIR splits function bodies into "basic blocks"
(straight-line sequences of instructions). Each block ends with
a terminator: a function call, a return, a branch, or a drop.

That's why function calls appear under "visit terminator"
in the trace: the call is the last thing in its block.

<!-- pause -->

**Function item vs. function pointer**: In Rust, the name `add`
has a unique zero-sized type (a "function item"). Writing
`let f: fn(i32,i32)->i32 = add` converts that item into a
generic function pointer. The compiler inserts a cast
to do this; the trace calls it a "fn-pointer coercion."

<!-- end_slide -->

# The program

```rust
fn add(a: i32, b: i32) -> i32 {
    a + b
}

fn apply(f: fn(i32, i32) -> i32, x: i32, y: i32) -> i32 {
    f(x, y)
}

fn main() {
    let f: fn(i32, i32) -> i32 = add;
    let result = apply(f, 3, 4);
    assert_eq!(result, 7);
}
```

Three functions. One function-pointer coercion. One indirect call.
What does stable-mir-json make of this?

<!-- end_slide -->

# Phase 1: Find all the functions

The compiler walks the crate and finds every concrete function
that would need to be compiled ("mono items").

<!-- pause -->

From **our** code:

```
  add, apply, main
```

<!-- pause -->

From **the standard library** (pulled in transitively):

```
  lang_start, lang_start closure, __rust_begin_short_backtrace,
  FnOnce::call_once (x2), call_once vtable shim, assert_failed,
  drop_in_place<&i32>, drop_in_place<closure>, Debug::fmt (x2),
  Termination::report
```

<!-- pause -->

**15 concrete functions total** from a 12-line program.
Most are runtime plumbing you never see in your source.

<!-- end_slide -->

# Phase 2: Walk each function body

For each function, the pipeline reads through its MIR body
and records what it finds:

<!-- pause -->

| The pipeline sees... | ...and records |
|---|---|
| A function call | Which function is being called |
| A fn-pointer coercion | The function being converted to a pointer |
| A constant value | Any compile-time memory it references |
| A type | The type and its memory layout |
| A source location | The file/line/col span |

<!-- pause -->

Each finding is logged as a **trace event** with a source
location: the exact range in the original `.rs` file.

Let's watch what happens when the pipeline walks `main`.

<!-- end_slide -->

<!-- column_layout: [1, 1] -->

<!-- column: 0 -->

### Source: `main`

```rust {10}
fn add(a: i32, b: i32) -> i32 {
    a + b
}

fn apply(f: fn(i32, i32) -> i32, x: i32, y: i32) -> i32 {
    f(x, y)
}

fn main() {
    let f: fn(i32, i32) -> i32 = add;
    let result = apply(f, 3, 4);
    assert_eq!(result, 7);
}
```

Line 10: the name `add` has a unique zero-sized
type. Assigning it to a `fn(...)` pointer means
the compiler inserts a **coercion** (a cast from
function item to function pointer).

<!-- column: 1 -->

### Output so far

```json
{
  "functions": [],
  "types": [],
  "allocs": []
}
```

<!-- pause -->

The pipeline sees the coercion and records `add`
as a function that needs to appear in the output:

```
  Found fn-pointer coercion:
    add -> fn(i32, i32) -> i32

  add also used as a value
    (passed around, not just called)
```

```json
{
  "functions": [
    { "name": "add", "via": "fn-pointer" }
  ]
}
```

<!-- end_slide -->

<!-- column_layout: [1, 1] -->

<!-- column: 0 -->

### Source: `main`

```rust {11}
fn add(a: i32, b: i32) -> i32 {
    a + b
}

fn apply(f: fn(i32, i32) -> i32, x: i32, y: i32) -> i32 {
    f(x, y)
}

fn main() {
    let f: fn(i32, i32) -> i32 = add;
    let result = apply(f, 3, 4);
    assert_eq!(result, 7);
}
```

Line 11: a direct function call.
The pipeline resolves which concrete function
is being called and adds it to the output.

<!-- column: 1 -->

### Output so far

```json
{
  "functions": [
    { "name": "add", "via": "fn-pointer" }
  ]
}
```

<!-- pause -->

```
  Found function call:
    apply (direct call)

  apply also used as a value
```

```json
{
  "functions": [
    { "name": "add",   "via": "fn-pointer" },
    { "name": "apply", "via": "call" }
  ]
}
```

<!-- end_slide -->

<!-- column_layout: [1, 1] -->

<!-- column: 0 -->

### Source: `main`

```rust {12}
fn add(a: i32, b: i32) -> i32 {
    a + b
}

fn apply(f: fn(i32, i32) -> i32, x: i32, y: i32) -> i32 {
    f(x, y)
}

fn main() {
    let f: fn(i32, i32) -> i32 = add;
    let result = apply(f, 3, 4);
    assert_eq!(result, 7);
}
```

Line 12: `assert_eq!` is a macro. The compiler
expands it into a call to a panic function
(`assert_failed`) plus some compile-time memory
for the comparison operands.

<!-- column: 1 -->

### Output so far

```json
{
  "functions": [
    { "name": "add",   "via": "fn-pointer" },
    { "name": "apply", "via": "call" }
  ],
  "allocs": []
}
```

<!-- pause -->

```
  Found function call:
    assert_failed<i32, i32>

  Found compile-time memory:
    &i32 reference  (0 -> 1 allocs)
```

```json
{
  "functions": [
    { "name": "add",           "via": "fn-ptr" },
    { "name": "apply",         "via": "call" },
    { "name": "assert_failed", "via": "call" }
  ],
  "allocs": [ { "ty": "&i32" } ]
}
```

<!-- end_slide -->

# Walking `main`: the tally

After reading through `main`'s body:

| What | Before | After |
|---|---|---|
| functions | 0 | 3 |
| allocations | 0 | 1 |
| types | 0 | 39 |
| source spans | 0 | 22 |

<!-- pause -->

39 types from a 12-line function? Most come from `assert_eq!`.
The macro pulls in formatting machinery (`fmt::Arguments`,
`fmt::Formatter`, `Option<usize>`, `&str`, `&dyn Debug`, ...)
and the pipeline records every type it touches.

<!-- end_slide -->

<!-- column_layout: [1, 1] -->

<!-- column: 0 -->

### Walking `add`

```rust {1-3}
fn add(a: i32, b: i32) -> i32 {
    a + b
}

fn apply(f: fn(i32, i32) -> i32, x: i32, y: i32) -> i32 {
    f(x, y)
}

fn main() {
    let f: fn(i32, i32) -> i32 = add;
    let result = apply(f, 3, 4);
    assert_eq!(result, 7);
}
```

No calls, no allocations. Just `a + b`.

<!-- column: 1 -->

### What the pipeline finds

```
  Started reading: add
```

<!-- pause -->

```
  Found type: ()
    (the empty tuple; every function that
     might panic has a diverging path that
     "returns" this)

  Found 5 source spans
    (the function signature, the body,
     the closing brace)
```

<!-- pause -->

```
  Finished reading: add
    new types:  +1
    new spans:  +5
```

A simple function contributes very little.

<!-- end_slide -->

<!-- column_layout: [1, 1] -->

<!-- column: 0 -->

### Walking `apply`

```rust {5-7}
fn add(a: i32, b: i32) -> i32 {
    a + b
}

fn apply(f: fn(i32, i32) -> i32, x: i32, y: i32) -> i32 {
    f(x, y)
}

fn main() {
    let f: fn(i32, i32) -> i32 = add;
    let result = apply(f, 3, 4);
    assert_eq!(result, 7);
}
```

`f(x, y)` calls through a function pointer.
The pipeline sees a call instruction, but the
target is a variable (`f`), not a known function.

It can't resolve which function will be called
at runtime, so: **no new function recorded.**

<!-- column: 1 -->

### What the pipeline finds

```
  Started reading: apply
```

<!-- pause -->

```
  Found 6 source spans
    (signature, body, closing brace)
```

<!-- pause -->

```
  Finished reading: apply
    new functions:  0
    new types:      0
    new spans:     +6
```

No new functions, no new types.
Everything about `fn(i32,i32)->i32` was
already recorded when we walked `main`.

<!-- end_slide -->

# Phase 3: Assemble the output

All 15 function bodies have been read. The pipeline
writes everything it found into the `.smir.json` file.

<!-- pause -->

```
  functions:    16
  allocations:   1
  types:        57
  source spans: 115
```

<!-- pause -->

Why 16 functions when there are 15 mono items?
Because `add` was discovered two ways: once as a mono
item, and once through its fn-pointer coercion. Both
paths create an entry.

57 types and 115 spans come mostly from standard library
code that `assert_eq!` and the runtime entry point pull in.

<!-- end_slide -->

# The trace file

Every step we just walked through is recorded in a
trace file. To generate it:

```
TRACE=1 cargo run -- -Zno-codegen program.rs
```

This produces a `*.smir.trace.json` alongside the
normal output.

<!-- pause -->

Each event in the trace carries three things:

- **What** happened (found a function call, found a type, ...)
- **Which function body** was being read
- **Where in the source** it happened (file, line, column range)

<!-- pause -->

The source range is what makes the side-by-side
visualization possible: highlight the code on the left,
show the growing output on the right.

<!-- end_slide -->

# Recap

```
  Source         Phase 1          Phase 2          Phase 3
                find all the     read each        write the
  .rs  -------> functions -----> body ----------> .smir.json
                 (15 found)      |  |  |
                                 |  |  +- types
                                 |  +---- allocations
                                 +------- function calls
```

<!-- pause -->

A 12-line Rust program produces:
**16 functions, 1 allocation, 57 types, 115 spans.**

Most of the complexity is invisible: standard library
plumbing, macro expansions, trait method copies.

The trace makes all of it visible.
