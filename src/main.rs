#![feature(rustc_private)]
use std::env;
pub mod driver;
pub mod printer;
pub mod pretty;
use driver::stable_mir_driver;
use printer::print_all_items;

fn main() {
    let args: Vec<_> = env::args().collect();
    stable_mir_driver(&args, print_all_items)
}
