//! @covers: static trait objects, dyn dispatch, vtable allocation
use std::fmt::Debug;

static S: u8 = 7;
const OBJS: [&'static dyn Debug; 1] = [&S as &dyn Debug];

fn main() {
    // Keep trait-object constant usage so both Static and VTable allocs are emitted.
    std::hint::black_box(OBJS[0]);
}
