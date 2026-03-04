//! @covers: ReifyFnPointer cast (fn item coerced to fn pointer via type annotation)
fn add(a: i32, b: i32) -> i32 {
    a + b
}

fn apply(f: fn(i32, i32) -> i32, x: i32, y: i32) -> i32 {
    f(x, y)
}

fn main() {
    // This assignment coerces the fn item `add` into a `fn(i32, i32) -> i32`
    // pointer, producing a ReifyFnPointer cast in MIR.
    let f: fn(i32, i32) -> i32 = add;
    let result = apply(f, 3, 4);
    assert_eq!(result, 7);
}
