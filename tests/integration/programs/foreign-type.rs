//! @covers: Foreign type kind (extern type used behind a pointer)
#![feature(extern_types)]

extern "C" {
    type Opaque;
}

fn main() {
    // Create a raw pointer to a Foreign type.
    // We never dereference it; we just need the type to be collected.
    let ptr: *const Opaque = core::ptr::null::<u8>() as *const Opaque;
    assert!(ptr.is_null());
}
