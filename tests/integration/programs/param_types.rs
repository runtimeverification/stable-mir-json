// scratch/param_types.rs

struct WithParam<T> {
    the_t: T,
    another: usize,
}

fn main() {
    let a: WithParam<u32> = WithParam{the_t: 42, another: 42};

    let b: WithParam<u64> = WithParam{the_t: 42, another: 42};

    let c: Result<u8, usize> = Err(a.another);
    let d : Result<u64, u8> = Ok(b.the_t);

    let x = c.err().unwrap();

    assert!(x as u64 == d.unwrap());
}
