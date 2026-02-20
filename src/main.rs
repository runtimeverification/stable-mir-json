#![feature(rustc_private)]
use stable_mir_json::driver::stable_mir_driver;
use stable_mir_json::mk_graph::{emit_d2file, emit_dotfile};
use stable_mir_json::printer::emit_smir;
use std::env;

fn main() {
    let mut args: Vec<String> = env::args().collect();

    match args.get(1) {
        None => stable_mir_driver(&args, emit_smir), // backward compatibility
        Some(arg) if arg == "--json" => {
            args.remove(1);
            stable_mir_driver(&args, emit_smir)
        }
        Some(arg) if arg == "--dot" => {
            args.remove(1);
            stable_mir_driver(&args, emit_dotfile)
        }
        Some(arg) if arg == "--d2" => {
            args.remove(1);
            stable_mir_driver(&args, emit_d2file)
        }
        Some(_other) => stable_mir_driver(&args, emit_smir), // backward compatibility
    }
}
