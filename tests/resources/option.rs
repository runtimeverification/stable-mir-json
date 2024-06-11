#[inline(never)]
fn an_option() -> Option<i32> {
    Some(2)
}

fn main() {
    let code: i32 = 0;
    let code = if let Some(code) = an_option() { code } else { code };
    std::process::exit(code)
}
