---
title: "Generics and Constants"
author: stable-mir-json
---

# Generics and Constants

How the pipeline handles generic functions,
trait-associated constants, and const generic parameters.

<!-- end_slide -->

# Quick glossary

<!-- pause -->

**Monomorphization**: the compiler's process of turning generic
code into concrete code. If you write `fn foo<T>` and call it
with `i32` and `String`, the compiler produces two separate
copies: `foo::<i32>` and `foo::<String>`. Each copy is a
"mono item" (see the previous deck).

<!-- pause -->

**Associated constant**: a constant defined inside a trait impl.

```rust
trait Stride { const STEP: usize; }
impl Stride for By2 { const STEP: usize = 2; }
```

`By2::STEP` is an associated constant with value `2`.

<!-- pause -->

**Const generic**: a generic parameter that's a value, not a type.

```rust
fn make_array<const N: usize>() -> [u8; N]
```

`N` is filled in at compile time: `make_array::<4>()`.

<!-- end_slide -->

# The program

```rust
trait Stride {
    const STEP: usize;
}

struct By2;
impl Stride for By2 {
    const STEP: usize = 2;
}

struct By3;
impl Stride for By3 {
    const STEP: usize = 3;
}

fn advance<S: Stride>(pos: usize) -> usize {
    pos + S::STEP
}

fn make_array<const N: usize>() -> [u8; N] {
    [0u8; N]
}

fn main() {
    let a = advance::<By2>(0);
    let b = advance::<By3>(10);
    assert_eq!(a, 2);
    assert_eq!(b, 13);

    let arr = make_array::<4>();
    assert_eq!(arr.len(), 4);
}
```

<!-- end_slide -->

# Phase 1: Find all the functions

The compiler finds **16 concrete functions**.
The interesting part: generics get split.

<!-- pause -->

From **our** code:

```
  advance::<By2>       one copy for By2
  advance::<By3>       another copy for By3
  make_array::<4>      N filled in with 4
  main
```

<!-- pause -->

One generic function `advance<S>` produced two mono items.
The compiler literally duplicated the function body, once
for each concrete type it was called with.

`make_array<const N>` also got its own concrete copy
with `N = 4`.

<!-- end_slide -->

# What happened to `S::STEP`?

In the source, `advance` reads a trait constant:

```rust
fn advance<S: Stride>(pos: usize) -> usize {
    pos + S::STEP
}
```

<!-- pause -->

You might expect the pipeline to see "unevaluated constant:
`S::STEP`" and then resolve it. But by the time we see the
MIR, the compiler has **already done the math**.

<!-- pause -->

In the concrete body of `advance::<By3>`, `S::STEP` is
just the number `3`: eight bytes of memory holding the
value `[3, 0, 0, 0, 0, 0, 0, 0]`.

There is nothing left to evaluate. The pipeline never
encounters an "unevaluated constant" event for this
program.

<!-- end_slide -->

<!-- column_layout: [1, 1] -->

<!-- column: 0 -->

### Reading `advance::<By3>`

```rust {15-17}
trait Stride {
    const STEP: usize;
}

struct By2;
impl Stride for By2 {
    const STEP: usize = 2;
}

struct By3;
impl Stride for By3 {
    const STEP: usize = 3;
}

fn advance<S: Stride>(pos: usize) -> usize {
    pos + S::STEP
}

fn make_array<const N: usize>() -> [u8; N] {
    [0u8; N]
}

fn main() {
    let a = advance::<By2>(0);
    let b = advance::<By3>(10);
    assert_eq!(a, 2);
    assert_eq!(b, 13);

    let arr = make_array::<4>();
    assert_eq!(arr.len(), 4);
}
```

Line 16: `pos + S::STEP`

`S::STEP` is already the number `3`
in this concrete copy of the function.

<!-- column: 1 -->

### What the pipeline finds

```
  Started reading: advance::<By3>
    types: 47, allocs: 0
```

<!-- pause -->

```
  Found constant value: usize
    (this is the "3"; no pointers in it,
     so alloc count stays at 0)

  Found constant value: usize
    (the overflow check comparison)

  Found type: bool
    (the overflow check result)

  Found type: ()
    (the "return nothing" type for the
     panic path if overflow happens)
```

<!-- pause -->

```
  Finished reading: advance::<By3>
    new types:  +2
    new allocs:  0
    new spans:  +5
```

No function calls here; just arithmetic.

<!-- end_slide -->

<!-- column_layout: [1, 1] -->

<!-- column: 0 -->

### Reading `advance::<By2>`

```rust {15-17}
trait Stride {
    const STEP: usize;
}

struct By2;
impl Stride for By2 {
    const STEP: usize = 2;
}

struct By3;
impl Stride for By3 {
    const STEP: usize = 3;
}

fn advance<S: Stride>(pos: usize) -> usize {
    pos + S::STEP
}

fn make_array<const N: usize>() -> [u8; N] {
    [0u8; N]
}

fn main() {
    let a = advance::<By2>(0);
    let b = advance::<By3>(10);
    assert_eq!(a, 2);
    assert_eq!(b, 13);

    let arr = make_array::<4>();
    assert_eq!(arr.len(), 4);
}
```

Same function body, different constant value.
`S::STEP` is `2` instead of `3`.

<!-- column: 1 -->

### What the pipeline finds

```
  Started reading: advance::<By2>
    types: 55, allocs: 0
```

<!-- pause -->

```
  Found constant value: usize
  Found constant value: usize
```

<!-- pause -->

```
  Finished reading: advance::<By2>
    new types:   0
    new allocs:  0
    new spans:   0
```

**Zero new types, zero new spans.**

The By3 copy already recorded `bool`, `()`,
`usize`, and all the source locations for this
function body. Since both copies share the same
source code, the second one adds nothing new.

This is deduplication at work: the pipeline
records each type and span only once.

<!-- end_slide -->

<!-- column_layout: [1, 1] -->

<!-- column: 0 -->

### Reading `main`: the calls

```rust {24-26}
trait Stride {
    const STEP: usize;
}

struct By2;
impl Stride for By2 {
    const STEP: usize = 2;
}

struct By3;
impl Stride for By3 {
    const STEP: usize = 3;
}

fn advance<S: Stride>(pos: usize) -> usize {
    pos + S::STEP
}

fn make_array<const N: usize>() -> [u8; N] {
    [0u8; N]
}

fn main() {
    let a = advance::<By2>(0);
    let b = advance::<By3>(10);
    assert_eq!(a, 2);
    assert_eq!(b, 13);

    let arr = make_array::<4>();
    assert_eq!(arr.len(), 4);
}
```

Two calls to `advance`, each with a different
type argument. By now the compiler has already
resolved each to a separate concrete function.

<!-- column: 1 -->

### What the pipeline finds

```
  Started reading: main
    functions: 13, types: 56
```

<!-- pause -->

```
  Found function call:       (line 25)
    advance::<By2>

  Found constant value: usize
    (the argument 0)
```

<!-- pause -->

```
  Found function call:       (line 26)
    advance::<By3>

  Found constant value: usize
    (the argument 10)
```

The two calls look the same in structure
but resolve to different concrete functions.

<!-- end_slide -->

<!-- column_layout: [1, 1] -->

<!-- column: 0 -->

### Reading `main`: the const generic

```rust {30}
trait Stride {
    const STEP: usize;
}

struct By2;
impl Stride for By2 {
    const STEP: usize = 2;
}

struct By3;
impl Stride for By3 {
    const STEP: usize = 3;
}

fn advance<S: Stride>(pos: usize) -> usize {
    pos + S::STEP
}

fn make_array<const N: usize>() -> [u8; N] {
    [0u8; N]
}

fn main() {
    let a = advance::<By2>(0);
    let b = advance::<By3>(10);
    assert_eq!(a, 2);
    assert_eq!(b, 13);

    let arr = make_array::<4>();
    assert_eq!(arr.len(), 4);
}
```

`make_array::<4>()`: the const generic `N = 4`
shows up as an already-computed value in the
function's type information.

<!-- column: 1 -->

### The const generic in the trace

```
  Found function call:       (line 35)
    make_array::<4>
```

The function's generic arguments show `N`
as an already-evaluated value:

```
  GenericArgs([
    Const(Value(
      type: usize,
      bytes: [4, 0, 0, 0, ...]
    ))
  ])
```

No symbolic "N" left; just the number 4
stored as eight bytes.

<!-- pause -->

```
  Found type: [u8; 4]
    (a concrete array type;
     the const generic made it concrete)
```

<!-- end_slide -->

<!-- column_layout: [1, 1] -->

<!-- column: 0 -->

### Reading `make_array::<4>`

```rust {19-21}
trait Stride {
    const STEP: usize;
}

struct By2;
impl Stride for By2 {
    const STEP: usize = 2;
}

struct By3;
impl Stride for By3 {
    const STEP: usize = 3;
}

fn advance<S: Stride>(pos: usize) -> usize {
    pos + S::STEP
}

fn make_array<const N: usize>() -> [u8; N] {
    [0u8; N]
}

fn main() {
    let a = advance::<By2>(0);
    let b = advance::<By3>(10);
    assert_eq!(a, 2);
    assert_eq!(b, 13);

    let arr = make_array::<4>();
    assert_eq!(arr.len(), 4);
}
```

`[0u8; N]` where `N = 4`.

<!-- column: 1 -->

### What the pipeline finds

```
  Started reading: make_array::<4>
    types: 61, allocs: 3
```

<!-- pause -->

```
  Found constant value: u8     (line 26)
    (the repeat value 0u8)
```

<!-- pause -->

```
  Finished reading: make_array::<4>
    new types:   0
    new allocs:  0
    new spans:  +4
```

Nothing new except source locations. The `u8`
and `[u8; 4]` types were already recorded when
we read `main` (which references the return type).

<!-- end_slide -->

# Phase 3: Assemble the output

All 16 function bodies have been read.

```
  functions:    17
  allocations:   3
  types:        61
  source spans: 145
```

<!-- pause -->

3 allocations: all from `assert_eq!` macro expansions,
which create compile-time `&usize` references for
comparing values.

The trait constants (`S::STEP = 2`, `S::STEP = 3`)
showed up as constant values in the MIR, but they
had no pointer content, so they didn't produce
tracked allocations.

<!-- end_slide -->

# The constants story

```
    In your source             What the pipeline sees
  ---------------------       -------------------------

  S::STEP                     the literal number 3
  (associated constant)       (already evaluated)

  make_array::<4>             GenericArgs([Value(4)])
  (const generic param)       (already evaluated)

  [0u8; N]                    [0u8; 4]
  (const in expression)       (already concrete)
```

<!-- pause -->

All three patterns are resolved by the compiler **before**
the pipeline ever sees them. The code has a path for
handling "unevaluated constants" (constants the compiler
hasn't computed yet), but it was never triggered here.

<!-- pause -->

The trace confirms this: zero "unevaluated constant
discovered" events in the entire output.

<!-- end_slide -->

# Recap

What the compiler does to generic code before the pipeline sees it:

<!-- pause -->

**1. Makes separate copies for each concrete use**
`advance<S>` becomes `advance::<By2>` and `advance::<By3>`,
each with its own function body.

<!-- pause -->

**2. Evaluates all constants ahead of time**
`S::STEP` becomes a literal number in memory.
`const N: usize` becomes the value `4` in the generic args.

<!-- pause -->

**3. Shares what it can**
The second `advance` copy contributes zero new types or
source spans; only the first copy pays the cost.

<!-- pause -->

The trace makes all of this visible.
