//! @covers: tuple construction, tuple field access
fn main() {
    let tup:(i32, i32) = (42, 99);
    
    assert!(tup.0 != tup.1);
}