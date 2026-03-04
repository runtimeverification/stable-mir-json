/// Exercises const evaluation patterns: associated constants in generic
/// contexts and const generic parameters. On the current nightly, all
/// constants are eagerly evaluated during monomorphization (appearing as
/// Allocated or Ty(Value(...)) in stable MIR), so the
/// ConstantKind::Unevaluated path in visit_mir_const is not triggered.
/// This test still exercises the const-adjacent pipeline paths.
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
