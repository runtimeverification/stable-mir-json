use std::env;
use std::collections::VecDeque;

fn main() {
    let mut args: VecDeque<_> = env::args().collect();
    let prog_name = args.pop_front().unwrap();
    
    // do the actual work here...

    println!("Program: {prog_name}, Args: {args:?}");
}