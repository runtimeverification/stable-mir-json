struct Pair {
    x: i32,
    y: i32,
}

#[inline(never)]
fn a_pair() -> Pair {
    Pair { x: 2, y: 3 }
}

fn main() {
    let pair = a_pair();
    let code = pair.x;
    std::process::exit(code)
}
