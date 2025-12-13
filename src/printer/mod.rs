use std::io::Write;
use std::{fs::File, io};

extern crate rustc_middle;
extern crate rustc_session;
extern crate serde_json;

use rustc_middle::ty::TyCtxt;
use rustc_session::config::{OutFileName, OutputType};

// Macros must be defined before module declarations (textual scoping)
macro_rules! def_env_var {
    ($fn_name:ident, $var_name:ident) => {
        fn $fn_name() -> bool {
            use std::sync::OnceLock;
            static VAR: OnceLock<bool> = OnceLock::new();
            *VAR.get_or_init(|| std::env::var(stringify!($var_name)).is_ok())
        }
    };
}

def_env_var!(debug_enabled, DEBUG);
def_env_var!(link_items_enabled, LINK_ITEMS);
def_env_var!(link_instance_enabled, LINK_INST);

macro_rules! debug_log_println {
    ($($args:tt)*) => {
        #[cfg(feature = "debug_log")]
        println!($($args)*);
    };
}

mod collect;
mod items;
mod link_map;
mod mir_visitor;
mod schema;
mod ty_visitor;
mod types;
mod uneval;
mod util;

// Re-exports preserving the public API
pub use collect::collect_smir;
pub use schema::{AllocInfo, FnSymType, Item, LinkMapKey, MonoItemKind, SmirJson};
pub use types::TypeMetadata;
pub use util::has_attr;

pub fn emit_smir(tcx: TyCtxt<'_>) {
    let smir_json =
        serde_json::to_string(&collect_smir(tcx)).expect("serde_json failed to write result");

    match tcx.output_filenames(()).path(OutputType::Mir) {
        OutFileName::Stdout => {
            write!(&io::stdout(), "{}", smir_json).expect("Failed to write smir.json");
        }
        OutFileName::Real(path) => {
            let mut b = io::BufWriter::new(
                File::create(path.with_extension("smir.json"))
                    .expect("Failed to create {path}.smir.json output file"),
            );
            write!(b, "{}", smir_json).expect("Failed to write smir.json");
        }
    }
}
